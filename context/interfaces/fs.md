# fs/ — Interface

## Imports

```rust
// External crates
use rcore_fs::{dev::block_cache::BlockCache, vfs::*};       // INode, FsError, FileType, Metadata,PollStatus, MMapArea, Timespec
use rcore_fs_devfs::{DevFS, special::{NullINode, ZeroINode}};
use rcore_fs_mountfs::MountFS;
use rcore_fs_ramfs::RamFS;
use rcore_fs_sfs::{SimpleFileSystem, INodeImpl};

use rcore_memory::memory_set::handler::{File, Linear, Shared, SharedGuard};
use rcore_memory::memory_set::MemoryAttr;
use rcore_memory::{PhysAddr, VirtAddr, PAGE_SIZE};

// Drivers
use crate::drivers::{BlockDriver, BlockDriverWrapper, BLK_DRIVERS};
use crate::drivers::{SERIAL_DRIVERS, SerialDriver};
use crate::drivers::gpu::fb::{FRAME_BUFFER, FramebufferInfo};

// Internal modules
use crate::sync::{SpinNoIrqLock as Mutex, EventBus, Event, Condvar};
use crate::memory::GlobalFrameAlloc;
use crate::process::{Process, Thread, current_thread, process_group, Pgid};
use crate::signal::{send_signal, Siginfo, SI_KERNEL, Signal};
use crate::trap::TICK_ACTIVITY;
use crate::syscall::{MmapProt, SysError, SysResult, TimeSpec};

// External
use alloc::sync::{Arc, Weak};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::pin::Pin;
use core::task::{Context, Poll};
use core::future::Future;
use bitflags::bitflags;
use spin::RwLock;
```

## Exports

```rust
// === Global Singletons ===
pub static ROOT_INODE: Arc<dyn INode>;      // lazy_static: mounts SFS + DevFS + RamFS
pub static TTY: Arc<TtyINode>;               // console terminal (/dev/tty)

// === Constants ===
pub const FOLLOW_MAX_DEPTH: usize = 3;

// === INodeExt ===
pub trait INodeExt {
    fn read_as_vec(&self) -> Result<Vec<u8>>;
}
impl INodeExt for dyn INode { /* reads entire file by metadata().size */ }

// === FileHandle ===
#[derive(Clone)]
pub struct FileHandle {
    inode: Arc<dyn INode>,
    description: Arc<RwLock<OpenFileDescription>>,
    pub path: String,
    pub pipe: bool,
    pub fd_cloexec: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub append: bool,
    pub nonblock: bool,
}

#[derive(Debug)]
pub enum SeekFrom {
    Start(u64),
    End(i64),
    Current(i64),
}

impl FileHandle {
    pub fn new(inode: Arc<dyn INode>, options: OpenOptions, path: String,
               pipe: bool, fd_cloexec: bool) -> Self;
    pub fn dup(&self, fd_cloexec: bool) -> Self;
    pub fn set_options(&self, arg: usize);            // set nonblock from fcntl
    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    pub async fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize>;
    pub fn write(&mut self, buf: &[u8]) -> Result<usize>;
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize>;
    pub fn seek(&mut self, pos: SeekFrom) -> Result<u64>;
    pub fn set_len(&mut self, len: u64) -> Result<()>;
    pub fn sync_all(&mut self) -> Result<()>;
    pub fn sync_data(&mut self) -> Result<()>;
    pub fn metadata(&self) -> Result<Metadata>;
    pub fn lookup_follow(&self, path: &str, max_follow: usize) -> Result<Arc<dyn INode>>;
    pub fn read_entry(&mut self) -> Result<String>;
    pub fn read_entry_with_metadata(&mut self) -> Result<(Metadata, String)>;
    pub fn poll(&self) -> Result<PollStatus>;
    pub async fn async_poll(&self) -> Result<PollStatus>;
    pub fn io_control(&self, cmd: u32, arg: usize) -> Result<usize>;
    pub fn mmap(&mut self, area: MMapArea) -> Result<()>;
    pub fn inode(&self) -> Arc<dyn INode>;
}

// === FileLike (enum for fd table) ===
#[derive(Clone)]
pub enum FileLike {
    File(FileHandle),
    Socket(Box<dyn Socket>),
    EpollInstance(EpollInstance),
}

impl FileLike {
    pub fn dup(&self, fd_cloexec: bool) -> FileLike;
    pub async fn read(&mut self, buf: &mut [u8]) -> SysResult;
    pub fn write(&mut self, buf: &[u8]) -> SysResult;
    pub fn ioctl(&mut self, request: usize, arg1: usize, arg2: usize, arg3: usize) -> SysResult;
    pub fn mmap(&mut self, area: MMapArea) -> SysResult;
    pub fn poll(&self) -> Result<PollStatus, SysError>;
    pub async fn async_poll(&self) -> Result<PollStatus, SysError>;
}

// === Pipe ===
#[derive(Clone)]
pub struct Pipe { /* data: Arc<Mutex<PipeData>>, direction: PipeEnd */ }

impl Pipe {
    pub fn create_pair() -> (Pipe, Pipe);
}

impl INode for Pipe { /* read_at from VecDeque, write_at pushes, poll checks can_read/can_write */ }

// === Epoll ===
pub struct EpollInstance {
    pub events: BTreeMap<usize, EpollEvent>,
    pub ready_list: SpinNoIrqLock<BTreeSet<usize>>,
    pub new_ctl_list: SpinNoIrqLock<BTreeSet<usize>>,
}

#[derive(Clone)]
pub struct EpollEvent {
    pub events: u32,
    pub data: EpollData,
}

impl EpollInstance {
    pub fn new(_flags: usize) -> Self;
    pub fn control(&mut self, op: usize, fd: usize, event: &EpollEvent) -> SysResult;
}

// === Pseudo-files (/proc) ===
pub struct Pseudo { /* content: Vec<u8>, type_: FileType */ }
impl Pseudo {
    pub fn new(s: &str, type_: FileType) -> Self;
}
impl INode for Pseudo { /* read from content vec, metadata returns it */ }

// === DevFS ===
pub struct TtyINode { /* foreground_pgid, buf, eventbus, winsize, termios */ }
impl TtyINode {
    pub fn push(&self, c: u8);         // called by trap::serial()
    pub fn pop(&self) -> u8;
    pub fn can_read(&self) -> bool;
}

pub struct Serial { id: usize, driver: Arc<dyn SerialDriver> }
impl Serial {
    pub fn new(id: usize, driver: Arc<dyn SerialDriver>) -> Self;
    pub fn wrap_all_serial_devices() -> Vec<Self>;
}

pub struct Fbdev;
pub struct RandomINode { /* prng seed */ }
pub struct ShmINode;  // stub directory for /dev/shm mount

// === fcntl Constants (pub mod) ===
pub const F_DUPFD: usize = 0;
pub const F_GETFD: usize = 1;
pub const F_SETFD: usize = 2;
pub const F_GETFL: usize = 3;
pub const F_SETFL: usize = 4;
pub const FD_CLOEXEC: usize = 1;
pub const F_DUPFD_CLOEXEC: usize = 1030;
pub const O_NONBLOCK: usize = 0o4000;
pub const O_APPEND: usize = 0o2000;
pub const O_CLOEXEC: usize = 0o2000000;
pub const AT_SYMLINK_NOFOLLOW: usize = 0x100;

// === ioctl Constants (pub mod) ===
pub const TCGETS: usize = 0x5401;        // varies by arch
pub const TCSETS: usize = 0x5402;
pub const TIOCGPGRP: usize = 0x540F;
pub const TIOCSPGRP: usize = 0x5410;
pub const TIOCGWINSZ: usize = 0x5413;
pub const FIONCLEX: usize = 0x5450;
pub const FIOCLEX: usize = 0x5451;
pub const FIONBIO: usize = 0x5421;

pub struct Termios { /* iflag, oflag, cflag, lflag, line, cc, ispeed, ospeed */ }
pub struct Winsize { /* row, ws_col, xpixel, ypixel */ }

bitflags! {
    pub struct LocalModes: u32 {
        const ISIG, ICANON, ECHO, ECHOE, ECHOK, ECHONL,
              NOFLSH, TOSTOP, IEXTEN, XCASE, ECHOCTL, ECHOPRT,
              ECHOKE, FLUSHO, PENDIN, EXTPROC;
    }
}

// === Device (linked SFS image) ===
pub struct MemBuf(RwLock<&'static mut [u8]>);
impl MemBuf {
    pub unsafe fn new(begin: unsafe extern "C" fn(), end: unsafe extern "C" fn()) -> Self;
}
impl Device for MemBuf { /* read_at/write_at on static memory region */ }

// === Process fs methods (defined in syscall/fs.rs, but on Process) ===
impl Process {
    pub fn get_file_like(&mut self, fd: usize) -> Result<&mut FileLike, SysError>;
    pub fn get_file(&mut self, fd: usize) -> Result<&mut FileHandle, SysError>;
    pub fn get_file_const(&self, fd: usize) -> Result<&FileHandle, SysError>;
    pub fn lookup_inode_at(&self, dirfd: usize, path: &str, follow: bool)
        -> Result<Arc<dyn INode>, SysError>;
    pub fn lookup_inode(&self, path: &str) -> Result<Arc<dyn INode>, SysError>;
    pub fn get_epoll_instance(&self, fd: usize) -> Result<&EpollInstance, SysError>;
    pub fn get_epoll_instance_mut(&mut self, fd: usize) -> Result<&mut EpollInstance, SysError>;
}
```
