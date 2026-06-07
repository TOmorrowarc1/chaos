use alloc::sync::Arc;
use rcore_fs::vfs::*;
use rcore_fs_devfs::{special::{NullINode, ZeroINode}, DevFS};
use rcore_fs_mountfs::MountFS;
use rcore_fs_ramfs::RamFS;
use rcore_fs_sfs::SimpleFileSystem;

use crate::drivers::{BlockDriver, BlockDriverWrapper};

mod file;
pub mod epoll;
mod inode;
mod membuf;
pub mod protocol;

pub use file::*;
pub use inode::*;
pub use membuf::MemBuf;
pub use protocol::ioctl;
pub use protocol::fcntl;

#[cfg(feature = "link_user")]
global_asm!(concat!(
    r#"
	.section .data.img
	.global _user_img_start
	.global _user_img_end
_user_img_start:
    .incbin ""#,
    env!("USER_IMG"),
    r#""
_user_img_end:
"#
));

lazy_static! {
    pub static ref ROOT_INODE: Arc<dyn INode> = {
        todo!("ROOT_INODE")
    };
}

pub const FOLLOW_MAX_DEPTH: usize = 3;

pub trait INodeExt {
    fn read_as_vec(&self) -> Result<Vec<u8>>;
}

impl INodeExt for dyn INode {
    fn read_as_vec(&self) -> Result<Vec<u8>> {
        let size = self.metadata()?.size;
        let mut buf = Vec::with_capacity(size);
        unsafe { buf.set_len(size); }
        self.read_at(0, buf.as_mut_slice())?;
        Ok(buf)
    }
}
