use core::any::Any;
use rcore_fs::vfs::*;

pub struct ShmINode;

impl INode for ShmINode {
    fn read_at(&self, _offset: usize, _buf: &mut [u8]) -> Result<usize> { Err(FsError::NotSupported) }
    fn write_at(&self, _offset: usize, _buf: &[u8]) -> Result<usize> { Err(FsError::NotSupported) }
    fn poll(&self) -> Result<PollStatus> {
        Ok(PollStatus { read: false, write: false, error: false })
    }
    fn metadata(&self) -> Result<Metadata> { todo!() }
    fn as_any_ref(&self) -> &dyn Any { self }
}
