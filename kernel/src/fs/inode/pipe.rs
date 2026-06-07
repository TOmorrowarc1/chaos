use crate::sync::{EventBus, SpinNoIrqLock as Mutex};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::any::Any;
use rcore_fs::vfs::*;

#[derive(Clone, PartialEq)]
pub enum PipeEnd { Read, Write }

pub struct PipeData {
    buf: VecDeque<u8>,
    eventbus: EventBus,
    end_cnt: i32,
}

#[derive(Clone)]
pub struct Pipe {
    data: Arc<Mutex<PipeData>>,
    direction: PipeEnd,
}

impl Drop for Pipe {
    fn drop(&mut self) { todo!() }
}

impl Pipe {
    pub fn create_pair() -> (Pipe, Pipe) { todo!() }
    fn can_read(&self) -> bool { todo!() }
    fn can_write(&self) -> bool { todo!() }
}

impl INode for Pipe {
    fn read_at(&self, _offset: usize, buf: &mut [u8]) -> Result<usize> { todo!() }
    fn write_at(&self, _offset: usize, buf: &[u8]) -> Result<usize> { todo!() }
    fn poll(&self) -> Result<PollStatus> { todo!() }
    fn as_any_ref(&self) -> &dyn Any { self }
}
