use crate::fs::ioctl::*;
use crate::signal::{send_signal, Siginfo, Signal, SI_KERNEL};
use crate::sync::{Event, EventBus, SpinNoIrqLock as Mutex};
use crate::task::process_group;
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::any::Any;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use rcore_fs::vfs::*;
use spin::RwLock;

pub type Pgid = i32;

/// console tty (/dev/tty)
/// Ref: https://linux.die.net/man/4/tty
pub struct TtyINode {
    /// foreground process group (target of terminal-generated signals)
    foreground_pgid: RwLock<Pgid>,
    buf: Mutex<VecDeque<u8>>,
    eventbus: Mutex<EventBus>,
    winsize: RwLock<Winsize>,
    termios: RwLock<Termios>,
}

lazy_static! {
    pub static ref TTY: Arc<TtyINode> = Arc::new(TtyINode {
        foreground_pgid: RwLock::new(0),
        buf: Mutex::new(VecDeque::new()),
        eventbus: Mutex::new(EventBus::default()),
        winsize: RwLock::new(Winsize::default()),
        termios: RwLock::new(Termios::default()),
    });
}

pub fn foreground_pgid() -> Pgid {
    *TTY.foreground_pgid.read()
}

impl TtyINode {
    /// Feed one input byte (called from `trap::serial`).
    /// If ISIG is enabled and the byte is a control char, generate the
    /// corresponding signal to the foreground group instead of buffering it.
    pub fn push(&self, c: u8) {
        let lflag = LocalModes::from_bits_truncate(self.termios.read().lflag);
        if lflag.contains(LocalModes::ISIG) && [0o3, 0o34, 0o32, 0o31].contains(&(c as i32)) {
            let foreground_processes = process_group(foreground_pgid());
            match c as i32 {
                // INTR (Ctrl-C) → SIGINT
                0o3 => {
                    for proc in foreground_processes {
                        send_signal(
                            proc,
                            -1,
                            Siginfo {
                                signo: Signal::SIGINT as i32,
                                errno: 0,
                                code: SI_KERNEL,
                                field: Default::default(),
                            },
                        );
                    }
                }
                _ => warn!("special char {} is unimplemented", c),
            }
        } else {
            self.buf.lock().push_back(c);
            self.eventbus.lock().set(Event::READABLE);
        }
    }

    pub fn pop(&self) -> u8 {
        let mut buf_lock = self.buf.lock();
        let c = buf_lock.pop_front().unwrap();
        if buf_lock.len() == 0 {
            self.eventbus.lock().clear(Event::READABLE);
        }
        c
    }

    pub fn can_read(&self) -> bool {
        self.buf.lock().len() > 0
    }
}

impl INode for TtyINode {
    fn read_at(&self, _offset: usize, buf: &mut [u8]) -> Result<usize> {
        if self.can_read() {
            buf[0] = self.pop();
            Ok(1)
        } else {
            Err(FsError::Again)
        }
    }

    fn write_at(&self, _offset: usize, buf: &[u8]) -> Result<usize> {
        use core::str;
        // we don't validate the utf-8, we just want to print it
        let s = unsafe { str::from_utf8_unchecked(buf) };
        print!("{}", s);
        Ok(buf.len())
    }

    fn poll(&self) -> Result<PollStatus> {
        Ok(PollStatus {
            read: self.can_read(),
            write: true,
            error: false,
        })
    }

    fn async_poll<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<PollStatus>> + Send + Sync + 'a>> {
        #[must_use = "future does nothing unless polled/`await`-ed"]
        struct TtyFuture<'a> {
            tty: &'a TtyINode,
        }

        impl<'a> Future for TtyFuture<'a> {
            type Output = Result<PollStatus>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                if self.tty.can_read() {
                    return Poll::Ready(self.tty.poll());
                }
                let waker = cx.waker().clone();
                self.tty.eventbus.lock().subscribe(Box::new(move |_| {
                    waker.wake_by_ref();
                    true
                }));
                Poll::Pending
            }
        }

        Box::pin(TtyFuture { tty: self })
    }

    fn io_control(&self, cmd: u32, data: usize) -> Result<usize> {
        let cmd = cmd as usize;
        match cmd {
            TIOCGPGRP => {
                let argp = data as *mut i32; // pid_t
                unsafe { *argp = *self.foreground_pgid.read() };
                Ok(0)
            }
            TIOCSPGRP => {
                let fpgid = unsafe { *(data as *const i32) };
                *self.foreground_pgid.write() = fpgid;
                info!("tty: set foreground process group to {}", fpgid);
                Ok(0)
            }
            TIOCGWINSZ => {
                let winsize = data as *mut Winsize;
                unsafe {
                    *winsize = *self.winsize.read();
                }
                Ok(0)
            }
            TCGETS => {
                let termios = data as *mut Termios;
                unsafe {
                    *termios = *self.termios.read();
                }
                Ok(0)
            }
            TCSETS => {
                let termios = data as *const Termios;
                unsafe {
                    *self.termios.write() = *termios;
                }
                Ok(0)
            }
            _ => Err(FsError::NotSupported),
        }
    }

    fn metadata(&self) -> Result<Metadata> {
        Ok(Metadata {
            dev: 1,
            inode: 13,
            size: 0,
            blk_size: 0,
            blocks: 0,
            atime: Timespec { sec: 0, nsec: 0 },
            mtime: Timespec { sec: 0, nsec: 0 },
            ctime: Timespec { sec: 0, nsec: 0 },
            type_: FileType::CharDevice,
            mode: 0o666,
            nlinks: 1,
            uid: 0,
            gid: 0,
            rdev: make_rdev(5, 0),
        })
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }
}
