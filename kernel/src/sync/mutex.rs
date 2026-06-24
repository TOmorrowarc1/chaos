use crate::arch::interrupt;
use core::cell::UnsafeCell;
use core::fmt;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{spin_loop_hint, AtomicBool, Ordering};

// ============================================================================
// SpinLock — basic spinlock, no interrupt interaction
// ============================================================================

pub struct SpinLock<T> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinLock<T> {}
unsafe impl<T: Send> Send for SpinLock<T> {}

pub struct SpinGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<'a, T> Drop for SpinGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.lock.store(false, Ordering::Release);
    }
}

impl<'a, T> Deref for SpinGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T { unsafe { &*self.lock.data.get() } }
}

impl<'a, T> DerefMut for SpinGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T { unsafe { &mut *self.lock.data.get() } }
}

impl<T> SpinLock<T> {
    pub const fn new(user_data: T) -> Self {
        SpinLock { lock: AtomicBool::new(false), data: UnsafeCell::new(user_data) }
    }

    pub fn lock(&self) -> SpinGuard<'_, T> {
        loop {
            if let Some(guard) = self.try_lock() {
                return guard;
            }
            while self.lock.load(Ordering::Relaxed) {
                spin_loop_hint();
            }
        }
    }

    pub fn try_lock(&self) -> Option<SpinGuard<'_, T>> {
        if self
            .lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(SpinGuard { lock: self })
        } else {
            None
        }
    }

    pub fn busy_lock(&self) -> SpinGuard<'_, T> {
        loop { if let Some(g) = self.try_lock() { return g; } }
    }

    pub unsafe fn force_unlock(&self) { self.lock.store(false, Ordering::Release); }

    pub fn into_inner(self) -> T { self.data.into_inner() }
}

impl<T: Clone> Clone for SpinLock<T> {
    fn clone(&self) -> Self {
        SpinLock::new(self.lock().clone())
    }
}

impl<T: Default> Default for SpinLock<T> {
    fn default() -> Self {
        SpinLock::new(T::default())
    }
}

impl<T: fmt::Debug> fmt::Debug for SpinLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.try_lock() {
            Some(guard) => f.debug_struct("SpinLock").field("data", &*guard).finish(),
            None => f.debug_struct("SpinLock").field("data", &"<locked>").finish(),
        }
    }
}

// ============================================================================
// SpinNoIrqLock — spinlock that also disables local interrupts
// ============================================================================

pub struct SpinNoIrqLock<T> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinNoIrqLock<T> {}
unsafe impl<T: Send> Send for SpinNoIrqLock<T> {}

pub struct NoIrqGuard<'a, T> {
    lock: &'a SpinNoIrqLock<T>,
    _flags: FlagsGuard,
}

impl<'a, T> Drop for NoIrqGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.lock.store(false, Ordering::Release);
    }
}

impl<'a, T> Deref for NoIrqGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T { unsafe { &*self.lock.data.get() } }
}

impl<'a, T> DerefMut for NoIrqGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T { unsafe { &mut *self.lock.data.get() } }
}

impl<T> SpinNoIrqLock<T> {
    pub const fn new(user_data: T) -> Self {
        SpinNoIrqLock { lock: AtomicBool::new(false), data: UnsafeCell::new(user_data) }
    }

    pub fn lock(&self) -> NoIrqGuard<'_, T> {
        loop {
            if let Some(guard) = self.try_lock() {
                return guard;
            }
            while self.lock.load(Ordering::Relaxed) {
                spin_loop_hint();
            }
        }
    }

    pub fn try_lock(&self) -> Option<NoIrqGuard<'_, T>> {
        // Disable interrupts FIRST: we must not be preempted while holding the lock.
        let flags = FlagsGuard::no_irq_region();
        if self
            .lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(NoIrqGuard { lock: self, _flags: flags })
        } else {
            // CAS failed: dropping `flags` here restores the interrupt state.
            None
        }
    }

    pub fn busy_lock(&self) -> NoIrqGuard<'_, T> {
        loop { if let Some(g) = self.try_lock() { return g; } }
    }

    pub unsafe fn force_unlock(&self) { self.lock.store(false, Ordering::Release); }

    pub fn into_inner(self) -> T { self.data.into_inner() }
}

impl<T: Clone> Clone for SpinNoIrqLock<T> {
    fn clone(&self) -> Self {
        SpinNoIrqLock::new(self.lock().clone())
    }
}

impl<T: Default> Default for SpinNoIrqLock<T> {
    fn default() -> Self {
        SpinNoIrqLock::new(T::default())
    }
}

impl<T: fmt::Debug> fmt::Debug for SpinNoIrqLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.try_lock() {
            Some(guard) => f.debug_struct("SpinNoIrqLock").field("data", &*guard).finish(),
            None => f.debug_struct("SpinNoIrqLock").field("data", &"<locked>").finish(),
        }
    }
}

// Compatibility alias: in this simple design the "mutex guard" is the
// interrupt-safe spinlock guard. (rCore parameterized this over a strategy
// type; we drop that parameter.)
pub type MutexGuard<'a, T> = NoIrqGuard<'a, T>;

// ============================================================================
// FlagsGuard — standalone RAII interrupt save/restore
// ============================================================================

pub struct FlagsGuard(usize);

impl Drop for FlagsGuard {
    fn drop(&mut self) { unsafe { interrupt::restore(self.0) } }
}

impl FlagsGuard {
    pub fn no_irq_region() -> Self { Self(unsafe { interrupt::disable_and_store() }) }
}
