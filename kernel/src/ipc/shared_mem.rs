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
        todo!()
    }
}
