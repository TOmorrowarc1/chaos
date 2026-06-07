use alloc::sync::Arc;
use core::any::Any;
use rcore_fs::vfs::*;

use crate::sync::SpinNoIrqLock as Mutex;

struct RandomINodeData { seed: u32 }

pub struct RandomINode {
    data: Arc<Mutex<RandomINodeData>>,
    secure: bool,
}

impl RandomINode {
    pub fn new(secure: bool) -> RandomINode {
        RandomINode { secure, data: Arc::new(Mutex::new(RandomINodeData { seed: 1 })) }
    }
}

impl INode for RandomINode {
    fn read_at(&self, _offset: usize, buf: &mut [u8]) -> Result<usize> { todo!() }
    fn write_at(&self, _offset: usize, _buf: &[u8]) -> Result<usize> { Err(FsError::NotSupported) }
    fn poll(&self) -> Result<PollStatus> {
        Ok(PollStatus { read: true, write: false, error: false })
    }
    fn metadata(&self) -> Result<Metadata> { todo!() }
    fn as_any_ref(&self) -> &dyn Any { self }
}
