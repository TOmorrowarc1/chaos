use crate::sync::SpinNoIrqLock as Mutex;
use crate::task::Process;
use alloc::sync::Arc;

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

    pub fn wait<'a, T>(&self, guard: crate::sync::MutexGuard<'a, T>) -> crate::sync::MutexGuard<'a, T> {
        todo!()
    }

    pub fn notify_one(&self) {}

    pub fn notify_all(&self) {}

    pub fn notify_n(&self, _n: usize) -> usize {
        0
    }

    pub fn register_epoll_list(
        &self,
        proc: Arc<Mutex<Process>>,
        tid: usize,
        epfd: usize,
        fd: usize,
    ) {
        todo!()
    }

    pub fn unregister_epoll_list(&self, tid: usize, epfd: usize, fd: usize) -> bool {
        todo!()
    }
}
