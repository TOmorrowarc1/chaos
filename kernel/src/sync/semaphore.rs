use crate::sync::{Event, EventBus, SpinNoIrqLock as Mutex};
use crate::syscall::SysError;
use alloc::boxed::Box;
use alloc::sync::Arc;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

pub struct Semaphore {
    lock: Arc<Mutex<SemaphoreInner>>,
}

struct SemaphoreInner {
    count: isize,
    pid: usize,
    removed: bool,
    eventbus: EventBus,
}

impl Semaphore {
    pub fn new(count: isize) -> Self {
        Semaphore {
            lock: Arc::new(Mutex::new(SemaphoreInner {
                count,
                removed: false,
                pid: 0,
                eventbus: EventBus::default(),
            })),
        }
    }

    pub fn remove(&self) {
        let mut inner = self.lock.lock();
        inner.removed = true;
        // Wake every waiter; each will re-poll, see `removed`, and return EIDRM.
        inner.eventbus.set(Event::SEMAPHORE_REMOVED);
    }

    pub async fn acquire(&self) -> Result<(), SysError> {
        #[must_use = "future does nothing unless polled/`await`-ed"]
        struct SemaphoreFuture {
            inner: Arc<Mutex<SemaphoreInner>>,
        }

        impl Future for SemaphoreFuture {
            type Output = Result<(), SysError>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                let mut inner = self.inner.lock();
                if inner.removed {
                    // Set removed has priority over an available count.
                    return Poll::Ready(Err(SysError::EIDRM));
                } else if inner.count >= 1 {
                    inner.count -= 1;
                    if inner.count < 1 {
                        inner.eventbus.clear(Event::SEMAPHORE_CAN_ACQUIRE);
                    }
                    return Poll::Ready(Ok(()));
                }

                // Not available: park, registering a waker that fires on any change.
                let waker = cx.waker().clone();
                inner.eventbus.subscribe(Box::new(move |_| {
                    waker.wake_by_ref();
                    true
                }));
                Poll::Pending
            }
        }

        // The future holds its own Arc clone, so the inner state stays alive
        // across the await even if the semaphore is removed meanwhile.
        let future = SemaphoreFuture {
            inner: self.lock.clone(),
        };
        future.await
    }

    pub fn release(&self) {
        let mut inner = self.lock.lock();
        inner.count += 1;
        if inner.count >= 1 {
            inner.eventbus.set(Event::SEMAPHORE_CAN_ACQUIRE);
        }
    }

    /// Get the current count.
    pub fn get(&self) -> isize {
        self.lock.lock().count
    }

    /// Set the current count, waking a waiter if it becomes acquirable.
    pub fn set(&self, value: isize) {
        let mut inner = self.lock.lock();
        inner.count = value;
        if inner.count >= 1 {
            inner.eventbus.set(Event::SEMAPHORE_CAN_ACQUIRE);
        }
    }

    /// Get the pid of the last process that operated on this semaphore (sempid).
    pub fn get_pid(&self) -> usize {
        self.lock.lock().pid
    }

    /// Record the pid of the last operating process (sempid).
    pub fn set_pid(&self, pid: usize) {
        self.lock.lock().pid = pid;
    }

    /// Number of tasks currently waiting (semncnt) = subscribed wakers.
    pub fn get_ncnt(&self) -> usize {
        self.lock.lock().eventbus.get_callback_len()
    }
}
