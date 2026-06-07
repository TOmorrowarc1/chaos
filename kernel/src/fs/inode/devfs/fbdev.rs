use core::any::Any;
use rcore_fs::vfs::*;

pub struct Fbdev;

impl INode for Fbdev {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize> { todo!() }
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize> { todo!() }
    fn poll(&self) -> Result<PollStatus> { todo!() }
    fn metadata(&self) -> Result<Metadata> { todo!() }
    fn io_control(&self, cmd: u32, data: usize) -> Result<usize> { todo!() }
    fn as_any_ref(&self) -> &dyn Any { self }
}
