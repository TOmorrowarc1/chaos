# sync/ — Interface

## Imports

```rust
// Arch — interrupt save/restore (for SpinNoIrqLock)
use crate::arch::interrupt::{disable_and_store, restore};

// Process — thread parking/wakeup (for Condvar)
use crate::process::{Thread, Process};

// Trap — uptime for wait_timeout
use crate::trap::uptime_msec;

// Syscall — TimeSpec for wait_timeout
use crate::syscall::TimeSpec;

// External
use alloc::sync::Arc;
use alloc::collections::VecDeque;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
```

## Exports

```rust
// === Type aliases ===
pub type SpinLock<T> = Mutex<T, Spin>;          // basic spinlock
pub type SpinNoIrqLock<T> = Mutex<T, SpinNoIrq>; // spinlock + irq disable
pub type SleepLock<T> = Mutex<T, Condvar>;       // blocking lock via Condvar

// === Generic Mutex ===
pub struct Mutex<T: ?Sized, S: MutexSupport> { /* ... */ }
pub struct MutexGuard<'a, T: ?Sized + 'a, S: MutexSupport + 'a> { /* ... */ }
pub trait MutexSupport {
    type GuardData;
    fn new() -> Self;
    fn cpu_relax(&self);
    fn before_lock() -> Self::GuardData;
    fn after_unlock(&self);
}
// Implementations:
//   Spin           — MutexSupport (spin_loop_hint, no-op else)
//   SpinNoIrq      — MutexSupport (disable_and_store/restore via FlagsGuard)

impl<T, S: MutexSupport> Mutex<T, S> {
    pub const fn new(user_data: T) -> Self;
    pub fn lock(&self) -> MutexGuard<T, S>;
    pub fn busy_lock(&self) -> MutexGuard<T, S>;  // spin until acquired
    pub fn try_lock(&self) -> Option<MutexGuard<T, S>>;
    pub unsafe fn force_unlock(&self);
    pub fn into_inner(self) -> T;
}

pub struct FlagsGuard(usize);  // RAII: restores irq state on drop
impl FlagsGuard {
    pub fn no_irq_region() -> Self;
}

// === Condvar ===
pub struct Condvar {
    wait_queue: SpinNoIrqLock<VecDeque<Arc<Thread>>>,
    epoll_queue: SpinNoIrqLock<VecDeque<RegisteredProcess>>,
}

impl Condvar {
    pub fn new() -> Self;
    pub fn wait_event<T>(condvar: &Condvar, condition: impl FnMut() -> Option<T>) -> T;
    pub fn wait_events<T>(condvars: &[&Condvar], mut condition: impl FnMut() -> Option<T>) -> T;
    pub fn wait<'a, T, S>(&self, guard: MutexGuard<'a, T, S>) -> MutexGuard<'a, T, S>;
    pub fn wait_timeout<'a, T, S>(&self, guard: MutexGuard<'a, T, S>, timeout: TimeSpec)
        -> Option<MutexGuard<'a, T, S>>;
    pub fn notify_one(&self);
    pub fn notify_all(&self);
    pub fn notify_n(&self, n: usize) -> usize;
    pub fn register_epoll_list(&self, proc: Arc<SpinNoIrqLock<Process>>, tid: usize, epfd: usize, fd: usize);
    pub fn unregister_epoll_list(&self, tid: usize, epfd: usize, fd: usize) -> bool;
}

// === EventBus ===
bitflags! {
    pub struct Event: u32 {
        const READABLE             = 1 << 0;
        const WRITABLE             = 1 << 1;
        const ERROR                = 1 << 2;
        const CLOSED               = 1 << 3;
        const PROCESS_QUIT         = 1 << 10;
        const CHILD_PROCESS_QUIT   = 1 << 11;
        const RECEIVE_SIGNAL       = 1 << 12;
        const SEMAPHORE_REMOVED    = 1 << 20;
        const SEMAPHORE_CAN_ACQUIRE = 1 << 21;
    }
}

pub type EventHandler = Box<dyn Fn(Event) -> bool + Send>;

#[derive(Default)]
pub struct EventBus {
    event: Event,
    callbacks: Vec<EventHandler>,
}

impl EventBus {
    pub fn new() -> Arc<SpinNoIrqLock<Self>>;
    pub fn set(&mut self, set: Event);
    pub fn clear(&mut self, set: Event);
    pub fn change(&mut self, reset: Event, set: Event);
    pub fn subscribe(&mut self, callback: EventHandler);
    pub fn get_callback_len(&self) -> usize;
}

pub fn wait_for_event(bus: Arc<SpinNoIrqLock<EventBus>>, mask: Event)
    -> impl Future<Output = Event>;

// === Semaphore (counting, async) ===
pub struct Semaphore { /* ... */ }
pub struct SemaphoreGuard<'a> { /* ... */ }

impl Semaphore {
    pub fn new(count: isize) -> Self;
    pub fn remove(&self);
    pub async fn acquire(&self) -> Result<(), SysError>;
    pub fn release(&self);
    pub async fn access(&self) -> Result<SemaphoreGuard<'_>, SysError>;
    pub fn get(&self) -> isize;
    pub fn get_ncnt(&self) -> usize;
    pub fn get_pid(&self) -> usize;
    pub fn set_pid(&self, pid: usize);
    pub fn set(&self, value: isize);
}
```
