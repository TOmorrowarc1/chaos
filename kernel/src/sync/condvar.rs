use crate::sync::SpinNoIrqLock as Mutex;

pub struct Condvar;

impl Condvar {
    pub fn new() -> Self {
        Condvar
    }

    pub fn wait_event<T>(condvar: &Condvar, mut condition: impl FnMut() -> Option<T>) -> T {
        todo!()
    }

    pub fn wait_events<T>(condvars: &[&Condvar], mut condition: impl FnMut() -> Option<T>) -> T {
        loop {
            if let Some(res) = condition() {
                return res;
            }
        }
    }

    pub fn wait<'a, T>(&self, _guard: crate::sync::SpinGuard<'a, T>) -> crate::sync::SpinGuard<'a, T> {
        todo!()
    }

    pub fn notify_one(&self) {}

    pub fn notify_all(&self) {}

    pub fn notify_n(&self, _n: usize) -> usize {
        0
    }
}
