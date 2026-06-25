//! Futex (fast user-space mutex) kernel backend.

use crate::arch::timer::timer_now;
use crate::sync::SpinNoIrqLock as Mutex;
use crate::syscall::{SysError, SysResult};
use crate::trap::NAIVE_TIMER;
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
        let mut inner = self.inner.lock();
        for i in 0..wake_count {
            if let Some(waiter) = inner.waiters.pop_front() {
                let mut waiter = waiter.lock();
                waiter.woken = true;
                if let Some(waker) = waiter.waker.take() {
                    waker.wake();
                }
            } else {
                return i;
            }
        }
        wake_count
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
        let mut inner = self.waiter.lock();
        // Check if we've already been woken.
        if inner.woken {
            return Poll::Ready(Ok(0));
        }
        // Check timeout.
        if let Some(deadline) = self.deadline {
            if timer_now() >= deadline {
                inner.woken = true;
                return Poll::Ready(Err(SysError::ETIMEDOUT));
            }
        }

        // First time polling: enqueue ourselves and arm the timer.
        if inner.waker.is_none() {
            let mut futex_lock = inner.futex.inner.lock();
            futex_lock.waiters.push_back(self.waiter.clone());
            drop(futex_lock);
            inner.waker.replace(cx.waker().clone());

            if let Some(deadline) = self.deadline {
                let waker = cx.waker().clone();
                NAIVE_TIMER
                    .lock()
                    .add(deadline, Box::new(move |_| waker.wake()));
            }
        }
        Poll::Pending
    }
}
