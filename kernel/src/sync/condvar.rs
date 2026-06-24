use crate::sync::{MutexGuard, SpinNoIrqLock as Mutex};
use crate::task::{Process, Thread};
use alloc::collections::VecDeque;
use alloc::sync::Arc;

/// A process + fd registered for epoll readiness notification.
pub struct RegisteredProcess {
    proc: Arc<Mutex<Process>>,
    tid: usize,
    epfd: usize,
    fd: usize,
}

/// A condition variable.
///
/// NOTE: this is a faithful port of rCore's (post-async-migration) condvar.
/// `wait_events` is a busy-spin that relies on interrupt handlers updating the
/// shared state checked by `condition()`; the notify / epoll machinery is
/// largely vestigial (the wait queue is never populated because there is no
/// real thread parking). See the TODO in `context/interfaces/sync.md` for the
/// plan to convert these paths to async/waker (EventBus) blocking.
#[derive(Default)]
pub struct Condvar {
    wait_queue: Mutex<VecDeque<Arc<Thread>>>,
    pub epoll_queue: Mutex<VecDeque<RegisteredProcess>>,
}

impl Condvar {
    pub fn new() -> Self {
        Condvar::default()
    }

    /// Wait until `condition()` returns `Some`.
    pub fn wait_event<T>(condvar: &Condvar, condition: impl FnMut() -> Option<T>) -> T {
        Self::wait_events(&[condvar], condition)
    }

    /// Wait on the given condvars until `condition()` returns `Some`.
    ///
    /// Busy-spin: the state that `condition()` inspects is updated by interrupt
    /// handlers (timer / NIC / serial) which call `notify_*` on the matching
    /// global condvar.
    pub fn wait_events<T>(_condvars: &[&Condvar], mut condition: impl FnMut() -> Option<T>) -> T {
        loop {
            if let Some(res) = condition() {
                return res;
            }
        }
    }

    /// Release `guard`, then re-acquire and return it.
    ///
    /// (A real implementation would park the current thread here; the spin
    /// design instead relies on the caller re-checking its condition in a loop,
    /// so we just drop the lock to let other code make progress and re-take it.)
    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        let mutex = guard.mutex();
        drop(guard);
        mutex.lock()
    }

    pub fn notify_one(&self) {
        let mut queue = self.wait_queue.lock();
        if let Some(t) = queue.front() {
            self.epoll_callback(t);
            queue.pop_front();
        }
    }

    pub fn notify_all(&self) {
        let mut queue = self.wait_queue.lock();
        for t in queue.iter() {
            self.epoll_callback(t);
        }
        queue.clear();
    }

    /// Notify up to `n` waiters; returns how many were notified.
    pub fn notify_n(&self, n: usize) -> usize {
        let mut count = 0;
        let mut queue = self.wait_queue.lock();
        for t in queue.iter() {
            if count >= n {
                break;
            }
            self.epoll_callback(t);
            count += 1;
        }
        for _ in 0..count {
            queue.pop_front();
        }
        count
    }

    pub fn register_epoll_list(
        &self,
        proc: Arc<Mutex<Process>>,
        tid: usize,
        epfd: usize,
        fd: usize,
    ) {
        self.epoll_queue.lock().push_back(RegisteredProcess {
            proc,
            tid,
            epfd,
            fd,
        });
    }

    pub fn unregister_epoll_list(&self, tid: usize, epfd: usize, fd: usize) -> bool {
        let mut epoll_list = self.epoll_queue.lock();
        for idx in 0..epoll_list.len() {
            if epoll_list[idx].tid == tid
                && epoll_list[idx].epfd == epfd
                && epoll_list[idx].fd == fd
            {
                epoll_list.remove(idx);
                return true;
            }
        }
        false
    }

    fn epoll_callback(&self, _thread: &Arc<Thread>) {
        let epoll_list = self.epoll_queue.lock();
        for ist in epoll_list.iter() {
            let proc = ist.proc.lock();
            match proc.get_epoll_instance(ist.epfd) {
                Ok(instance) => {
                    instance.ready_list.lock().insert(ist.fd);
                }
                Err(_) => {
                    panic!("epoll instance not exist");
                }
            }
        }
    }
}
