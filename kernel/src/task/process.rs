//! `Process` — the shared-resource container (corresponds to rCore `proc.rs`).

use super::init::{
    self, ElfExt, ProcInitInfo, AT_BASE, AT_ENTRY, AT_PAGESZ, AT_PHDR, AT_PHENT, AT_PHNUM,
};
use super::{Futex, Tid};
use crate::consts::{USER_STACK_OFFSET, USER_STACK_SIZE};
use crate::fs::{FileHandle, FileLike, OpenOptions};
use crate::ipc::{SemProc, ShmProc};
use crate::memory::{ByFrame, Delay, GlobalFrameAlloc, MemoryAttr, MemorySet};
use crate::signal::{Siginfo, Signal, SignalAction, Sigset};
use crate::sync::{Event, EventBus, SpinNoIrqLock as Mutex};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::fmt;
use core::mem::MaybeUninit;
use log::*;
use rcore_fs::vfs::INode;
use rcore_memory::{Page, PAGE_SIZE};
use spin::RwLock;
use xmas_elf::{header, ElfFile};

/// Process id type. Equals the tid of the process's first (main) thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pid(pub usize);

impl Pid {
    pub const INIT: usize = 1;

    pub fn new() -> Self {
        Pid(0)
    }

    pub fn get(&self) -> usize {
        self.0
    }

    /// Whether this pid represents the init process.
    pub fn is_init(&self) -> bool {
        self.0 == Self::INIT
    }
}

impl fmt::Display for Pid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Process group id type.
pub type Pgid = i32;

/// A process: the set of resources shared by its threads.
pub struct Process {
    /// Virtual memory (address space). Shared with each `Thread.vm`.
    pub vm: Arc<Mutex<MemorySet>>,

    /// Opened files (the file descriptor table).
    pub files: BTreeMap<usize, FileLike>,

    /// Current working directory.
    pub cwd: String,

    /// Path of the executable.
    pub exec_path: String,

    /// Futexes keyed by user-space address.
    pub futexes: BTreeMap<usize, Arc<Futex>>,

    /// System V semaphores owned by this process.
    pub semaphores: SemProc,

    /// Process id (i.e. tgid), usually the tid of the first thread.
    pub pid: Pid,

    /// Process group id.
    pub pgid: Pgid,

    /// Parent process (pid kept out of the Weak to avoid deadlock).
    pub parent: (Pid, Weak<Mutex<Process>>),

    /// Child processes.
    pub children: Vec<(Pid, Weak<Mutex<Process>>)>,

    /// Threads belonging to this process.
    pub threads: Vec<Tid>,

    /// Event bus for notifications (e.g. process exit).
    pub eventbus: Arc<Mutex<EventBus>>,

    /// Exit code.
    pub exit_code: usize,

    /// Delivered signals; the `isize` is the target tid (-1 = any thread).
    pub sig_queue: VecDeque<(Siginfo, isize)>,
    pub pending_sigset: Sigset,

    /// Signal dispositions (actions).
    pub dispositions: [SignalAction; Signal::RTMAX + 1],

    /// System V shared memory owned by this process.
    pub shm_identifiers: ShmProc,
}

lazy_static! {
    /// Mapping between pid and `Process`.
    pub static ref PROCESSES: RwLock<BTreeMap<usize, Arc<Mutex<Process>>>> =
        RwLock::new(BTreeMap::new());
}

/// Return the process that thread `tid` belongs to.
pub fn process_of(tid: usize) -> Option<Arc<Mutex<Process>>> {
    PROCESSES
        .read()
        .values()
        .find(|p| p.lock().threads.contains(&tid))
        .cloned()
}

/// Get a process by pid.
pub fn process(pid: usize) -> Option<Arc<Mutex<Process>>> {
    PROCESSES.read().get(&pid).cloned()
}

/// Get all processes in a process group.
pub fn process_group(pgid: Pgid) -> Vec<Arc<Mutex<Process>>> {
    PROCESSES
        .read()
        .values()
        .filter(|p| p.lock().pgid == pgid)
        .cloned()
        .collect()
}

/// Assign `pid` to the process and register it in the global process table.
pub fn add_to_process_table(proc: Arc<Mutex<Process>>, pid: Pid) {
    proc.lock().pid = pid;
    PROCESSES.write().insert(pid.get(), proc);
}

impl Process {
    /// Lowest free fd.
    pub fn get_free_fd(&self) -> usize {
        (0..).find(|i| !self.files.contains_key(i)).unwrap()
    }

    /// Lowest free fd greater than or equal to `arg`.
    pub fn get_free_fd_from(&self, arg: usize) -> usize {
        (arg..).find(|i| !self.files.contains_key(i)).unwrap()
    }

    /// Add a file to the process, returning its fd.
    pub fn add_file(&mut self, file_like: FileLike) -> usize {
        let fd = self.get_free_fd();
        self.files.insert(fd, file_like);
        fd
    }

    /// Get (or lazily create) the futex for `uaddr`.
    pub fn get_futex(&mut self, uaddr: usize) -> Arc<Futex> {
        if !self.futexes.contains_key(&uaddr) {
            self.futexes.insert(uaddr, Arc::new(Futex::new()));
        }
        self.futexes.get(&uaddr).unwrap().clone()
    }

    /// Exit the process: drop resources and notify the parent.
    pub fn exit(&mut self, exit_code: usize) {
        // Drop open files manually (the map's own Drop can cause issues).
        let fds: Vec<_> = self.files.keys().copied().collect();
        for fd in fds {
            drop(self.files.remove(&fd));
        }

        // Notify our own event bus and the parent.
        self.eventbus.lock().set(Event::PROCESS_QUIT);
        if let Some(parent) = self.parent.1.upgrade() {
            parent.lock().eventbus.lock().set(Event::CHILD_PROCESS_QUIT);
        }
        self.exit_code = exit_code;

        // Remove all of our threads from the global thread table.
        // This must come after setting the exit events so that a thread that
        // happens to be exiting concurrently doesn't appear to have exited
        // before us.
        use crate::task::thread::THREADS;
        let mut table = THREADS.write();
        for tid in &self.threads {
            table.remove(tid);
        }
        self.threads.clear();

        info!("process {} exit with {}", self.pid.get(), exit_code);
    }

    /// Whether the process has terminated (no live threads).
    pub fn exited(&self) -> bool {
        self.threads.is_empty()
    }

    /// Build the address space of a new user process from the ELF at `inode`.
    /// Returns `(entry_point, ustack_top)`.
    pub fn new_user_vm(
        inode: &Arc<dyn INode>,
        args: Vec<String>,
        envs: Vec<String>,
        vm: &mut MemorySet,
    ) -> Result<(usize, usize), &'static str> {
        let mut data = [0u8; 0x3c0];
        inode
            .read_at(0, &mut data)
            .map_err(|_| "failed to read from INode")?;
        let elf = ElfFile::new(&data)?;

        match elf.header.pt2.type_().as_type() {
            header::Type::Executable => {}
            header::Type::SharedObject => {}
            _ => return Err("ELF is not executable or shared object"),
        }
        match elf.header.pt2.machine().as_machine() {
            #[cfg(target_arch = "x86_64")]
            header::Machine::X86_64 => {}
            #[cfg(target_arch = "aarch64")]
            header::Machine::AArch64 => {}
            #[cfg(riscv)]
            header::Machine::Other(243) => {}
            #[cfg(target_arch = "mips")]
            header::Machine::Mips => {}
            _ => return Err("invalid ELF arch"),
        }

        let mut auxv = BTreeMap::new();
        if let Some(phdr_vaddr) = elf.get_phdr_vaddr() {
            auxv.insert(AT_PHDR, phdr_vaddr as usize);
        }
        auxv.insert(AT_PHENT, elf.header.pt2.ph_entry_size() as usize);
        auxv.insert(AT_PHNUM, elf.header.pt2.ph_count() as usize);
        auxv.insert(AT_PAGESZ, PAGE_SIZE);

        let mut entry_addr = elf.header.pt2.entry_point() as usize;
        vm.clear();
        let bias = elf.make_memory_set(vm, inode);

        if let Ok(loader_path) = elf.get_interpreter() {
            info!("Handling interpreter… offset={:x}", bias);
            let interp_inode = crate::fs::ROOT_INODE
                .lookup_follow(loader_path, crate::fs::FOLLOW_MAX_DEPTH)
                .map_err(|_| "interpreter not found")?;
            let mut interp_data: [u8; 0x3c0] = unsafe { MaybeUninit::zeroed().assume_init() };
            interp_inode
                .read_at(0, &mut interp_data)
                .map_err(|_| "failed to read from INode")?;
            let elf_interp = ElfFile::new(&interp_data)?;
            elf_interp.append_as_interpreter(&interp_inode, vm, bias);
            auxv.insert(AT_ENTRY, elf.header.pt2.entry_point() as usize);
            auxv.insert(AT_BASE, bias);
            entry_addr = elf_interp.header.pt2.entry_point() as usize + bias;
        }

        let mut ustack_top = {
            let ustack_bottom = USER_STACK_OFFSET;
            let ustack_top = USER_STACK_OFFSET + USER_STACK_SIZE;
            vm.push(
                ustack_bottom,
                ustack_top - PAGE_SIZE * 4,
                MemoryAttr::default().user().execute(),
                Delay::new(GlobalFrameAlloc),
                "user_stack_delay",
            );
            vm.push(
                ustack_top - PAGE_SIZE * 4,
                ustack_top,
                MemoryAttr::default().user().execute(),
                ByFrame::new(GlobalFrameAlloc),
                "user_stack",
            );
            ustack_top
        };

        let init_info = ProcInitInfo { args, envs, auxv };
        unsafe {
            vm.with(|| ustack_top = init_info.push_at(ustack_top));
        }
        Ok((entry_addr, ustack_top))
    }

    /// Create a brand-new user process from `inode`.
    /// Returns the (unregistered) `Process`, entry point, and stack pointer.
    pub fn new_user(
        inode: &Arc<dyn INode>,
        exec_path: &str,
        args: Vec<String>,
        envs: Vec<String>,
    ) -> (Arc<Mutex<Process>>, usize, usize) {
        let mut vm = MemorySet::new();
        let (entry_addr, ustack_top) = Self::new_user_vm(inode, args, envs, &mut vm).unwrap();

        let vm = Arc::new(Mutex::new(vm));

        let mut files = BTreeMap::new();
        files.insert(
            0,
            FileLike::File(FileHandle::new(
                crate::fs::TTY.clone(),
                OpenOptions {
                    read: true,
                    write: false,
                    append: false,
                    nonblock: false,
                },
                String::from("/dev/tty"),
                false,
                false,
            )),
        );
        files.insert(
            1,
            FileLike::File(FileHandle::new(
                crate::fs::TTY.clone(),
                OpenOptions {
                    read: false,
                    write: true,
                    append: false,
                    nonblock: false,
                },
                String::from("/dev/tty"),
                false,
                false,
            )),
        );
        files.insert(
            2,
            FileLike::File(FileHandle::new(
                crate::fs::TTY.clone(),
                OpenOptions {
                    read: false,
                    write: true,
                    append: false,
                    nonblock: false,
                },
                String::from("/dev/tty"),
                false,
                false,
            )),
        );

        let proc = Arc::new(Mutex::new(Process {
            vm,
            files,
            cwd: String::from("/"),
            exec_path: String::from(exec_path),
            futexes: BTreeMap::default(),
            semaphores: SemProc::default(),
            pid: Pid::new(),
            pgid: 0,
            parent: (Pid::new(), Weak::new()),
            children: Vec::new(),
            threads: Vec::new(),
            exit_code: 0,
            pending_sigset: Sigset::empty(),
            sig_queue: VecDeque::new(),
            dispositions: [SignalAction::default(); Signal::RTMAX + 1],
            eventbus: EventBus::new(),
            shm_identifiers: ShmProc::default(),
        }));

        (proc, entry_addr, ustack_top)
    }

    /// Fork a new process from this one (deep-copy vm + files).
    /// Returns the new (unregistered) child `Process`.
    pub fn fork(&self) -> Arc<Mutex<Process>> {
        // Deep-copy the address space.
        let vm = self.vm.lock().clone();
        let vm = Arc::new(Mutex::new(vm));

        // The caller (`sys_fork`) will fill in the correct parent Weak after
        // the main thread is created (it has the parent's Arc at that point).
        Arc::new(Mutex::new(Process {
            vm,
            files: self.files.clone(),
            cwd: self.cwd.clone(),
            exec_path: self.exec_path.clone(),
            futexes: BTreeMap::default(),
            semaphores: self.semaphores.clone(),
            pid: Pid::new(),
            pgid: self.pgid,
            parent: (self.pid, Weak::new()),
            children: Vec::new(),
            threads: Vec::new(),
            exit_code: 0,
            pending_sigset: Sigset::empty(),
            sig_queue: VecDeque::new(),
            dispositions: self.dispositions.clone(),
            eventbus: EventBus::new(),
            shm_identifiers: self.shm_identifiers.clone(),
        }))
    }
}
