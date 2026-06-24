use core::any::Any;
use rcore_fs::vfs::*;

pub struct ShmINode;

impl INode for ShmINode {
    fn read_at(&self, _offset: usize, _buf: &mut [u8]) -> Result<usize> { Err(FsError::NotSupported) }
    fn write_at(&self, _offset: usize, _buf: &[u8]) -> Result<usize> { Err(FsError::NotSupported) }
    fn poll(&self) -> Result<PollStatus> {
        Ok(PollStatus { read: false, write: false, error: false })
    }
    fn metadata(&self) -> Result<Metadata> {
        // /dev/shm is exposed as a directory so it can be mounted on.
        Ok(Metadata {
            dev: 1,
            inode: 2,
            size: 0,
            blk_size: 0,
            blocks: 0,
            atime: Timespec { sec: 0, nsec: 0 },
            mtime: Timespec { sec: 0, nsec: 0 },
            ctime: Timespec { sec: 0, nsec: 0 },
            type_: FileType::Dir,
            mode: 0o666,
            nlinks: 1,
            uid: 0,
            gid: 0,
            rdev: make_rdev(0, 40),
        })
    }
    fn as_any_ref(&self) -> &dyn Any { self }
}
