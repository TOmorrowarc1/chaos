//! System V IPC: per-process semaphore and shared-memory tables.
//!
//! The actual kernel objects live in `semary.rs` (`SemArray`) and
//! `shared_mem.rs` (`SharedGuard` via the memory module). This file holds the
//! per-process *handle tables* that `semget`/`shmget` index into.

mod semary;
mod shared_mem;

pub use self::semary::*;
pub use self::shared_mem::*;

use crate::memory::GlobalFrameAlloc;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use log::*;
use rcore_memory::memory_set::handler::SharedGuard;
use rcore_memory::VirtAddr;

/// Semaphore set id (within a process).
type SemId = usize;
/// Shared-memory id (within a process).
type ShmId = usize;
/// Semaphore number (within an array).
type SemNum = u16;
/// Semaphore operation value.
type SemOp = i16;

/// Per-process System V semaphore table.
#[derive(Default)]
pub struct SemProc {
    /// Semaphore arrays, keyed by per-process id.
    arrays: BTreeMap<SemId, Arc<SemArray>>,
    /// Undo operations, applied when the process terminates (SEM_UNDO).
    undos: BTreeMap<(SemId, SemNum), SemOp>,
}

impl SemProc {
    /// Insert `array`, returning its new id.
    pub fn add(&mut self, array: Arc<SemArray>) -> SemId {
        let id = self.get_free_id();
        self.arrays.insert(id, array);
        id
    }

    /// Remove an array by id.
    pub fn remove(&mut self, id: SemId) {
        self.arrays.remove(&id);
    }

    /// Lowest free id.
    fn get_free_id(&self) -> SemId {
        (0..).find(|i| self.arrays.get(i).is_none()).unwrap()
    }

    /// Get an array by id (owned clone).
    pub fn get(&self, id: SemId) -> Option<Arc<SemArray>> {
        self.arrays.get(&id).map(|a| a.clone())
    }

    /// Record an undo operation for SEM_UNDO.
    pub fn add_undo(&mut self, id: SemId, num: SemNum, op: SemOp) {
        let old_val = *self.undos.get(&(id, num)).unwrap_or(&0);
        // Accumulate the reverse operation so we can restore on exit.
        let new_val = old_val - op;
        self.undos.insert((id, num), new_val);
    }
}

/// Fork: share the arrays, clear undo bookkeeping.
impl Clone for SemProc {
    fn clone(&self) -> Self {
        SemProc {
            arrays: self.arrays.clone(),
            undos: BTreeMap::default(),
        }
    }
}

/// Auto-perform SEM_UNDO on process exit.
impl Drop for SemProc {
    fn drop(&mut self) {
        for (&(id, num), &op) in self.undos.iter() {
            debug!("semundo: id: {}, num: {}, op: {}", id, num, op);
            let sem_array = self.arrays[&id].clone();
            let sem = &sem_array[num as usize];
            match op {
                1 => sem.release(),
                0 => {}
                _ => unimplemented!("Semaphore: semundo.(Not 1)"),
            }
        }
    }
}

/// Per-process System V shared-memory table.
#[derive(Default)]
pub struct ShmProc {
    shm_identifiers: BTreeMap<ShmId, ShmIdentifier>,
}

impl ShmProc {
    /// Insert the `SharedGuard`, returning its new id.
    pub fn add(&mut self, shared_guard: Arc<spin::Mutex<SharedGuard<GlobalFrameAlloc>>>) -> ShmId {
        let id = self.get_free_id();
        let shm_identifier = ShmIdentifier {
            addr: 0,
            shared_guard,
        };
        self.shm_identifiers.insert(id, shm_identifier);
        id
    }

    /// Lowest free id.
    fn get_free_id(&self) -> ShmId {
        (0..)
            .find(|i| self.shm_identifiers.get(i).is_none())
            .unwrap()
    }

    /// Get an attachment record by id (owned clone).
    pub fn get(&self, id: ShmId) -> Option<ShmIdentifier> {
        self.shm_identifiers.get(&id).map(|a| a.clone())
    }

    /// Set (overwrite) the record for `id` (used by shmat to record the address).
    pub fn set(&mut self, id: ShmId, shm_id: ShmIdentifier) {
        self.shm_identifiers.insert(id, shm_id);
    }

    /// Reverse-lookup the id by attach virtual address (used by shmdt).
    pub fn get_id(&self, addr: VirtAddr) -> Option<ShmId> {
        for (key, value) in &self.shm_identifiers {
            if value.addr == addr {
                return Some(*key);
            }
        }
        None
    }

    /// Detach: remove the record for `id`.
    pub fn pop(&mut self, id: ShmId) {
        self.shm_identifiers.remove(&id);
    }
}

/// Fork: clone the attachment table (children inherit, sharing frames via Arc).
impl Clone for ShmProc {
    fn clone(&self) -> Self {
        ShmProc {
            shm_identifiers: self.shm_identifiers.clone(),
        }
    }
}
