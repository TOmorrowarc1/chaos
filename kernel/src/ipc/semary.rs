//! A System V semaphore set (`SemArray`) and its metadata.

use crate::sync::Semaphore;
use crate::sync::SpinLock as Mutex;
use crate::syscall::SysError;
use alloc::{collections::BTreeMap, sync::Arc, sync::Weak, vec::Vec};
use bitflags::*;
use core::ops::Index;
use spin::RwLock;

bitflags! {
    struct SemGetFlag: usize {
        const CREAT = 1 << 9;
        const EXCLUSIVE = 1 << 10;
        const NO_WAIT = 1 << 11;
    }
}

/// `struct ipc_perm` — access permissions / ownership of the set.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct IpcPerm {
    pub key: u32,   // Key supplied to semget(2)
    pub uid: u32,   // Effective UID of owner
    pub gid: u32,   // Effective GID of owner
    pub cuid: u32,  // Effective UID of creator
    pub cgid: u32,  // Effective GID of creator
    pub mode: u32,  // Permissions (low 9 bits)
    pub __seq: u32, // Sequence number
    pub __pad1: usize,
    pub __pad2: usize,
}

/// `struct semid_ds` — metadata for semctl(IPC_STAT / IPC_SET).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SemidDs {
    pub perm: IpcPerm, // Ownership and permissions
    pub otime: usize,  // Last semop time
    __pad1: usize,
    pub ctime: usize, // Last change time
    __pad2: usize,
    pub nsems: usize, // Number of semaphores in the set
}

/// A System V semaphore set.
pub struct SemArray {
    pub semid_ds: Mutex<SemidDs>,
    sems: Vec<Semaphore>,
}

impl Index<usize> for SemArray {
    type Output = Semaphore;
    fn index(&self, idx: usize) -> &Semaphore {
        todo!()
    }
}

lazy_static! {
    /// Global key -> semaphore-set registry (weak, so unused sets are freed).
    static ref KEY2SEM: RwLock<BTreeMap<u32, Weak<SemArray>>> = RwLock::new(BTreeMap::new());
}

impl SemArray {
    /// IPC_RMID: remove the set, unregister its key, and wake all waiters (EIDRM).
    pub fn remove(&self) {
        todo!()
    }

    /// Stamp the last-semop time.
    pub fn otime(&self) {
        todo!()
    }

    /// Stamp the last-change time.
    pub fn ctime(&self) {
        todo!()
    }

    /// IPC_SET: update ownership / permissions from `new`.
    pub fn set(&self, new: &SemidDs) {
        todo!()
    }

    /// semget: get the set with `key`, or create a new one with `nsems` elements.
    pub fn get_or_create(key: u32, nsems: usize, flags: usize) -> Result<Arc<Self>, SysError> {
        todo!()
    }
}
