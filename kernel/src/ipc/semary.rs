//! A System V semaphore set (`SemArray`) and its metadata.

use crate::sync::Semaphore;
use crate::sync::SpinLock as Mutex;
use crate::syscall::{SysError, TimeSpec};
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
        &self.sems[idx]
    }
}

lazy_static! {
    /// Global key -> semaphore-set registry (weak, so unused sets are freed).
    static ref KEY2SEM: RwLock<BTreeMap<u32, Weak<SemArray>>> = RwLock::new(BTreeMap::new());
}

impl SemArray {
    /// IPC_RMID: remove the set, unregister its key, and wake all waiters (EIDRM).
    pub fn remove(&self) {
        let mut key2sem = KEY2SEM.write();
        let key = self.semid_ds.lock().perm.key;
        key2sem.remove(&key);
        for sem in self.sems.iter() {
            // Wakes every blocked waiter; each will re-poll, see `removed`, and return EIDRM.
            sem.remove();
        }
    }

    /// Stamp the last-semop time.
    pub fn otime(&self) {
        self.semid_ds.lock().otime = TimeSpec::get_epoch().sec;
    }

    /// Stamp the last-change time.
    pub fn ctime(&self) {
        self.semid_ds.lock().ctime = TimeSpec::get_epoch().sec;
    }

    /// IPC_SET: update ownership / permissions from `new`.
    pub fn set(&self, new: &SemidDs) {
        let mut lock = self.semid_ds.lock();
        lock.perm.uid = new.perm.uid;
        lock.perm.gid = new.perm.gid;
        lock.perm.mode = new.perm.mode & 0x1ff; // low 9 bits
    }

    /// semget: get the set with `key`, or create a new one with `nsems` elements.
    pub fn get_or_create(mut key: u32, nsems: usize, flags: usize) -> Result<Arc<Self>, SysError> {
        let mut key2sem = KEY2SEM.write();
        let flag = SemGetFlag::from_bits_truncate(flags);

        if key == 0 {
            // IPC_PRIVATE: allocate a fresh, unused key.
            key = (1u32..).find(|i| key2sem.get(i).is_none()).unwrap();
        } else {
            // Check whether the key already resolves to a live set.
            if let Some(weak_array) = key2sem.get(&key) {
                if let Some(array) = weak_array.upgrade() {
                    if flag.contains(SemGetFlag::CREAT) && flag.contains(SemGetFlag::EXCLUSIVE) {
                        return Err(SysError::EEXIST);
                    }
                    return Ok(array);
                }
            }
        }

        // Not found (or expired), create one.
        let mut semaphores = Vec::new();
        for _ in 0..nsems {
            semaphores.push(Semaphore::new(0));
        }

        let array = Arc::new(SemArray {
            semid_ds: Mutex::new(SemidDs {
                perm: IpcPerm {
                    key,
                    uid: 0,
                    gid: 0,
                    cuid: 0,
                    cgid: 0,
                    mode: (flags as u32) & 0x1ff,
                    __seq: 0,
                    __pad1: 0,
                    __pad2: 0,
                },
                otime: 0,
                ctime: TimeSpec::get_epoch().sec,
                nsems,
                __pad1: 0,
                __pad2: 0,
            }),
            sems: semaphores,
        });
        // Register a Weak so other processes can find the set by key, but the
        // set is freed when all users drop their Arcs.
        key2sem.insert(key, Arc::downgrade(&array));
        Ok(array)
    }
}
