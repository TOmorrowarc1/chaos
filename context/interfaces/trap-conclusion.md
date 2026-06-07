# trap.rs — Conclusion

## Overall Role

`trap.rs` provides cross-module state for timer ticks and serial input. The actual trap handling (user→supervisor register save/restore, exception dispatch) is per-architecture in `arch/*/interrupt/mod.rs`. `trap.rs` only holds the **global state** that the arch handler updates.

## Dependencies

```rust
use crate::sync::{SpinNoIrqLock as Mutex, Condvar};
use crate::fs::TTY;           // serial input routing
use crate::arch::timer::timer_now;
use crate::consts::{INFORM_PER_MSEC, USEC_PER_TICK};
use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;
use naive_timer::Timer;
```

## Exports

```rust
// === Tick Counters ===
pub static TICK: AtomicUsize;               // wall clock ticks (CPU 0 only)
pub static TICK_ALL_PROCESSORS: AtomicUsize; // per-CPU ticks (all CPUs)
pub static TICK_ACTIVITY: Condvar;           // signaled every tick

pub unsafe fn wall_tick() -> usize;          // read TICK
pub fn cpu_tick() -> usize;                  // read TICK_ALL_PROCESSORS
pub fn do_tick();                            // increment counters (CPU 0 -> TICK)
pub fn uptime_msec() -> usize;               // wall_tick() * USEC_PER_TICK / 1000

// === Timer Wheel ===
pub static NAIVE_TIMER: Mutex<Timer>;        // lazy_static

pub fn timer();                              // called on timer interrupt:
                                             //   do_tick()
                                             //   NAIVE_TIMER.lock().expire(now)

// === Serial Input ===
pub fn serial(c: u8);                        // called on serial interrupt:
                                             //   '\r' → b'\n'
                                             //   else → TTY.push(c)
```

## Flow

```
Timer interrupt:
  arch/*/interrupt/mod.rs → crate::trap::timer()
    → do_tick()
      → TICK.fetch_add(1)    (CPU 0 only, others increment TICK_ALL_PROCESSORS)
    → TICK_ACTIVITY.notify_all()  (wake epoll/poll/select waiters)
    → NAIVE_TIMER.lock().expire(now)  (fire nanosleep futures)
    → return to arch handler → return to user

Serial input:
  arch/*/interrupt/mod.rs → crate::trap::serial(c)
    → '\r'? → push '\n' to TTY
    → else → push c to TTY
    → return to arch handler → return to user
```

## What trap.rs is NOT

- Trap dispatch (syscall/page fault/etc.) — handled by `arch/*/interrupt/mod.rs`
- User/supervisor context switch — handled by `trapframe` crate + arch assembly
- Signal handling — handled by `signal/` module
- Syscall processing — handled by `syscall/` module and `process/thread.rs`

`trap.rs` is purely a notification hub: receive per-architecture events, update global counters, notify waiting subsystems. That is all.
