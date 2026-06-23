use crate::fs::epoll::EpollInstance;
use super::handle::FileHandle;
use crate::net::Socket;
use crate::syscall::{SysError, SysResult};
use alloc::boxed::Box;
use core::fmt;
use rcore_fs::vfs::{MMapArea, PollStatus};

#[derive(Clone)]
pub enum FileLike {
    File(FileHandle),
    Socket(Box<dyn Socket>),
    EpollInstance(EpollInstance),
}

impl fmt::Debug for FileLike {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FileLike::File(file) => write!(f, "File({})", file.path),
            FileLike::Socket(_) => write!(f, "Socket"),
            FileLike::EpollInstance(_) => write!(f, "EpollInstance"),
        }
    }
}

impl FileLike {
    pub fn dup(&self, fd_cloexec: bool) -> FileLike {
        match self {
            FileLike::File(file) => FileLike::File(file.dup(fd_cloexec)),
            FileLike::Socket(s) => FileLike::Socket(s.clone()),
            FileLike::EpollInstance(e) => FileLike::EpollInstance(e.clone()),
        }
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> SysResult {
        let len = match self {
            FileLike::File(file) => file.read(buf).await?,
            FileLike::Socket(socket) => socket.read(buf).0?,
            FileLike::EpollInstance(_) => return Err(SysError::ENOSYS),
        };
        Ok(len)
    }

    pub fn write(&mut self, buf: &[u8]) -> SysResult {
        let len = match self {
            FileLike::File(file) => file.write(buf)?,
            FileLike::Socket(socket) => socket.write(buf, None)?,
            FileLike::EpollInstance(_) => return Err(SysError::ENOSYS),
        };
        Ok(len)
    }

    pub fn ioctl(&mut self, request: usize, arg1: usize, arg2: usize, arg3: usize) -> SysResult {
        match self {
            FileLike::File(file) => file.io_control(request as u32, arg1).map_err(Into::into),
            FileLike::Socket(socket) => socket.ioctl(request, arg1, arg2, arg3),
            FileLike::EpollInstance(_) => Err(SysError::ENOSYS),
        }
    }

    pub fn mmap(&mut self, area: MMapArea) -> SysResult {
        match self {
            FileLike::File(file) => file.mmap(area)?,
            _ => return Err(SysError::ENOSYS),
        }
        Ok(0)
    }

    pub fn poll(&self) -> Result<PollStatus, SysError> {
        let status = match self {
            FileLike::File(file) => file.poll()?,
            FileLike::Socket(socket) => {
                let (read, write, error) = socket.poll();
                PollStatus { read, write, error }
            }
            FileLike::EpollInstance(_) => return Err(SysError::ENOSYS),
        };
        Ok(status)
    }

    pub async fn async_poll(&self) -> Result<PollStatus, SysError> {
        let status = match self {
            FileLike::File(file) => file.async_poll().await?,
            FileLike::Socket(socket) => {
                let (read, write, error) = socket.poll();
                PollStatus { read, write, error }
            }
            FileLike::EpollInstance(_) => return Err(SysError::ENOSYS),
        };
        Ok(status)
    }
}
