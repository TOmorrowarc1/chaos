//! Task management: threads, processes, process groups.
//!
//! In this kernel the schedulable primitive is the `Thread`. A `Process` is a
//! shared-resource container (address space, file table, signals, IPC objects)
//! pointed to by one or more threads. Process groups are a pure id grouping
//! used for signal delivery / job control.

pub mod futex;
pub mod init;
pub mod process;
pub mod thread;

pub use futex::*;
pub use init::*;
pub use process::*;
pub use thread::*;

use crate::arch::cpu;
use crate::consts::MAX_CPU_NUM;
use alloc::sync::Arc;
use log::*;

/// Initialize the task subsystem: create the init process (user shell).
pub fn init() {
    crate::shell::add_user_shell();
    info!("task: init end");
}

/// Per-CPU pointer to the thread currently running on that CPU.
static mut PROCESSORS: [Option<Arc<Thread>>; MAX_CPU_NUM] = [None; MAX_CPU_NUM];

/// Get the thread currently running on this CPU.
/// `Thread` is effectively a CPU-local object while running.
pub fn current_thread() -> Option<Arc<Thread>> {
    let cpu_id = cpu::id();
    unsafe { PROCESSORS[cpu_id].clone() }
}
