# process/ — Interface

## Imports

```rust
// Arch
use crate::arch::cpu;
use crate::arch::paging::*;
use crate::arch::interrupt::consts::{is_intr, is_page_fault, is_syscall, is_timer_intr};
use crate::arch::interrupt::{get_trap_num, handle_reserved_inst, handle_user_page_fault};
use crate::arch::memory::{get_page_fault_addr, set_page_table};
use crate::arch::fp::FpState;

// FS
use crate::fs::{FileHandle, FileLike, OpenOptions, TTY, ROOT_INODE, FOLLOW_MAX_DEPTH};

// Memory
use crate::memory::{
    phys_to_virt, ByFrame, Delay, File, GlobalFrameAlloc, KernelStack,
    MemoryAttr, MemorySet, Read, AccessType,
};

// Signal
use crate::signal::{handle_signal, Siginfo, Signal, SignalAction, SignalStack, Sigset};

// Sync
use crate::sync::{EventBus, SpinLock, SpinNoIrqLock as Mutex};

// Syscall
use crate::syscall::handle_syscall;

// Other
use crate::ipc::{SemProc, ShmProc};
use crate::drivers::IRQ_MANAGER;

// External
use trapframe::{TrapFrame, UserContext};
use xmas_elf::{ElfFile, header, program::{Flags, SegmentData, Type}};
use rcore_fs::vfs::INode;
use rcore_memory::{Page, PAGE_SIZE};
use alloc::sync::{Arc, Weak};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
```

## Exports

```rust
// === Types ===
pub type Tid = usize;
pub type Pgid = i32;          // process group id

pub struct Pid(usize);
impl Pid {
    pub const INIT: usize = 1;
    pub fn new() -> Self;
    pub fn get(&self) -> usize;
    pub fn is_init(&self) -> bool;
}

pub struct Process {
    pub vm: Arc<Mutex<MemorySet>>,
    pub files: BTreeMap<usize, FileLike>,
    pub cwd: String,
    pub exec_path: String,
    pub futexes: BTreeMap<usize, Arc<Futex>>,
    pub semaphores: SemProc,
    pub pid: Pid,
    pub pgid: Pgid,
    pub parent: (Pid, Weak<Mutex<Process>>),
    pub children: Vec<(Pid, Weak<Mutex<Process>>)>,
    pub threads: Vec<Tid>,
    pub eventbus: Arc<Mutex<EventBus>>,
    pub exit_code: usize,
    pub sig_queue: VecDeque<(Siginfo, isize)>,
    pub pending_sigset: Sigset,
    pub dispositions: [SignalAction; Signal::RTMAX + 1],
    pub shm_identifiers: ShmProc,
}

pub struct Thread {
    pub inner: Mutex<ThreadInner>,
    pub vm: Arc<Mutex<MemorySet>>,
    pub proc: Arc<Mutex<Process>>,
    pub tid: Tid,
}

pub struct ThreadContext {
    user: Box<UserContext>,
    fp: Box<FpState>,
}

pub struct ThreadInner {
    pub context: Option<ThreadContext>,
    pub clear_child_tid: usize,
    pub sig_mask: Sigset,
    pub signal_alternate_stack: SignalStack,
}

// === Global Tables ===
pub static PROCESSES: RwLock<BTreeMap<usize, Arc<Mutex<Process>>>>;
pub static THREADS: RwLock<BTreeMap<usize, Arc<Thread>>>;
pub static PROCESSORS: [Option<Arc<Thread>>; MAX_CPU_NUM];

// === Process Management ===
pub fn current_thread() -> Option<Arc<Thread>>;   // reads PROCESSORS[cpu::id()]
pub fn spawn(thread: Arc<Thread>);                // starts async future on executor
pub fn process_of(tid: usize) -> Option<Arc<Mutex<Process>>>;
pub fn process(pid: usize) -> Option<Arc<Mutex<Process>>>;
pub fn process_group(pgid: Pgid) -> Vec<Arc<Mutex<Process>>>;
pub fn add_to_process_table(proc: Arc<Mutex<Process>>, pid: Pid);
pub fn init();                                     // spawns user shell

impl Process {
    pub fn get_free_fd(&self) -> usize;
    pub fn get_free_fd_from(&self, arg: usize) -> usize;
    pub fn add_file(&mut self, file_like: FileLike) -> usize;
    pub fn get_futex(&mut self, uaddr: usize) -> Arc<Futex>;
    pub fn exit(&mut self, exit_code: usize);
    pub fn exited(&self) -> bool;
}

impl Thread {
    pub fn add_to_table(self) -> Arc<Self>;
    pub fn new_user(inode: &Arc<dyn INode>, exec_path: &str,
                    args: Vec<String>, envs: Vec<String>) -> Arc<Thread>;
    pub fn new_user_vm(inode: &Arc<dyn INode>, args: Vec<String>, envs: Vec<String>,
                       vm: &mut MemorySet) -> Result<(usize, usize), &'static str>;
    pub fn fork(&self, tf: &UserContext) -> Arc<Thread>;
    pub fn new_clone(&self, context: &UserContext, stack_top: usize,
                     tls: usize, clear_child_tid: usize) -> Arc<Thread>;
    pub fn begin_running(&self) -> ThreadContext;
    pub fn end_running(&self, cx: ThreadContext);
    pub fn has_signal_to_handle(&self) -> bool;
}

// === ELF Loading ===
pub struct ProcInitInfo {
    pub args: Vec<String>,
    pub envs: Vec<String>,
    pub auxv: BTreeMap<u8, usize>,
}
impl ProcInitInfo {
    pub unsafe fn push_at(&self, stack_top: usize) -> usize;
}

pub trait ElfExt {
    fn make_memory_set(&self, ms: &mut MemorySet, inode: &Arc<dyn INode>) -> usize;
    fn get_interpreter(&self) -> Result<&str, &str>;
    fn append_as_interpreter(&self, inode: &Arc<dyn INode>, ms: &mut MemorySet, bias: usize);
    fn get_phdr_vaddr(&self) -> Option<u64>;
}

pub const AT_PHDR: u8 = 3;
pub const AT_PHENT: u8 = 4;
pub const AT_PHNUM: u8 = 5;
pub const AT_PAGESZ: u8 = 6;
pub const AT_BASE: u8 = 7;
pub const AT_ENTRY: u8 = 9;

pub struct INodeForMap(pub Arc<dyn INode>);
impl Read for INodeForMap {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize;
}

// === Futex ===
pub struct Futex {
    pub inner: Mutex<FutexInner>;   // contains VecDeque<Arc<Mutex<Waiter>>>
}
impl Futex {
    pub fn new() -> Self;
    pub fn wake(&self, wake_count: usize) -> usize;
    pub fn wait(self: &Arc<Self>, timeout: Option<Duration>)
        -> impl Future<Output = SysResult>;
}
```
