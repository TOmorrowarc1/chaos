# ipc/ — Interface

## Imports

```rust
// Memory
use crate::memory::{FrameAllocator, GlobalFrameAlloc};
use rcore_memory::memory_set::handler::{Shared, SharedGuard};
use rcore_memory::{PhysAddr, VirtAddr, PAGE_SIZE};

// Sync
use crate::sync::{Semaphore, SpinLock as Mutex};

// Syscall
use crate::syscall::{SemBuf, SysError, SysResult, TimeSpec};

// External
use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::ops::Index;
use spin::RwLock;
use bitflags::bitflags;
```

## Exports

```rust
// === SemProc (per-process semaphore table) ===
pub type SemId = usize;

#[derive(Default)]
pub struct SemProc {
    arrays: BTreeMap<SemId, Arc<SemArray>>,
    undos: BTreeMap<(SemId, u16), i16>,
}

impl SemProc {
    pub fn add(&mut self, array: Arc<SemArray>) -> SemId;
    pub fn remove(&mut self, id: SemId);
    pub fn get(&self, id: SemId) -> Option<&Arc<SemArray>>;
    pub fn get_free_id(&self) -> SemId;
    pub fn add_undo(&mut self, id: SemId, num: u16, op: i16);
}
impl Clone for SemProc { /* deep clone arrays, copy undos */ }
impl Drop for SemProc { /* auto-perform undo on all pending semops */ }

// === SemArray (System V semaphore set) ===
pub struct SemArray {
    pub semid_ds: Mutex<SemidDs>,
    sems: Vec<Semaphore>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct IpcPerm {
    pub key: u32,
    pub uid: u32, pub gid: u32, pub cuid: u32, pub cgid: u32,
    pub mode: u32, pub __seq: u32,
    pub __pad1: usize, pub __pad2: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SemidDs {
    pub perm: IpcPerm,
    pub otime: usize,
    __pad1: usize,
    pub ctime: usize,
    __pad2: usize,
    pub nsems: usize,
}

impl SemArray {
    pub fn remove(&self);
    pub fn otime(&self);
    pub fn ctime(&self);
    pub fn set(&self, new: &SemidDs);
    pub fn get_or_create(key: u32, nsems: usize, flags: usize)
        -> Result<Arc<Self>, SysError>;
}

impl Index<usize> for SemArray { /* index into sems vec */ }

// === ShmProc (per-process shared memory table) ===
pub type ShmId = usize;

pub struct ShmIdentifier {
    pub addr: VirtAddr,
    pub shared_guard: Arc<spin::Mutex<SharedGuard<GlobalFrameAlloc>>>,
}

impl ShmIdentifier {
    pub fn set_addr(&mut self, addr: VirtAddr);
    pub fn new_shared_guard(key: usize, memsize: usize)
        -> Arc<spin::Mutex<SharedGuard<GlobalFrameAlloc>>>;
}

#[derive(Default)]
pub struct ShmProc {
    shm_identifiers: BTreeMap<ShmId, ShmIdentifier>,
}

impl ShmProc {
    pub fn add(&mut self, shared_guard: Arc<spin::Mutex<SharedGuard<GlobalFrameAlloc>>>) -> ShmId;
    pub fn get(&self, id: ShmId) -> Option<&ShmIdentifier>;
    pub fn set(&mut self, id: ShmId, identifier: ShmIdentifier);
    pub fn get_id(&self, addr: VirtAddr) -> Option<ShmId>;
    pub fn pop(&mut self, id: ShmId);
}
impl Clone for ShmProc { /* ... */ }
```
