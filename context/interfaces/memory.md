# memory.rs — Interface

## Imports

```rust
// External crates
use rcore_memory::*;                          // MemorySet, MemoryArea, MemoryAttr, handlers
pub use rcore_memory::memory_set::{handler::*, MemoryArea, MemoryAttr};
pub type MemorySet = rcore_memory::memory_set::MemorySet<PageTableImpl>;

use bitmap_allocator::BitAlloc256M;           // x86_64: up to 1T
use bitmap_allocator::BitAlloc1M;             // riscv/aarch64/mips: up to 4G
use buddy_system_allocator::{LockedHeapWithRescue, Heap};

// Arch
pub use crate::arch::paging::*;               // PageTableImpl, phys_to_virt, etc.

// Internal modules
use crate::sync::SpinNoIrqLock;
use crate::consts::{KERNEL_OFFSET, MEMORY_OFFSET, PHYSICAL_MEMORY_OFFSET};
use crate::process::current_thread;
```

## Exports

```rust
// === Frame Allocator ===
// Architecture-dependent bitmap allocator type
#[cfg(target_arch = "x86_64")]
pub type FrameAlloc = BitAlloc256M;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64",
          target_arch = "aarch64", target_arch = "mips"))]
pub type FrameAlloc = BitAlloc1M;

pub static FRAME_ALLOCATOR: SpinNoIrqLock<FrameAlloc>;

// GlobalFrameAlloc implements rcore_memory::FrameAllocator trait
pub struct GlobalFrameAlloc;
impl FrameAllocator for GlobalFrameAlloc {
    fn alloc(&self) -> Option<PhysAddr>;
    fn alloc_contiguous(&self, size: usize, align_log2: usize) -> Option<PhysAddr>;
    fn dealloc(&self, target: PhysAddr);
}

// Free functions
pub fn alloc_frame() -> Option<usize>;                    // returns PhysAddr
pub fn dealloc_frame(target: usize);
pub fn alloc_frame_contiguous(size: usize, align_log2: usize) -> Option<usize>;

// === Kernel Stack ===
pub struct KernelStack(usize);   // 16KB per thread
impl KernelStack {
    pub fn new() -> Self;
    pub fn top(&self) -> usize;  // stack pointer = base + KSTK_SZ
}

// === Heap ===
pub fn init_heap();   // initializes the global HEAP_ALLOCATOR from a static array
pub fn enlarge_heap(heap: &mut Heap);  // called by LockedHeapWithRescue on OOM

// === Page Fault Handling ===
pub fn handle_page_fault(addr: usize) -> bool;
pub fn handle_page_fault_ext(addr: usize, access: AccessType) -> bool;

// === User Memory Access ===
pub fn access_ok(addr: usize, len: usize) -> bool;
pub unsafe extern "C" fn read_user_fixup() -> usize;
pub fn copy_from_user<T>(addr: *const T) -> Option<T>;
pub fn copy_to_user<T>(addr: *mut T, src: *const T) -> bool;

// === Re-exported from rcore_memory for callers ===
pub struct MemoryAttr { /* builder: .user().readonly().execute().writable().mmio() */ }
pub trait MemoryHandler { map, unmap, clone_map, handle_page_fault, handle_page_fault_ext }
pub struct Linear { offset: isize }         // identity/kernel mapping
pub struct ByFrame<T: FrameAllocator>       // eager per-page alloc
pub struct Delay<T: FrameAllocator>         // demand paging (fault → alloc + zero)
pub struct File<F: Read, T: FrameAllocator> // file-backed mmap
pub struct Shared<T: FrameAllocator>        // shared memory (IPC)
pub struct SharedGuard<T: FrameAllocator> { /* ... */ }
pub type MemorySet = MemorySet<PageTableImpl>;
```
