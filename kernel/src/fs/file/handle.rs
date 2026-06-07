use alloc::string::String;
use alloc::sync::Arc;
use rcore_fs::vfs::{FileType, FsError, INode, MMapArea, Metadata, PollStatus, Result};
use spin::RwLock;

enum Flock { None, Shared, Exclusive }

struct OpenFileDescription {
    offset: u64,
    options: OpenOptions,
    flock: Flock,
}

impl OpenFileDescription {
    fn create(options: OpenOptions) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(OpenFileDescription { offset: 0, options, flock: Flock::None }))
    }
}

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
    pub fn new(inode: Arc<dyn INode>, options: OpenOptions, path: String, pipe: bool, fd_cloexec: bool) -> Self {
        FileHandle { inode, description: OpenFileDescription::create(options), path, pipe, fd_cloexec }
    }

    pub fn dup(&self, fd_cloexec: bool) -> Self {
        FileHandle {
            inode: self.inode.clone(),
            description: self.description.clone(),
            path: self.path.clone(),
            pipe: self.pipe,
            fd_cloexec,
        }
    }

    pub fn set_options(&self, arg: usize) { todo!() }

    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> { todo!() }
    pub async fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize> { todo!() }
    pub fn write(&mut self, buf: &[u8]) -> Result<usize> { todo!() }
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize> { todo!() }
    pub fn seek(&mut self, pos: SeekFrom) -> Result<u64> { todo!() }
    pub fn set_len(&mut self, len: u64) -> Result<()> { todo!() }
    pub fn sync_all(&mut self) -> Result<()> { todo!() }
    pub fn sync_data(&mut self) -> Result<()> { todo!() }
    pub fn metadata(&self) -> Result<Metadata> { todo!() }
    pub fn lookup_follow(&self, path: &str, max_follow: usize) -> Result<Arc<dyn INode>> { todo!() }
    pub fn read_entry(&mut self) -> Result<String> { todo!() }
    pub fn read_entry_with_metadata(&mut self) -> Result<(Metadata, String)> { todo!() }
    pub fn poll(&self) -> Result<PollStatus> { todo!() }
    pub async fn async_poll(&self) -> Result<PollStatus> { todo!() }
    pub fn io_control(&self, cmd: u32, arg: usize) -> Result<usize> { todo!() }
    pub fn mmap(&mut self, area: MMapArea) -> Result<()> { todo!() }
    pub fn inode(&self) -> Arc<dyn INode> { self.inode.clone() }
}
