//! Implement INode for Pipe

use crate::sync::{Event, EventBus, SpinNoIrqLock as Mutex};
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::any::Any;
use core::cmp::min;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use rcore_fs::vfs::FsError::Again;
use rcore_fs::vfs::*;

#[derive(Clone, PartialEq)]
pub enum PipeEnd {
    Read,
    Write,
}

pub struct PipeData {
    buf: VecDeque<u8>,
    eventbus: EventBus,
    /// number of open pipe ends (read + write)
    end_cnt: i32,
}

#[derive(Clone)]
pub struct Pipe {
    data: Arc<Mutex<PipeData>>,
    direction: PipeEnd,
}

impl Drop for Pipe {
    fn drop(&mut self) {
        // One end is being closed; notify the other side via CLOSED so a
        // blocked reader/writer wakes and observes EOF / broken-pipe.
        let mut data = self.data.lock();
        data.end_cnt -= 1;
        data.eventbus.set(Event::CLOSED);
    }
}

impl Pipe {
    /// Create a pair of pipe ends: (read, write), sharing one buffer.
    pub fn create_pair() -> (Pipe, Pipe) {
        let inner = PipeData {
            buf: VecDeque::new(),
            eventbus: EventBus::default(),
            end_cnt: 2, // one read end, one write end
        };
        let data = Arc::new(Mutex::new(inner));
        (
            Pipe {
                data: data.clone(),
                direction: PipeEnd::Read,
            },
            Pipe {
                data: data.clone(),
                direction: PipeEnd::Write,
            },
        )
    }

    fn can_read(&self) -> bool {
        if let PipeEnd::Read = self.direction {
            let data = self.data.lock();
            // readable if there is data, or the write end is gone (EOF)
            data.buf.len() > 0 || data.end_cnt < 2
        } else {
            false
        }
    }

    fn can_write(&self) -> bool {
        if let PipeEnd::Write = self.direction {
            // writable while the read end is still open
            self.data.lock().end_cnt == 2
        } else {
            false
        }
    }
}

impl INode for Pipe {
    fn read_at(&self, _offset: usize, buf: &mut [u8]) -> Result<usize> {
        if buf.len() == 0 {
            return Ok(0);
        }
        if let PipeEnd::Read = self.direction {
            let mut data = self.data.lock();
            if data.buf.len() == 0 && data.end_cnt == 2 {
                // empty but the writer is still open → would block
                Err(Again)
            } else {
                let len = min(buf.len(), data.buf.len());
                for i in 0..len {
                    buf[i] = data.buf.pop_front().unwrap();
                }
                if data.buf.len() == 0 {
                    data.eventbus.clear(Event::READABLE);
                }
                Ok(len)
            }
        } else {
            Ok(0)
        }
    }

    fn write_at(&self, _offset: usize, buf: &[u8]) -> Result<usize> {
        if let PipeEnd::Write = self.direction {
            let mut data = self.data.lock();
            for c in buf {
                data.buf.push_back(*c);
            }
            data.eventbus.set(Event::READABLE);
            Ok(buf.len())
        } else {
            Ok(0)
        }
    }

    fn poll(&self) -> Result<PollStatus> {
        Ok(PollStatus {
            read: self.can_read(),
            write: self.can_write(),
            error: false,
        })
    }

    fn async_poll<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<PollStatus>> + Send + Sync + 'a>> {
        #[must_use = "future does nothing unless polled/`await`-ed"]
        struct PipeFuture<'a> {
            pipe: &'a Pipe,
        }

        impl<'a> Future for PipeFuture<'a> {
            type Output = Result<PollStatus>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                if self.pipe.can_read() || self.pipe.can_write() {
                    return Poll::Ready(self.pipe.poll());
                }
                // Not ready: subscribe a one-shot waker on the shared eventbus.
                let waker = cx.waker().clone();
                let mut data = self.pipe.data.lock();
                data.eventbus.subscribe(Box::new(move |_| {
                    waker.wake_by_ref();
                    true
                }));
                Poll::Pending
            }
        }

        Box::pin(PipeFuture { pipe: self })
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }
}
