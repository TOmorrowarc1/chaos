use crate::sync::{EventBus, SpinNoIrqLock as Mutex};
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use core::any::Any;
use crate::fs::ioctl::*;
use rcore_fs::vfs::*;
use spin::RwLock;

pub type Pgid = i32;

pub struct TtyINode {
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

impl TtyINode {
    pub fn push(&self, c: u8) { todo!() }
    pub fn pop(&self) -> u8 { todo!() }
    pub fn can_read(&self) -> bool { todo!() }
}

impl INode for TtyINode {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize> { todo!() }
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize> { todo!() }
    fn poll(&self) -> Result<PollStatus> { todo!() }
    fn io_control(&self, cmd: u32, data: usize) -> Result<usize> { todo!() }
    fn metadata(&self) -> Result<Metadata> { todo!() }
    fn as_any_ref(&self) -> &dyn Any { self }
}
