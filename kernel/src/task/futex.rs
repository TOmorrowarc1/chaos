//! Futex (fast user-space mutex) kernel backend.

use crate::arch::timer::timer_now;
use crate::sync::SpinNoIrqLock as Mutex;
use crate::syscall::{SysError, SysResult};
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use core::time::Duration;

/// A single thread waiting on a futex.
pub struct Waiter {
    waker: Option<Waker>,
    woken: bool,
    futex: Arc<Futex>,
}

/// Mutable part of a futex.
pub struct FutexInner {
    waiters: VecDeque<Arc<Mutex<Waiter>>>,
}

/// A futex object, keyed in the process by user-space address.
pub struct Futex {
    pub inner: Mutex<FutexInner>,
}

impl Futex {
    pub fn new() -> Self {
        Futex {
            inner: Mutex::new(FutexInner {
                waiters: VecDeque::new(),
            }),
        }
    }

    /// Wake up to `wake_count` waiters. Returns the number actually woken.
    pub fn wake(&self, wake_count: usize) -> usize {
        todo!()
    }

    /// Wait on this futex until woken or until `timeout` elapses.
    pub fn wait(self: &Arc<Self>, timeout: Option<Duration>) -> impl Future<Output = SysResult> {
        FutexFuture {
            waiter: Arc::new(Mutex::new(Waiter {
                waker: None,
                woken: false,
                futex: self.clone(),
            })),
            deadline: timeout.map(|t| timer_now() + t),
        }
    }
}

#[must_use = "future does nothing unless polled/`await`-ed"]
struct FutexFuture {
    waiter: Arc<Mutex<Waiter>>,
    deadline: Option<Duration>,
}

impl Future for FutexFuture {
    type Output = SysResult;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!()
    }
}
