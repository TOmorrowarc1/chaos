# Chaos OS Memory Module Reference

## 1. OS-Level Concept: Virtual Memory Foundation

### Paging

Physical RAM is divided into 4 KiB frames (also called pages). Each frame has a physical address (`PhysAddr`). The kernel manages which frames are free and which are allocated by the frame allocator.

A CPU with an MMU translates every address the program sees (virtual address, `VirtAddr`) to a physical address (`PhysAddr`) via a **page table**. The page table is a multi-level tree in memory, indexed by virtual address, whose leaves are page table entries (PTEs). Each PTE stores:

- **Physical frame number** (PPN) — which physical frame this virtual page maps to
- **Permission bits** — R (readable), W (writable), X (executable), U (user-mode accessible)
- **Status bits** — V (valid/present), A (accessed), D (dirty) — set by hardware
- **Software bits** (RSW) — reserved for OS use (COW marking, swap flags)

On RISC-V Sv39, the page table has three levels. A virtual address is split as:

```
  38    30 29    21 20    12 11        0
┌─────────┬─────────┬─────────┬──────────┐
│ VPN[2]  │ VPN[1]  │ VPN[0]  │ offset   │
└─────────┴─────────┴─────────┴──────────┘
```

Each level indexes into a 4 KiB page table page containing 512 PTEs (8 bytes each). The final PTE's PPN + page offset gives the physical address.

### Privilege Levels

RISC-V has three privilege modes: M (machine), S (supervisor/kernel), U (user). The page table's U bit controls user access:
- **U=0**: only S-mode (kernel) can access
- **U=1**: both S-mode and U-mode can access

Kernel sets `sstatus.SUM` (permit supervisor user-memory access) to allow kernel code to read/write user pages when needed (e.g., `copy_from_user`).

A page fault occurs when:
- V=0 → page not present (demand paging, swapped out)
- R=0, W=1, X=1 → invalid permission combination (RISC-V requires R=1 for W or X)
- U=0 + access from user mode → privilege violation
- W=1 + write to read-only page → COW trigger

The CPU saves the fault address in `stval` and the trapping instruction address in `sepc`, then jumps to the trap handler.

### The Active Page Table

The `satp` CSR holds the physical page number of the root page table. Switching `satp` switches the entire address space — this is the core of process isolation. Each process has its own `satp` value (called `token`), and context switching involves:
1. Save old process's registers + `satp`
2. Write new process's `satp`
3. `sfence.vma` (flush TLB)
4. Restore new process's registers

---

## 2. The Memory Module's Role

The memory module lives in `rcore_memory` (external crate at `crate/memory/`) with architecture-specific glue in `arch/riscv/paging.rs` and `arch/riscv/memory.rs`. It provides a complete virtual memory abstraction to the rest of the kernel (process, syscall, trap handlers).

### Boundaries

```
Other modules (process, syscall, trap)
  │
  ▼
MemorySet<T: PageTableExt>       ← high-level API: push, pop, handle_page_fault, translate
  │
  ├── Vec<MemoryArea>            ← region descriptors: [start, end) + attr + handler
  │     └── Box<dyn MemoryHandler>  ← per-page strategy: map, unmap, fault
  │
  └── T: PageTable + PageTableExt  ← arch-specific page table
        │
        └── arch/riscv/paging.rs    ← RISC-V Sv39 PTE format, SATP register

FrameAllocator trait              ← physical frame allocator interface
  └── rCore/src/memory.rs          ← bitmap_allocator::BitAlloc1M backing
```

### Lock-Free Design

`MemorySet` and `MemoryArea` contain **no locks**. They assume single-threaded or externally-synchronized access. The process module wraps each `Thread`'s `MemorySet` in `Arc<Mutex<MemorySet>>` (see `rCore/src/process/thread.rs`):

```rust
pub struct Thread {
    pub vm: Arc<Mutex<MemorySet>>,    // the address space, mutex-protected
    pub proc: Arc<Mutex<Process>>,
    pub tid: Tid,
}
```

Every operation on a process's memory (`push`, `pop`, `handle_page_fault`, `translate`, `clone`) locks the `Mutex<MemorySet>` first. The lock-free interior means handlers can be called recursively or from interrupt context without deadlock risk, as long as the outer `Mutex<MemorySet>` is not held re-entrantly.

The `FrameAllocator` implementations may have their own internal locks (e.g., `FRAME_ALLOCATOR: SpinNoIrqLock<BitAlloc1M>`).

---

## 3. Page Table & Entry Traits

### `PageTable` Trait — Object-safe, used by `MemoryHandler`

```rust
pub trait PageTable {
    fn map(&mut self, addr: VirtAddr, target: PhysAddr) -> &mut dyn Entry;
    fn unmap(&mut self, addr: VirtAddr);
    fn get_entry(&mut self, addr: VirtAddr) -> Option<&mut dyn Entry>;
    fn get_page_slice_mut<'a>(&mut self, addr: VirtAddr) -> &'a mut [u8];
    fn flush_cache_copy_user(&mut self, start: VirtAddr, end: VirtAddr, execute: bool);
}
```

| Method | Hardware Action | Called By |
|---|---|---|
| `map(addr, target)` | Walk page table, write PTE with `PPN=target/4096, V=1, R=1, W=1`. Alloc intermediate page table pages via `FrameAllocatorForRiscv` if needed. Returns `&mut Entry` for caller to set permissions. | `MemoryArea::map()` → handler's `map()` |
| `unmap(addr)` | Walk page table, clear PTE (`V=0`). Requires PTE to be present. | `MemoryArea::unmap()` → handler's `unmap()` |
| `get_entry(addr)` | Walk page table, return `&mut Entry` if present. | handler's `handle_page_fault()` reads PTE state |
| `get_page_slice_mut(addr)` | Translate VA→PA, return `&mut [u8; 4096]` via `phys_addr + PHYSICAL_MEMORY_OFFSET`. | handler copies data into frame (file data, zero-init) |
| `flush_cache_copy_user(...)` | Flush I/D cache after writing user code. No-op on RISC-V. | After writing executable pages in clone |

### `Entry` Trait — Single PTE field accessors

```rust
pub trait Entry {
    fn update(&mut self);                            // sfence.vma for this page
    fn accessed(&self) -> bool;                      // A bit
    fn dirty(&self) -> bool;                         // D bit
    fn writable(&self) -> bool;                      // W bit
    fn present(&self) -> bool;                       // V bit
    fn clear_accessed(&mut self);
    fn clear_dirty(&mut self);
    fn set_writable(&mut self, value: bool);
    fn set_present(&mut self, value: bool);
    fn target(&self) -> PhysAddr;                    // PPN << 12
    fn set_target(&mut self, target: PhysAddr);
    fn writable_shared(&self) -> bool;               // RSW[0]: COW writable-shared
    fn readonly_shared(&self) -> bool;               // RSW[1]: COW read-only-shared
    fn set_shared(&mut self, writable: bool);
    fn clear_shared(&mut self);
    fn swapped(&self) -> bool;                       // RSW[0]: swapped out
    fn set_swapped(&mut self, value: bool);
    fn user(&self) -> bool;                          // U bit
    fn set_user(&mut self, value: bool);
    fn execute(&self) -> bool;                       // X bit
    fn set_execute(&mut self, value: bool);
    fn mmio(&self) -> u8;
    fn set_mmio(&mut self, value: u8);
}
```

### `PageTableExt` Trait — Construction and lifecycle (not object-safe)

```rust
pub trait PageTableExt: PageTable + Sized {
    fn new() -> Self { ... }            // new_bare() + map_kernel()
    fn new_bare() -> Self;             // alloc root frame, zero it
    fn map_kernel(&mut self);          // write kernel superpage entries
    fn token(&self) -> usize;          // SATP value = root_frame.ppn | mode
    unsafe fn set_token(token: usize); // satp::write()
    fn active_token() -> usize;        // satp::read()
    fn flush_tlb();                    // sfence.vma all
    unsafe fn activate(&self);         // switch to this page table
    unsafe fn with<T>(&self, f: impl FnOnce() -> T) -> T;  // temp switch
}
```

### RISC-V Implementation: `PageTableImpl` (`arch/riscv/paging.rs`)

```rust
pub struct PageTableImpl {
    page_table: TopLevelPageTable<'static>,  // Rv39PageTable from riscv crate
    root_frame: Frame,                        // physical frame of root table
    entry: Option<PageEntry>,                 // cached last entry for &mut dyn Entry
}
```

`TopLevelPageTable` wraps the raw 512-entry table in kernel-mapped virtual memory (`phys_to_virt(root_frame_addr)`). The `riscv` crate's `Mapper` handles the three-level page table walk.

`FrameAllocatorForRiscv` bridges the `riscv` crate's internal allocator trait (needed by `Mapper::map_to` for intermediate page tables) to the kernel's `alloc_frame()`:

```rust
struct FrameAllocatorForRiscv;
impl FrameAllocatorFor<PhysAddr> for FrameAllocatorForRiscv {
    fn alloc(&mut self) -> Option<Frame> {
        alloc_frame().map(|addr| Frame::of_addr(PhysAddr::new_u64(addr as u64)))
    }
}
```

---

## 4. Frame Allocator

### Trait

```rust
pub trait FrameAllocator: Debug + Clone + Send + Sync + 'static {
    fn alloc(&self) -> Option<PhysAddr>;                              // single frame
    fn alloc_contiguous(&self, size: usize, align_log2: usize) -> Option<PhysAddr>;
    fn dealloc(&self, target: PhysAddr);
}
```

### Reference Implementation (`rCore/src/memory.rs`)

```rust
pub type FrameAlloc = bitmap_allocator::BitAlloc1M;
pub static FRAME_ALLOCATOR: SpinNoIrqLock<FrameAlloc> = ...;

pub struct GlobalFrameAlloc;

impl FrameAllocator for GlobalFrameAlloc {
    fn alloc(&self) -> Option<PhysAddr> {
        FRAME_ALLOCATOR.lock().alloc()
            .map(|id| id * PAGE_SIZE + MEMORY_OFFSET)
    }
    fn dealloc(&self, target: PhysAddr) {
        FRAME_ALLOCATOR.lock().dealloc((target - MEMORY_OFFSET) / PAGE_SIZE);
    }
}
```

`BitAlloc1M` manages a bitmap of up to 1M frames (4 GiB of physical memory). It is initialized in `arch/riscv/memory.rs::init_frame_allocator()` with the range `[kernel_end, MEMORY_END)`.

The `alloc_frame()` / `dealloc_frame()` free functions are thin wrappers:

```rust
pub fn alloc_frame() -> Option<usize> { GlobalFrameAlloc.alloc() }
pub fn dealloc_frame(target: usize) { GlobalFrameAlloc.dealloc(target); }
```

---

## 5. Memory Area

```rust
/// A continuous memory space with the same attribute
pub struct MemoryArea {
    start_addr: VirtAddr,                 // first virtual address (page-aligned)
    end_addr: VirtAddr,                   // first address AFTER the area (exclusive)
    attr: MemoryAttr,                     // user/writable/execute/mmio flags
    handler: Box<dyn MemoryHandler>,      // per-page strategy
    name: &'static str,                   // debug label
}
```

### Fields

| Field | Meaning |
|---|---|
| `start_addr` | Lowest virtual address in this region (page-aligned by `MemorySet::push()`) |
| `end_addr` | Exclusive upper bound (page-aligned) |
| `attr` | Permission flags applied to each PTE in this region |
| `handler` | Strategy object: controls how each page in this region is mapped, unmapped, and page-fault-resolved |
| `name` | Human-readable label for debugging ("elf", "heap", "user_stack") |

### `MemoryAttr` — Permission flags

```rust
pub struct MemoryAttr {
    user: bool,        // U bit: user-mode accessible
    readonly: bool,    // !W bit: writes forbidden
    execute: bool,     // X bit: instruction fetch allowed
    mmio: u8,          // MMIO hint (unused on RISC-V)
}

// Builder API:
MemoryAttr::default().user().writable().execute()
```

`attr.apply(entry)` sets the PTE's user/writable/execute bits and calls `entry.update()`.

### Methods

| Method | What it does |
|---|---|
| `contains(addr)` | `addr >= start && addr < end` |
| `map(pt)` | Iterates all pages in range, calls `handler.map(pt, page_addr, &attr)` for each |
| `unmap(pt)` | Iterates all pages, calls `handler.unmap(pt, page_addr)` for each |
| `is_overlap_with(s, e)` | Page-level overlap test |
| `check_read_array(ptr, count)` | Returns max contiguous readable bytes in this area from `ptr` |
| `check_write_array(ptr, count)` | Same but rejects readonly areas |

---

## 6. Memory Handler — Page-Level Strategy

```rust
pub trait MemoryHandler: Debug + Send + Sync + 'static {
    fn box_clone(&self) -> Box<dyn MemoryHandler>;
    fn map(&self, pt: &mut dyn PageTable, addr: VirtAddr, attr: &MemoryAttr);
    fn unmap(&self, pt: &mut dyn PageTable, addr: VirtAddr);
    fn clone_map(&self, pt: &mut dyn PageTable, src_pt: &mut dyn PageTable,
                  addr: VirtAddr, attr: &MemoryAttr);
    fn handle_page_fault(&self, pt: &mut dyn PageTable, addr: VirtAddr) -> bool;
    fn handle_page_fault_ext(&self, pt: &mut dyn PageTable, addr: VirtAddr,
                              access: AccessType) -> bool;
}
```

The handler answers: **"what happens to this single page when someone maps it, unmaps it, copies it, or faults on it?"**

### Implementations

#### `Linear` — Identity/kernel mapping

```rust
pub struct Linear { offset: isize }
// map(addr):  pt.map(addr, addr + offset)
// unmap(addr): pt.unmap(addr)
// handle_page_fault: false (always present, never faults)
```

`Linear::new(PHYSICAL_MEMORY_OFFSET as isize)` maps kernel text/data — every VA maps to `phys_addr = va - KERNEL_OFFSET + MEM_OFFSET`. Used for the kernel's own address space.

#### `ByFrame<T: FrameAllocator>` — Eager allocation

```rust
pub struct ByFrame<T: FrameAllocator> { allocator: T }
// map(addr):  frame = allocator.alloc(); pt.map(addr, frame); apply attr
// unmap(addr): entry = pt.get_entry(addr); allocator.dealloc(entry.target()); pt.unmap(addr)
// handle_page_fault: false (always present)
// clone_map: alloc new frame, copy data from src_pt to pt
```

Allocates a physical frame immediately on `map()`. Used for page table frames and small fixed mappings.

#### `Delay<T: FrameAllocator>` — Demand paging

```rust
pub struct Delay<T: FrameAllocator> { allocator: T }
// map(addr):  pt.map(addr, 0); set_present(false); apply attr
// unmap(addr): if present: dealloc target; pt.unmap(addr)
// handle_page_fault(addr):
//     if present → permission check
//     else → frame = allocator.alloc(); entry.set_target(frame); set_present(true);
//            zero-fill page; update();
// clone_map: if src present → eager copy; else → delay-map
```

Maps with `present=false` so the first access faults. On fault, allocates a frame, fills with zero, and marks present. Used for anonymous heap, stack, and bss — pages consume no physical memory until touched.

#### `File<F: Read, T: FrameAllocator>` — File-backed mmap

```rust
pub struct File<F, T> {
    pub file: F,           // source of data (implements Read)
    pub mem_start: usize,  // VA where the mapping starts
    pub file_start: usize, // offset in file
    pub file_end: usize,   // end offset in file
    pub allocator: T,
}
// map(addr): pt.map(addr, 0); set_present(false); apply attr
// handle_page_fault(addr):
//     frame = allocator.alloc(); set_target(frame); set_present(true);
//     read_size = file.read_at(file_offset_for(addr), page_data);
//     zero-fill remainder of page; flush cache
// clone_map: if present+!readonly → eager copy; else → delay-map
```

Used for ELF binary mapping — the binary's segments are mapped with present=false, and on page fault the handler reads the relevant portion of the file into the frame.

#### `Shared<T: FrameAllocator>` — Shared memory (IPC)

```rust
pub struct Shared<T: FrameAllocator> {
    allocator: T,
    start_virt_addr: Arc<Mutex<Option<usize>>>,
    guard: Arc<Mutex<SharedGuard<T>>>,    // shared mapping: addr_offset → PhysAddr
}
// map(addr): if first caller, records start_virt_addr;
//            if guard has phys_addr for offset → pt.map(addr, phys_addr)
//            else → pt.map(addr, 0); set_present(false)  (delay)
// handle_page_fault(addr):
//     if guard has no frame for offset → alloc via guard, map, zero-fill
//     else → map existing shared frame
```

Multiple `Shared` handlers sharing the same `Arc<Mutex<SharedGuard<T>>>` see the same physical frames — this is the basis for `mmap` with `MAP_SHARED` and `shmget`.

---

## 7. Memory Set — Per-Process Address Space

```rust
pub struct MemorySet<T: PageTableExt> {
    areas: Vec<MemoryArea>,    // sorted by start_addr
    page_table: T,             // the arch-specific page table (PageTableImpl)
}
```

### Constructor

```rust
// PageTableExt::new() = PageTableExt::new_bare() + map_kernel()
MemorySet::new()      → creates new page table with kernel mapped
MemorySet::new_bare() → creates empty page table (no kernel mapping)
```

`new()` is used for user processes — each gets a fresh page table with kernel superpages pre-mapped (indices 509,511 on Sv39). The kernel is mapped into every address space; the U bit prevents user access.

### Core APIs

#### `push(start, end, attr, handler, name)` — Add a region

1. Page-align `start` (round down) and `end` (round up)
2. Assert no overlap with existing areas
3. Create `MemoryArea { start, end, attr, handler: Box::new(handler), name }`
4. Call `area.map(&mut page_table)` — for each page, calls `handler.map(pt, page_addr, &attr)`
5. Insert into `areas` sorted by `start_addr`

```rust
// Example: add a demand-paged anonymous region at 0x10000-0x20000
ms.push(0x10000, 0x20000,
    MemoryAttr::default().user().writable(),
    Delay::new(GlobalFrameAlloc),
    "heap");
```

#### `pop(start, end)` / `pop_with_split(start, end)` — Remove a region

`pop()` removes an exact match. `pop_with_split()` handles partial overlap:
- If `[start, end)` is a subset of an area → split into two areas, unmap the middle
- If `[start, end)` overlaps prefix of an area → shrink the area from the left, unmap the overlap
- If `[start, end)` overlaps postfix → shrink from the right
- If `[start, end)` is a superset → remove entirely, unmap

Each unmapped page calls `handler.unmap(pt, page_addr)`, which deallocates the frame if the handler owns it.

#### `find_free_area(hint, len) → VirtAddr` — Find unmapped space

Brute-force: try `hint`, then try each existing area's end address. Round each candidate up to page boundary. Return the first where `test_free_area(candidate, candidate+len)` passes (no overlap with any existing area).

#### `handle_page_fault(addr) → bool` — Resolve page fault

1. Find area containing `addr` via linear scan
2. If found: delegate to `area.handler.handle_page_fault_ext(&mut page_table, addr, access)`
3. If not found: return `false` (unhandled → kernel panic / SIGSEGV)

The handler may allocate a frame, read file data, update the PTE, and return `true`.

#### `translate(addr) → Option<PhysAddr>` — Page table walk

Calls `page_table.get_entry(addr)`, returns `entry.target()` only if `entry.user() == true`. Used for `copy_from_user` / `copy_to_user` validation.

#### `clear()` — Tear down all regions

Iterates all areas, calls `area.unmap(page_table)` for each (deallocates frames, clears PTEs), then clears the areas list.

#### `clone() → Self` — Deep copy for fork

1. Create new `PageTableImpl::new()` (fresh root frame with kernel mapped)
2. For each area, for each page, call `handler.clone_map(&mut new_pt, &mut old_pt, addr, &attr)`
3. Clone the `areas` vec (handlers implement `Clone` via `box_clone()`)
4. Return `MemorySet { areas, page_table: new_pt }`

Handlers decide clone behavior:
- `Linear`: re-map the same offset (kernel identity)
- `ByFrame`: alloc new frame, copy data
- `Delay`: if src frame is present → alloc new + copy; else → delay-map
- `File`: same as Delay
- `Shared`: delay-map (actual data shared via SharedGuard)

#### `activate()` / `token()` — Page table switching

```rust
fn activate(&self) {
    let old = PageTableImpl::active_token();
    let new = self.page_table.token();
    if old != new {
        PageTableImpl::set_token(new);
        PageTableImpl::flush_tlb();
    }
}
```

Called during context switch. The `token` is the SATP value (`root_frame.ppn | (8 << 60)` for Sv39).

#### `check_read/write_ptr/array` — User pointer validation

Used by syscall handlers to verify that user-space pointers point to readable/writable memory before accessing them. Walks areas and checks coverage + permission.

### Drop

`Drop` calls `clear()` — all areas are unmapped, all frames deallocated, the page table root frame is freed by `PageTableImpl::Drop`.

---

## 8. Process Memory Allocation Flow

When a new process is created (`Thread::new_user()` in `rCore/src/process/thread.rs`):

```
1. MemorySet::new()
   → PageTableImpl::new_bare()       // alloc root frame, zero it
   → PageTableImpl::map_kernel()     // write kernel superpage PTEs
   → return MemorySet { areas:[], page_table }

2. For each ELF segment (from load headers):
   if segment.type == PT_LOAD:
       ms.push(seg.vaddr, seg.vaddr + seg.memsz,
           MemoryAttr::default().user() | flags_to_attr(seg.flags),
           File { file: inode, mem_start, file_start, file_end, allocator },
           "elf")

3. Push user stack region:
   ms.push(STACK_BOTTOM, STACK_TOP - 4*PAGE_SIZE,
       MemoryAttr::default().user().execute(),
       Delay::new(GlobalFrameAlloc),
       "user_stack_delay")
   ms.push(STACK_TOP - 4*PAGE_SIZE, STACK_TOP,
       MemoryAttr::default().user().execute(),
       ByFrame::new(GlobalFrameAlloc),
       "user_stack")       // top 4 pages mapped eagerly for init data

4. Write init info (argv, envp, auxv) to user stack:
   ms.with(|| {
       // temporarily activate this page table
       // write strings and pointers to user stack
   })

5. Wrap in Thread:
   Thread {
       tid: allocated_id,
       inner: Mutex::new(ThreadInner { context: Some(UserContext::with(entry, sp)), ... }),
       vm: Arc::new(Mutex::new(ms)),    // ← MemorySet locked here
       proc: Arc::new(Mutex::new(Process { ... })),
   }

6. spawn(thread) → executor::spawn(run loop)
```

During execution, page faults resolve lazily:
```
User code accesses unmapped page
  → CPU traps to kernel (page fault)
  → trap handler reads stval
  → calls memory::handle_page_fault_ext(addr, access)
    → thread = current_thread()
    → thread.vm.lock().handle_page_fault_ext(addr, access)
      → find area containing addr
      → handler.handle_page_fault_ext(&mut pt, addr, access)
        → Delay: alloc frame, zero, set PTE present=true
        → File:   alloc frame, read file data, set PTE present=true
      → return true/false
  → if false: panic/SIGSEGV
  → sret (retry faulting instruction)
```

---

## 9. Higher-Level Mechanisms

### COW (Copy-on-Write)

Not implemented in the reference handlers (the `writable_shared`/`readonly_shared`/`swapped` fields in `Entry` are reserved for this). The actual COW logic lives in `CowExt<T: PageTable>` (`crate/memory/src/cow.rs`):

```
Fork:
  → Creator maps all writable pages with:
      entry.set_writable(false)       // remove write permission
      entry.set_shared(writable=true) // mark RSW[0]
  → Child's MemorySet.clone() copies the same PTE state

Page fault on write to COW page:
  → CowExt::page_fault_handler(addr, alloc_frame)
  → if readonly_shared (RSW[1]):
      // read-only shared page, just set readable
      entry.set_writable(false)
      entry.clear_shared()
  → if writable_shared (RSW[0]):
      // need to copy
      new_frame = alloc_frame()
      copy old_frame → new_frame
      entry.set_target(new_frame)
      entry.set_writable(true)
      entry.clear_shared()
      entry.update()
```

### mmap — Memory Mapping

`sys_mmap` in `rCore/src/syscall/mem.rs`:

```rust
pub fn sys_mmap(&mut self, addr, len, prot, flags, fd, offset) -> SysResult {
    // 1. Determine target address
    if MAP_FIXED → use addr as-is, pop any existing overlapping areas
    else → addr = self.vm().find_free_area(addr, len)

    // 2. Pick handler based on flags
    if ANONYMOUS:
        if SHARED → Shared::new(GlobalFrameAlloc)
        else → Delay::new(GlobalFrameAlloc)
    else:
        file_like = current_proc.get_file_like(fd)?
        → File { file: file_like, mem_start: addr, file_start: offset,
                 file_end: offset + len, allocator: GlobalFrameAlloc }

    // 3. Push region
    self.vm().push(addr, addr + len, prot_to_attr(prot), handler, "mmap")
    Ok(addr)
}
```

### brk — Heap Expansion

Not implemented in the reference rCore (`sys_brk` returns `ENOMEM`). A proper implementation would:

```rust
// Process struct holds: brk: usize
SYS_BRK(new_brk):
  if new_brk == 0 → return current_brk
  if new_brk > current_brk:
      vm.push(current_brk, new_brk,
          MemoryAttr::default().user().writable(),
          Delay::new(GlobalFrameAlloc),
          "heap")
  if new_brk < current_brk:
      vm.pop_with_split(new_brk, current_brk)
  current_brk = new_brk
  return new_brk
```

### MMIO (Memory-Mapped I/O)

Device memory mapped using `Linear` handler with `mmio` flag. The MMIO attribute disables caching and prevents user access. For example, framebuffer mapping:

```rust
ms.push(0xF000_0000, 0xF010_0000,
    MemoryAttr::default().mmio(1),  // mmio hint
    Linear::new(PHYSICAL_MEMORY_OFFSET as isize),
    "framebuffer")
```

The `Linear` handler maps VA → PA = `VA + offset` where `offset = PHYSICAL_MEMORY_OFFSET` gives the identity mapping. The `mmio` flag is available for arch-specific cache control.
