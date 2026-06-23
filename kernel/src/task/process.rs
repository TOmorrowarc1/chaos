//! `Process` — the shared-resource container (corresponds to rCore `proc.rs`).

use super::{Futex, Tid};
use crate::fs::FileLike;
use crate::ipc::{SemProc, ShmProc};
use crate::memory::MemorySet;
use crate::signal::{Siginfo, Signal, SignalAction, Sigset};
use crate::sync::{Event, EventBus, SpinNoIrqLock as Mutex};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::fmt;
use spin::RwLock;

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
    todo!()
}

/// Get a process by pid.
pub fn process(pid: usize) -> Option<Arc<Mutex<Process>>> {
    todo!()
}

/// Get all processes in a process group.
pub fn process_group(pgid: Pgid) -> Vec<Arc<Mutex<Process>>> {
    todo!()
}

/// Assign `pid` to the process and register it in the global process table.
pub fn add_to_process_table(proc: Arc<Mutex<Process>>, pid: Pid) {
    todo!()
}

impl Process {
    /// Lowest free fd.
    pub fn get_free_fd(&self) -> usize {
        todo!()
    }

    /// Lowest free fd greater than or equal to `arg`.
    pub fn get_free_fd_from(&self, arg: usize) -> usize {
        todo!()
    }

    /// Add a file to the process, returning its fd.
    pub fn add_file(&mut self, file_like: FileLike) -> usize {
        todo!()
    }

    /// Get (or lazily create) the futex for `uaddr`.
    pub fn get_futex(&mut self, uaddr: usize) -> Arc<Futex> {
        todo!()
    }

    /// Exit the process: drop resources and notify the parent.
    pub fn exit(&mut self, exit_code: usize) {
        todo!()
    }

    /// Whether the process has terminated (no live threads).
    pub fn exited(&self) -> bool {
        todo!()
    }
}
