//! INode for the framebuffer device (/dev/fb0).

use crate::drivers::gpu::fb::FRAME_BUFFER;
use core::any::Any;
use rcore_fs::vfs::*;

pub struct Fbdev;

impl INode for Fbdev {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        if let Some(fb) = FRAME_BUFFER.read().as_ref() {
            Ok(fb.read_at(offset, buf))
        } else {
            Err(FsError::NoDevice)
        }
    }

    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize> {
        if let Some(fb) = FRAME_BUFFER.write().as_mut() {
            let count = fb.write_at(offset, buf);
            if count == buf.len() {
                Ok(count)
            } else {
                Err(FsError::NoDeviceSpace)
            }
        } else {
            Err(FsError::NoDevice)
        }
    }

    fn poll(&self) -> Result<PollStatus> {
        Ok(PollStatus {
            read: true,
            write: false,
            error: false,
        })
    }

    fn metadata(&self) -> Result<Metadata> {
        Ok(Metadata {
            dev: 0,
            inode: 0,
            size: 0,
            blk_size: 0,
            blocks: 0,
            atime: Timespec { sec: 0, nsec: 0 },
            mtime: Timespec { sec: 0, nsec: 0 },
            ctime: Timespec { sec: 0, nsec: 0 },
            type_: FileType::CharDevice,
            mode: 0o660,
            nlinks: 1,
            uid: 0,
            gid: 0,
            rdev: make_rdev(29, 0),
        })
    }

    fn io_control(&self, _cmd: u32, _data: usize) -> Result<usize> {
        // The FBIOGET_VSCREENINFO / FBIOGET_FSCREENINFO screen-info queries are
        // a graphics-only feature (they fill fb_{var,fix}_screeninfo structs
        // from the framebuffer). They are inert under the `nographic` build
        // where FRAME_BUFFER is None, so we leave them unimplemented for now.
        Err(FsError::NotSupported)
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }
}
