use crate::task::Process;
use crate::sync::SpinNoIrqLock;
use crate::syscall::{SysError, SysResult};
use alloc::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct EpollInstance {
    pub events: BTreeMap<usize, EpollEvent>,
    pub ready_list: SpinNoIrqLock<BTreeSet<usize>>,
    pub new_ctl_list: SpinNoIrqLock<BTreeSet<usize>>,
}

impl EpollInstance {
    pub fn new(_flags: usize) -> Self {
        EpollInstance {
            events: BTreeMap::new(),
            ready_list: Default::default(),
            new_ctl_list: Default::default(),
        }
    }

    pub fn control(&mut self, op: usize, fd: usize, event: &EpollEvent) -> SysResult {
        todo!()
    }
}

#[derive(Clone, Copy)]
pub struct EpollData { _ptr: u64 }

#[derive(Clone)]
pub struct EpollEvent {
    pub events: u32,
    pub data: EpollData,
}

impl EpollEvent {
    pub const EPOLLIN: u32 = 0x001;
    pub const EPOLLOUT: u32 = 0x004;
    pub const EPOLLERR: u32 = 0x008;
    pub const EPOLLHUP: u32 = 0x010;
    pub const EPOLLPRI: u32 = 0x002;
    pub const EPOLLRDNORM: u32 = 0x040;
    pub const EPOLLRDBAND: u32 = 0x080;
    pub const EPOLLWRNORM: u32 = 0x100;
    pub const EPOLLWRBAND: u32 = 0x200;
    pub const EPOLLMSG: u32 = 0x400;
    pub const EPOLLRDHUP: u32 = 0x2000;
    pub const EPOLLEXCLUSIVE: u32 = 1 << 28;
    pub const EPOLLWAKEUP: u32 = 1 << 29;
    pub const EPOLLONESHOT: u32 = 1 << 30;
    pub const EPOLLET: u32 = 1 << 31;

    pub fn contains(&self, events: u32) -> bool {
        (self.events & events) != 0
    }
}

pub struct EPollCtlOp;
impl EPollCtlOp {
    pub const ADD: i32 = 1;
    pub const DEL: i32 = 2;
    pub const MOD: i32 = 3;
}

impl Process {
    pub fn get_epoll_instance_mut(&mut self, fd: usize) -> Result<&mut EpollInstance, SysError> {
        todo!()
    }

    pub fn get_epoll_instance(&self, fd: usize) -> Result<&EpollInstance, SysError> {
        todo!()
    }
}
