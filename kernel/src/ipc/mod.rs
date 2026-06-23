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

/// Per-process System V shared-memory table.
#[derive(Default)]
pub struct ShmProc {
    shm_identifiers: BTreeMap<ShmId, ShmIdentifier>,
}

impl SemProc {
    /// Insert `array`, returning its new id.
    pub fn add(&mut self, array: Arc<SemArray>) -> SemId {
        todo!()
    }

    /// Remove an array by id.
    pub fn remove(&mut self, id: SemId) {
        todo!()
    }

    /// Lowest free id.
    fn get_free_id(&self) -> SemId {
        todo!()
    }

    /// Get an array by id (owned clone).
    pub fn get(&self, id: SemId) -> Option<Arc<SemArray>> {
        todo!()
    }

    /// Record an undo operation for SEM_UNDO.
    pub fn add_undo(&mut self, id: SemId, num: SemNum, op: SemOp) {
        todo!()
    }
}

/// Fork: share the arrays, clear undo bookkeeping.
impl Clone for SemProc {
    fn clone(&self) -> Self {
        todo!()
    }
}

/// Auto-perform SEM_UNDO on process exit.
impl Drop for SemProc {
    fn drop(&mut self) {
        todo!()
    }
}

impl ShmProc {
    /// Insert the `SharedGuard`, returning its new id.
    pub fn add(&mut self, shared_guard: Arc<spin::Mutex<SharedGuard<GlobalFrameAlloc>>>) -> ShmId {
        todo!()
    }

    /// Lowest free id.
    fn get_free_id(&self) -> ShmId {
        todo!()
    }

    /// Get an attachment record by id (owned clone).
    pub fn get(&self, id: ShmId) -> Option<ShmIdentifier> {
        todo!()
    }

    /// Set (overwrite) the record for `id` (used by shmat to record the address).
    pub fn set(&mut self, id: ShmId, shm_id: ShmIdentifier) {
        todo!()
    }

    /// Reverse-lookup the id by attach virtual address (used by shmdt).
    pub fn get_id(&self, addr: VirtAddr) -> Option<ShmId> {
        todo!()
    }

    /// Detach: remove the record for `id`.
    pub fn pop(&mut self, id: ShmId) {
        todo!()
    }
}

/// Fork: clone the attachment table (children inherit, sharing frames via Arc).
impl Clone for ShmProc {
    fn clone(&self) -> Self {
        todo!()
    }
}
