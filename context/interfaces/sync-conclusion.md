# sync/ — Conclusion

## Overall Role

The sync module provides synchronization primitives for the kernel: mutual exclusion (locks), event notification (EventBus), counting synchronization (Semaphore), and condition waiting (Condvar). Every other module depends on sync — drivers, fs, process, memory, syscall, net, ipc, lkm, signal, trap, arch all import from it.

---

## 1. Mutex — Two Lock Types

### Role

Provide mutual exclusion with different interrupt behaviors. The two types share a common interface but differ in interrupt handling. No generic `MutexSupport` trait — each is a standalone struct.

### Dependencies

```rust
// SpinNoIrqLock only:
use crate::arch::interrupt::{disable_and_store, restore};
// Already exists in all 4 arch/*/interrupt/mod.rs
```

### Exported Types

```rust
// === SpinLock — basic spinlock, no interrupt interaction ===
pub struct SpinLock<T> { /* AtomicBool + UnsafeCell<T> */ }
pub struct SpinGuard<'a, T> { /* ... */ }  // impl Deref, DerefMut, Drop

impl<T> SpinLock<T> {
    pub const fn new(user_data: T) -> Self;
    pub fn lock(&self) -> SpinGuard<'_, T>;
    pub fn try_lock(&self) -> Option<SpinGuard<'_, T>>;
    pub fn busy_lock(&self) -> SpinGuard<'_, T>;    // loop { try_lock() }
    pub unsafe fn force_unlock(&self);                // atomic.store(false)
}

// === SpinNoIrqLock — disables interrupts while holding the lock ===
pub struct SpinNoIrqLock<T> { /* AtomicBool + UnsafeCell<T> */ }
pub struct NoIrqGuard<'a, T> { /* ... */ }  // drops → restore(irq_state)

impl<T> SpinNoIrqLock<T> {
    pub const fn new(user_data: T) -> Self;
    pub fn lock(&self) -> NoIrqGuard<'_, T>;          // disable_and_store + CAS
    pub fn try_lock(&self) -> Option<NoIrqGuard<'_, T>>;
    pub fn busy_lock(&self) -> NoIrqGuard<'_, T>;
    pub unsafe fn force_unlock(&self);
}

// === FlagsGuard — standalone RAII interrupt save/restore ===
pub struct FlagsGuard(usize);
impl FlagsGuard {
    pub fn no_irq_region() -> Self;  // disable_and_store, restore on Drop
}


```

### Callers

| Type | Used by |
|------|---------|
| `SpinLock` | drivers/bus/pci, drivers/serial/simple_uart, drivers/serial/uart16550, lkm/structs, lkm/manager, lkm/kernelvm, arch/x86_64/gdt |
| `SpinNoIrqLock` | Most of kernel — drivers (block, net, irq, mmc, serial, gpu, input), arch (aarch64 mailbox, aarch64 memory), net/structs, signal, syscall, lkm, and all fs/ |
| `FlagsGuard` | drivers/net/ixgbe |

---

## 2. EventBus — Event Notification Channel

### Role

A flag register (`Event` bitmask) with a list of one-shot callbacks. When the flags change, all callbacks are fired. Callbacks that return `true` are removed (one-shot). Used as the building block for async notification — Pipe, TtyINode, Semaphore all use it.

### Dependencies

```rust
use crate::sync::SpinNoIrqLock as Mutex;  // EventBus::new() wraps in Arc<Mutex<Self>>
```

### Exported Types

```rust
bitflags! {
    pub struct Event: u32 {
        const READABLE              = 1 << 0;
        const WRITABLE              = 1 << 1;
        const ERROR                 = 1 << 2;
        const CLOSED                = 1 << 3;
        const PROCESS_QUIT          = 1 << 10;
        const CHILD_PROCESS_QUIT    = 1 << 11;
        const RECEIVE_SIGNAL        = 1 << 12;
        const SEMAPHORE_REMOVED     = 1 << 20;
        const SEMAPHORE_CAN_ACQUIRE = 1 << 21;
    }
}

pub type EventHandler = Box<dyn Fn(Event) -> bool + Send>;

pub struct EventBus {
    event: Event,                   // current state
    callbacks: Vec<EventHandler>,   // registered waiters
}

impl EventBus {
    pub fn new() -> Arc<SpinNoIrqLock<Self>>;   // wrapped in Arc<Mutex>!
    pub fn set(&mut self, set: Event);
    pub fn clear(&mut self, set: Event);
    pub fn change(&mut self, reset: Event, set: Event);  // atomically reset then set
    pub fn subscribe(&mut self, callback: EventHandler);
    pub fn get_callback_len(&self) -> usize;
}
```

### Core Logic

```rust
fn change(&mut self, reset: Event, set: Event) {
    let old = self.event;
    self.event = (self.event & !reset) | set;
    if self.event != old {
        self.callbacks.retain(|cb| !cb(self.event));  // fire all, remove ones returning true
    }
}
```

### Used by

`Semaphore`, `Pipe` (fs/inode/pipe.rs), `TtyINode` (fs/inode/devfs/tty.rs), `syscall/proc.rs`

### EventBusFuture — wrapping EventBus in a Future

`wait_for_event` creates a `Future` that polls the EventBus and subscribes a waker:

```rust
pub fn wait_for_event(bus: Arc<Mutex<EventBus>>, mask: Event) -> impl Future<Output = Event> {
    EventBusFuture { bus, mask }
}

struct EventBusFuture {
    bus: Arc<Mutex<EventBus>>,
    mask: Event,
}

impl Future for EventBusFuture {
    type Output = Event;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut lock = self.bus.lock();
        if !(lock.event & self.mask).is_empty() {
            return Poll::Ready(lock.event);
        }
        let waker = cx.waker().clone();
        let mask = self.mask;
        lock.subscribe(Box::new(move |s| {
            if (s & mask).is_empty() { return false; }
            waker.wake_by_ref();
            true  // one-shot: remove after firing
        }));
        Poll::Pending
    }
}
```

The pattern: check flag → if set, return Ready → if not set, subscribe `cx.waker()` as a callback that fires when flag changes → return Pending. Used by `syscall/proc.rs` (wait4 child exit) and as the template for Semaphore's acquire future.

---

## 3. Semaphore — Counting Async Semaphore

### Role

A counting semaphore using EventBus for async wakeup. `acquire()` returns a Future that subscribes a waker to the EventBus and returns `Pending` until `release()` increments the count. This is the only sync primitive that correctly implements the async waker pattern.

### Dependencies

```rust
use crate::sync::{Event, EventBus, SpinNoIrqLock as Mutex};
use crate::syscall::SysError;  // SysError::EIDRM for removed semaphore
```

### Exported Types

```rust
pub struct Semaphore {
    lock: Arc<SpinNoIrqLock<SemaphoreInner>>,
}
struct SemaphoreInner {
    count: isize,
    pid: usize,
    removed: bool,
    eventbus: EventBus,
}

impl Semaphore {
    pub fn new(count: isize) -> Self;
    pub fn remove(&self);                              // mark removed, wake all waiters → EIDRM
    pub async fn acquire(&self) -> Result<(), SysError>;  // wait until count >= 1
    pub fn release(&self);                             // count += 1, signal waiters
    pub fn get(&self) -> isize;
    pub fn get_ncnt(&self) -> usize;
    pub fn get_pid(&self) -> usize;
    pub fn set_pid(&self, pid: usize);
    pub fn set(&self, value: isize);
}
```

### Acquire Flow (the correct async pattern)

```
poll():
  if inner.removed      → Ready(Err(EIDRM))
  if inner.count >= 1   → count--, Ready(Ok(()))
  else:
      let waker = cx.waker().clone()
      inner.eventbus.subscribe(Box::new(move |_| { waker.wake(); true }))
      Pending
```

### Used by

`syscall/ipc.rs` (System V semop — `sem.acquire().await`), `ipc/semary.rs` (SemArray wraps Vec of Semaphore)

---

## 4. Condvar — Condition Variable (BROKEN)

### Role

Wait for a predicate to become true, with broadcast wakeup. Currently broken — `wait_events` is a spin loop that never sleeps, and `notify_all` does nothing useful because the wait queue is never populated and `unpark()` is never called. It works only because the spin loop polls fast enough.

### Current Design (will be fixed after process/ module)

```rust
pub struct Condvar {
    wait_queue: SpinNoIrqLock<VecDeque<Arc<Thread>>>,   // UNUSED — never pushed
    pub epoll_queue: SpinNoIrqLock<VecDeque<RegisteredProcess>>,  // HACK — epoll internals
}
pub struct RegisteredProcess {
    proc: Arc<SpinNoIrqLock<Process>>,
    tid: usize, epfd: usize, fd: usize,
}
```

### Dependencies

```rust
use crate::process::{Process, Thread};   // for struct fields, not actual park/unpark
use crate::consts::{INFORM_PER_MSEC, USEC_PER_TICK};
use crate::syscall::TimeSpec;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
```

### Exported Methods

```rust
impl Condvar {
    pub fn new() -> Self;
    pub fn wait_event<T>(condvar: &Condvar, condition: impl FnMut() -> Option<T>) -> T;
    pub fn wait_events<T>(condvars: &[&Condvar], condition: impl FnMut() -> Option<T>) -> T;
    pub fn wait<'a, T, S>(&self, guard: MutexGuard<'a, T, S>) -> MutexGuard<'a, T, S>;
    pub fn wait_timeout<'a, T, S>(&self, guard: MutexGuard<'a, T, S>, timeout: TimeSpec)
        -> Option<MutexGuard<'a, T, S>>;
    pub fn notify_one(&self);
    pub fn notify_all(&self);    // only calls epoll_callback, never wakes threads
    pub fn notify_n(&self, n: usize) -> usize;
    pub fn register_epoll_list(&self, proc, tid, epfd, fd);
    pub fn unregister_epoll_list(&self, tid, epfd, fd) -> bool;
}
```

### What Actually Happens

```rust
// wait_events:
loop {
    lock all wait_queues, unlock all (protects nothing)
    if condition() returns Some → return it
}
// notify_all:
for each in epoll_queue: add fd to epoll ready_list
// never calls thread::unpark()
```

### Future Fix (after process/ module provides park/unpark)

Replace `wait_queue: VecDeque<Arc<Thread>>` with proper thread parking:

```rust
pub fn wait_events<T>(condvars: &[&Condvar], condition: impl FnMut() -> Option<T>) -> T {
    loop {
        if let Some(res) = condition() { return res; }
        let thread = current_thread();
        for cv in condvars { cv.wait_queue.lock().push_back(thread.clone()); }
        thread::park();
    }
}
pub fn notify_all(&self) {
    let queue = self.wait_queue.lock().drain(..).collect::<Vec<_>>();
    for t in queue { t.unpark(); }
}
```

Or convert to Future-based (EventBus pattern) when all callers support async. Either way, the `epoll_queue` hack, `RegisteredProcess`, and `epoll_callback` disappear.

### Used by (current broken version)

- `syscall/fs.rs` — `TICK_ACTIVITY` + `SOCKET_ACTIVITY` in epoll/poll/select
- `syscall/mod.rs` — `spin_and_wait` helper
- `drivers/mod.rs` — `SOCKET_ACTIVITY` (global Condvar)
- `drivers/serial/mod.rs` — `SERIAL_ACTIVITY` (global Condvar)
- `trap.rs` — `TICK_ACTIVITY` (global Condvar, notified on every timer tick)
- `net/structs.rs` — socket send/receive blocking waits

---
