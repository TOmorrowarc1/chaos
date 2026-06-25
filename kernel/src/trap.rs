use crate::consts::USEC_PER_TICK;
use crate::fs::TTY;
use crate::sync::{Condvar, SpinNoIrqLock as Mutex};
use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;
use naive_timer::Timer;

pub static TICK: AtomicUsize = AtomicUsize::new(0);
pub static TICK_ALL_PROCESSORS: AtomicUsize = AtomicUsize::new(0);

lazy_static! {
    pub static ref TICK_ACTIVITY: Condvar = Condvar::new();
    pub static ref NAIVE_TIMER: Mutex<Timer> = Mutex::new(Timer::default());
}

pub unsafe fn wall_tick() -> usize {
    TICK.load(Ordering::Acquire)
}

pub fn cpu_tick() -> usize {
    TICK_ALL_PROCESSORS.load(Ordering::Acquire)
}

pub fn do_tick() {
    if crate::arch::cpu::id() == 0 {
        TICK.fetch_add(1, Ordering::Release);
    }
    TICK_ALL_PROCESSORS.fetch_add(1, Ordering::Release);
    TICK_ACTIVITY.notify_all();
}

pub fn uptime_msec() -> usize {
    unsafe { wall_tick() * USEC_PER_TICK / 1000 }
}

pub fn timer() {
    do_tick();
    let now = crate::arch::timer::timer_now();
    NAIVE_TIMER.lock().expire(now);
}

pub fn serial(c: u8) {
    TTY.push(if c == b'\r' { b'\n' } else { c });
}
