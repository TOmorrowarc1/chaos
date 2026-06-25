use crate::fs::fcntl::O_NONBLOCK;
use crate::memory::GlobalFrameAlloc;
use crate::syscall::{MmapProt, TimeSpec};
use crate::task::{current_thread, INodeForMap};
use alloc::string::String;
use alloc::sync::Arc;
use rcore_fs::vfs::{FileType, FsError, INode, MMapArea, Metadata, PollStatus, Result};
use rcore_memory::memory_set::handler::File;
use spin::RwLock;

enum Flock {
    None,
    Shared,
    Exclusive,
}

struct OpenFileDescription {
    offset: u64,
    options: OpenOptions,
    flock: Flock,
}

impl OpenFileDescription {
    fn create(options: OpenOptions) -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(OpenFileDescription {
            offset: 0,
            options,
            flock: Flock::None,
        }))
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
    pub fn new(
        inode: Arc<dyn INode>,
        options: OpenOptions,
        path: String,
        pipe: bool,
        fd_cloexec: bool,
    ) -> Self {
        FileHandle {
            inode,
            description: OpenFileDescription::create(options),
            path,
            pipe,
            fd_cloexec,
        }
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

    pub fn set_options(&self, arg: usize) {
        let options = &mut self.description.write().options;
        options.nonblock = (arg & O_NONBLOCK) != 0;
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let offset = self.description.read().offset as usize;
        let len = self.read_at(offset, buf).await?;
        self.description.write().offset += len as u64;
        Ok(len)
    }

    pub async fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        if !self.description.read().options.read {
            return Err(FsError::InvalidParam); // TODO: => EBADF
        }
        if !self.description.read().options.nonblock {
            // Blocking read: retry on `Again`, parking on the inode's async_poll
            // until it signals readiness, then try again.
            loop {
                match self.inode.read_at(offset, buf) {
                    Ok(read_len) => return Ok(read_len),
                    Err(FsError::Again) => {
                        self.async_poll().await?;
                    }
                    Err(err) => return Err(err),
                }
            }
        } else {
            let len = self.inode.read_at(offset, buf)?;
            Ok(len)
        }
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let description = self.description.read();
        // O_APPEND: each write starts at the current end of file.
        let offset = match description.options.append {
            true => self.inode.metadata()?.size as u64,
            false => description.offset,
        } as usize;
        drop(description);
        let len = self.write_at(offset, buf)?;
        self.description.write().offset += len as u64;
        Ok(len)
    }

    pub fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize> {
        if !self.description.read().options.write {
            return Err(FsError::InvalidParam); // TODO: => EBADF
        }
        let len = self.inode.write_at(offset, buf)?;
        TimeSpec::update(&self.inode);
        Ok(len)
    }

    pub fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let mut description = self.description.write();
        description.offset = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => (self.inode.metadata()?.size as i64 + offset) as u64,
            SeekFrom::Current(offset) => (description.offset as i64 + offset) as u64,
        };
        Ok(description.offset)
    }

    pub fn set_len(&mut self, len: u64) -> Result<()> {
        if !self.description.read().options.write {
            return Err(FsError::InvalidParam); // TODO: => EBADF
        }
        self.inode.resize(len as usize)?;
        Ok(())
    }

    pub fn sync_all(&mut self) -> Result<()> {
        self.inode.sync_all()
    }

    pub fn sync_data(&mut self) -> Result<()> {
        self.inode.sync_data()
    }

    pub fn metadata(&self) -> Result<Metadata> {
        self.inode.metadata()
    }

    pub fn lookup_follow(&self, path: &str, max_follow: usize) -> Result<Arc<dyn INode>> {
        self.inode.lookup_follow(path, max_follow)
    }

    pub fn read_entry(&mut self) -> Result<String> {
        let mut description = self.description.write();
        if !description.options.read {
            return Err(FsError::InvalidParam); // TODO: => EBADF
        }
        let name = self.inode.get_entry(description.offset as usize)?;
        description.offset += 1;
        Ok(name)
    }

    pub fn read_entry_with_metadata(&mut self) -> Result<(Metadata, String)> {
        let mut description = self.description.write();
        if !description.options.read {
            return Err(FsError::InvalidParam); // TODO: => EBADF
        }
        let ret = self
            .inode
            .get_entry_with_metadata(description.offset as usize)?;
        description.offset += 1;
        Ok(ret)
    }

    pub fn poll(&self) -> Result<PollStatus> {
        self.inode.poll()
    }

    pub async fn async_poll(&self) -> Result<PollStatus> {
        self.inode.async_poll().await
    }

    pub fn io_control(&self, cmd: u32, arg: usize) -> Result<usize> {
        self.inode.io_control(cmd, arg)
    }

    pub fn mmap(&mut self, area: MMapArea) -> Result<()> {
        match self.inode.metadata()?.type_ {
            FileType::File => {
                let prot = MmapProt::from_bits_truncate(area.prot);
                let thread = current_thread().unwrap();
                thread.vm.lock().push(
                    area.start_vaddr,
                    area.end_vaddr,
                    prot.to_attr(),
                    File {
                        file: INodeForMap(self.inode.clone()),
                        mem_start: area.start_vaddr,
                        file_start: area.offset,
                        file_end: area.offset + area.end_vaddr - area.start_vaddr,
                        allocator: GlobalFrameAlloc,
                    },
                    "mmap_file",
                );
                Ok(())
            }
            FileType::CharDevice => self.inode.mmap(area),
            _ => Err(FsError::NotSupported),
        }
    }

    pub fn inode(&self) -> Arc<dyn INode> {
        self.inode.clone()
    }
}
