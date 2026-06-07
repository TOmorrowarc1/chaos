use crate::sync::{Event, EventBus, SpinNoIrqLock as Mutex};
use crate::syscall::SysError;
use alloc::sync::Arc;

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
        todo!()
    }

    pub async fn acquire(&self) -> Result<(), SysError> {
        todo!()
    }

    pub fn release(&self) {
        todo!()
    }

    pub fn get(&self) -> isize {
        todo!()
    }

    pub fn get_ncnt(&self) -> usize {
        todo!()
    }

    pub fn get_pid(&self) -> usize {
        todo!()
    }

    pub fn set_pid(&self, pid: usize) {
        todo!()
    }

    pub fn set(&self, value: isize) {
        todo!()
    }
}
