//! System V shared memory: the per-process attachment record + global registry.
//!
//! The actual shared frames are a `SharedGuard` from the memory module; this
//! file only handles naming (key -> frames) and per-process bookkeeping. The
//! page-table mapping itself is done in `syscall/ipc.rs` via the memory
//! module's `Shared` handler.

use crate::memory::GlobalFrameAlloc;
use alloc::collections::BTreeMap;
use alloc::sync::{Arc, Weak};
use rcore_memory::memory_set::handler::SharedGuard;
use rcore_memory::VirtAddr;
use spin::RwLock;

lazy_static! {
    /// Global key -> shared-frames registry (weak, so unused regions are freed).
    static ref KEY2SHM: RwLock<BTreeMap<usize, Weak<spin::Mutex<SharedGuard<GlobalFrameAlloc>>>>> =
        RwLock::new(BTreeMap::new());
}

/// Per-process record of a shared-memory attachment.
#[derive(Clone)]
pub struct ShmIdentifier {
    /// Virtual address where attached in this process (0 = not attached yet).
    pub addr: VirtAddr,
    /// Strong reference to the shared frames.
    pub shared_guard: Arc<spin::Mutex<SharedGuard<GlobalFrameAlloc>>>,
}

impl ShmIdentifier {
    /// Record the attach address.
    pub fn set_addr(&mut self, addr: VirtAddr) {
        self.addr = addr;
    }

    /// shmget: get-or-create the shared frames for `key` (size `memsize`).
    pub fn new_shared_guard(
        key: usize,
        memsize: usize,
    ) -> Arc<spin::Mutex<SharedGuard<GlobalFrameAlloc>>> {
        let mut key2shm = KEY2SHM.write();
        // Already exists and is alive → share it.
        if let Some(weak_guard) = key2shm.get(&key) {
            if let Some(guard) = weak_guard.upgrade() {
                return guard;
            }
        }
        // Create a new set of shared frames, register under the key (weak, so
        // the guard is freed when all users detach).
        let shared_guard = Arc::new(spin::Mutex::new(SharedGuard::new_with_size(
            GlobalFrameAlloc,
            memsize,
        )));
        key2shm.insert(key, Arc::downgrade(&shared_guard));
        shared_guard
    }
}
