//! `Thread` — the schedulable execution unit, plus scheduling glue.

use super::{Process, PROCESSORS};
use super::process::{add_to_process_table, Pid};
use crate::arch::cpu;
use crate::arch::fp::FpState;
use crate::arch::interrupt::consts::{
    is_intr, is_page_fault, is_reserved_inst, is_syscall, is_timer_intr,
};
use crate::arch::interrupt::{get_trap_num, handle_reserved_inst};
use crate::arch::memory::{get_page_fault_addr, set_page_table};
use crate::arch::paging::*;
use crate::drivers::IRQ_MANAGER;
use crate::memory::MemorySet;
use crate::signal::{handle_signal, Sigset, SignalStack};
use crate::sync::SpinNoIrqLock as Mutex;
use crate::syscall::handle_syscall;
use crate::memory::AccessType;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use log::*;
use num::FromPrimitive;
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
    pub fn add_to_table(mut self) -> Arc<Self> {
        let mut table = THREADS.write();

        // assign tid, do not start from 0
        let tid = (Pid::INIT..)
            .find(|i| table.get(i).is_none())
            .unwrap();
        self.tid = tid;

        let self_ref = Arc::new(self);
        table.insert(tid, self_ref.clone());
        self_ref
    }

    /// Create a new (main) thread inside `proc` (which has already been built
    /// by `Process::new_user` or `Process::fork`).  Builds the `ThreadContext`,
    /// registers the thread in `THREADS`, and links it into `PROCESSES`.
    pub fn new(proc: Arc<Mutex<Process>>, entry: usize, sp: usize) -> Arc<Thread> {
        // User context: entry point + stack pointer.
        let mut context = UserContext::default();
        context.set_ip(entry);
        context.set_sp(sp);

        // Arch-specific register initialisation.
        #[cfg(target_arch = "x86_64")]
        { context.general.rflags = 0x3202; }
        #[cfg(riscv)]
        { context.sstatus = 1 << 18 | 1 << 14 | 1 << 13 | 1 << 5; }
        #[cfg(target_arch = "aarch64")]
        { context.spsr = 0b1101_00_0000; }
        #[cfg(target_arch = "mips")]
        { context.status = 1 << 4 | 1 << 29 | 1; context.status |= 1 << 8 | 1 << 9 | 1 << 15 | 1 << 14 | 1 << 13 | 1 << 12; }

        let thread = Thread {
            tid: 0, // assigned below
            inner: Mutex::new(ThreadInner {
                context: Some(ThreadContext {
                    user: Box::new(context),
                    fp: Box::new(FpState::new()),
                }),
                clear_child_tid: 0,
                sig_mask: Sigset::default(),
                signal_alternate_stack: SignalStack::default(),
            }),
            vm: proc.lock().vm.clone(),
            proc: proc.clone(),
        };

        let res = thread.add_to_table();

        // Register in PROCESSES (pid = main thread's tid).
        add_to_process_table(proc.clone(), Pid(res.tid));
        proc.lock().threads.push(res.tid);

        res
    }

    /// Create a new thread in the same process (clone with CLONE_VM).
    pub fn clone_thread(
        &self,
        context: &UserContext,
        stack_top: usize,
        tls: usize,
        clear_child_tid: usize,
    ) -> Arc<Thread> {
        let mut new_context = context.clone();
        new_context.set_syscall_ret(0);
        new_context.set_sp(stack_top);
        new_context.set_tls(tls);

        let sig_mask = self.inner.lock().sig_mask;
        let sigaltstack = self.inner.lock().signal_alternate_stack;

        let thread = Thread {
            tid: 0,
            inner: Mutex::new(ThreadInner {
                clear_child_tid,
                context: Some(ThreadContext {
                    user: Box::new(new_context),
                    fp: Box::new(FpState::new()),
                }),
                sig_mask,
                signal_alternate_stack: sigaltstack,
            }),
            vm: self.vm.clone(),
            proc: self.proc.clone(),
        };

        let res = thread.add_to_table();
        res.proc.lock().threads.push(res.tid);
        res
    }

    /// Take the saved context to begin running.
    pub fn begin_running(&self) -> ThreadContext {
        self.inner.lock().context.take().unwrap()
    }

    /// Store the context back after a trap/yield.
    pub fn end_running(&self, cx: ThreadContext) {
        self.inner.lock().context = Some(cx);
    }

    /// Whether this thread has a pending, unmasked signal.
    pub fn has_signal_to_handle(&self) -> bool {
        let proc = self.proc.lock();
        proc.sig_queue.iter().any(|(info, tid)| {
            let tid = *tid;
            (tid == -1 || tid as usize == self.tid)
                && !self.inner.lock().sig_mask.contains(
                    num::FromPrimitive::from_i32(info.signo).unwrap(),
                )
        })
    }
}

// ---------------------------------------------------------------------------
// Scheduling
// ---------------------------------------------------------------------------

/// Spawn a thread onto the async executor.
pub fn spawn(thread: Arc<Thread>) {
    let vmtoken = thread.vm.lock().token();
    let temp = thread.clone();

    let future = async move {
        loop {
            let mut thread_context = thread.begin_running();
            let cx = &mut thread_context.user;

            trace!("go to user: {:#x?}", cx);
            thread_context.fp.restore();
            cx.run();
            thread_context.fp.save();

            let trap_num = get_trap_num(&cx);
            trace!("back from user: {:#x?} trap_num {:#x}", cx, trap_num);

            let mut exit = false;
            let mut do_yield = false;

            match trap_num {
                _ if is_page_fault(trap_num) => {
                    let addr = get_page_fault_addr();
                    info!("page fault from user @ {:#x}", addr);
                    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
                    {
                        use crate::arch::interrupt::consts::{
                            is_execute_page_fault, is_read_page_fault,
                            is_write_page_fault,
                        };
                        use crate::arch::interrupt::handle_user_page_fault_ext;
                        let access_type = match trap_num {
                            _ if is_execute_page_fault(trap_num) => {
                                AccessType::execute(true)
                            }
                            _ if is_read_page_fault(trap_num) => {
                                AccessType::read(true)
                            }
                            _ if is_write_page_fault(trap_num) => {
                                AccessType::write(true)
                            }
                            _ => unreachable!(),
                        };
                        if !handle_user_page_fault_ext(&thread, addr, access_type) {
                            // TODO: SIGSEGV
                            panic!("page fault handle failed");
                        }
                    }
                    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
                    {
                        use crate::arch::interrupt::handle_user_page_fault;
                        if !handle_user_page_fault(&thread, addr) {
                            panic!("page fault handle failed");
                        }
                    }
                }
                _ if is_syscall(trap_num) => exit = handle_syscall(&thread, cx).await,
                _ if is_intr(trap_num) => {
                    crate::arch::interrupt::ack(trap_num);
                    trace!("handle irq {:#x}", trap_num);
                    if is_timer_intr(trap_num) {
                        do_yield = true;
                        crate::arch::interrupt::timer();
                    }
                    IRQ_MANAGER.read().try_handle_interrupt(Some(trap_num));
                }
                _ if is_reserved_inst(trap_num) => {
                    if !handle_reserved_inst(cx) {
                        panic!(
                            "unhandled reserved instr in thread {} trap {:#x} {:x?}",
                            thread.tid, trap_num, cx
                        );
                    }
                }
                _ => {
                    panic!(
                        "unhandled trap in thread {} trap {:#x} {:x?}",
                        thread.tid, trap_num, cx
                    );
                }
            }

            // Check signals before deciding whether to exit / yield.
            if !exit {
                exit = handle_signal(&thread, cx);
            }

            thread.end_running(thread_context);

            if exit {
                info!("thread {} stopped", thread.tid);
                break;
            } else if do_yield {
                yield_now().await;
            }
        }
    };

    spawn_thread(Box::pin(future), vmtoken, temp);
}

fn spawn_thread(
    future: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
    vmtoken: usize,
    thread: Arc<Thread>,
) {
    executor::spawn(PageTableSwitchWrapper {
        inner: Mutex::new(future),
        vmtoken,
        thread,
    });
}

#[must_use = "future does nothing unless polled/`await`-ed"]
struct PageTableSwitchWrapper {
    inner: Mutex<Pin<Box<dyn Future<Output = ()> + Send>>>,
    vmtoken: usize,
    thread: Arc<Thread>,
}

impl Future for PageTableSwitchWrapper {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let cpu_id = cpu::id();
        unsafe {
            PROCESSORS[cpu_id] = Some(self.thread.clone());
        }
        // vmtoken won't change across the lifetime of this thread.
        set_page_table(self.vmtoken);
        let res = self.inner.lock().as_mut().poll(cx);
        unsafe {
            PROCESSORS[cpu_id] = None;
        }
        res
    }
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

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        if self.flag {
            Poll::Ready(())
        } else {
            self.flag = true;
            cx.waker().clone().wake();
            Poll::Pending
        }
    }
}
