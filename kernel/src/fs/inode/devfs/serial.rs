use crate::drivers::SerialDriver;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::any::Any;
use rcore_fs::vfs::*;

pub struct Serial {
    id: usize,
    driver: Arc<dyn SerialDriver>,
}

impl Serial {
    pub fn new(id: usize, driver: Arc<dyn SerialDriver>) -> Self {
        Serial { id, driver }
    }
    pub fn wrap_all_serial_devices() -> Vec<Self> { Vec::new() }
}

impl INode for Serial {
    fn read_at(&self, _offset: usize, buf: &mut [u8]) -> Result<usize> { todo!() }
    fn write_at(&self, _offset: usize, buf: &[u8]) -> Result<usize> { todo!() }
    fn poll(&self) -> Result<PollStatus> {
        Ok(PollStatus { read: true, write: true, error: false })
    }
    fn metadata(&self) -> Result<Metadata> { todo!() }
    fn as_any_ref(&self) -> &dyn Any { self }
}
