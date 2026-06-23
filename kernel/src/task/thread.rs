//! `Thread` — the schedulable execution unit, plus scheduling glue.

use super::Process;
use crate::arch::fp::FpState;
use crate::memory::MemorySet;
use crate::signal::{Sigset, SignalStack};
use crate::sync::SpinNoIrqLock as Mutex;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use rcore_fs::vfs::INode;
use spin::RwLock;
use trapframe::UserContext;

/// Thread id type.
pub type Tid = usize;

/// Saved CPU state of a thread.
pub struct ThreadContext {
    /// General-purpose / control registers + PC + SP.
    user: Box<UserContext>,
    /// Floating-point registers.
    /// TODO: lazy fp
    fp: Box<FpState>,
}

/// Mutable part of a thread.
#[derive(Default)]
pub struct ThreadInner {
    /// User context. `None` while the thread is running in user mode.
    context: Option<ThreadContext>,
    /// Kernel performs a futex wake at this address when the thread exits.
    pub clear_child_tid: usize,
    /// Signal mask.
    pub sig_mask: Sigset,
    /// Signal alternate stack.
    pub signal_alternate_stack: SignalStack,
}

/// A schedulable thread.
pub struct Thread {
    /// Mutable part.
    pub inner: Mutex<ThreadInner>,
    /// Same `Arc` as `proc.vm`; kept here to avoid extra locking.
    pub vm: Arc<Mutex<MemorySet>>,
    /// The process this thread belongs to.
    pub proc: Arc<Mutex<Process>>,
    /// Thread id.
    pub tid: Tid,
}

lazy_static! {
    /// Mapping between tid and `Thread`.
    pub static ref THREADS: RwLock<BTreeMap<usize, Arc<Thread>>> =
        RwLock::new(BTreeMap::new());
}

impl Thread {
    /// Assign a tid and register in the global thread table.
    pub fn add_to_table(self) -> Arc<Self> {
        todo!()
    }

    /// Build the address space of a new user process from the ELF at `inode`.
    /// Returns `(entry_point, ustack_top)`.
    pub fn new_user_vm(
        inode: &Arc<dyn INode>,
        args: Vec<String>,
        envs: Vec<String>,
        vm: &mut MemorySet,
    ) -> Result<(usize, usize), &'static str> {
        todo!()
    }

    /// Create a brand-new user process (and its main thread) from `inode`.
    pub fn new_user(
        inode: &Arc<dyn INode>,
        exec_path: &str,
        args: Vec<String>,
        envs: Vec<String>,
    ) -> Arc<Thread> {
        todo!()
    }

    /// Fork a new process from the current one.
    pub fn fork(&self, tf: &UserContext) -> Arc<Thread> {
        todo!()
    }

    /// Create a new thread in the same process (clone with CLONE_VM).
    pub fn new_clone(
        &self,
        context: &UserContext,
        stack_top: usize,
        tls: usize,
        clear_child_tid: usize,
    ) -> Arc<Thread> {
        todo!()
    }

    /// Take the saved context to begin running.
    pub fn begin_running(&self) -> ThreadContext {
        todo!()
    }

    /// Store the context back after a trap/yield.
    pub fn end_running(&self, cx: ThreadContext) {
        todo!()
    }

    /// Whether this thread has a pending, unmasked signal.
    pub fn has_signal_to_handle(&self) -> bool {
        todo!()
    }
}

/// Spawn a thread onto the async executor.
pub fn spawn(thread: Arc<Thread>) {
    todo!()
}

/// Yield execution back to the async runtime.
pub fn yield_now() -> impl Future<Output = ()> {
    YieldFuture::default()
}

#[must_use = "yield_now does nothing unless polled/`await`-ed"]
#[derive(Default)]
struct YieldFuture {
    flag: bool,
}

impl Future for YieldFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        todo!()
    }
}
