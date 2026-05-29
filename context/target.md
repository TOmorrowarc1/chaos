# Recovery Target

Restore the six missing subsystems in `kernel/src/` so the bare-metal `no_std` kernel compiles and runs. Reference the clean implementation in `rCore/src/` for design, but write original code.

## Phases (ordered by dependency)

### Phase 1 — sync/
Create `kernel/src/sync/` with:
- `SpinLock` — test-and-set spinlock
- `SpinNoIrqLock` — spinlock that also disables local interrupts
- `Condvar` — condition variable built on SpinLock + thread park/unpark
- `Semaphore` — counting semaphore
- `EventBus` — event flag + callback list

### Phase 2 — memory.rs
Create `kernel/src/memory.rs` with:
- FrameAllocator wrapping `bitmap_allocator::BitAlloc1M`
- `alloc_frame()`, `dealloc_frame()`, `alloc_frame_contiguous()`
- `KernelStack` — per-thread kernel stack
- `handle_page_fault()`, `handle_page_fault_ext()`
- `init_heap()` — wire up `HEAP_ALLOCATOR`
- `access_ok()`, `copy_from_user()`, `copy_to_user()`

### Phase 3 — trap.rs
Create `kernel/src/trap.rs` with:
- `TICK`, `TICK_ALL_PROCESSORS` — atomic tick counters
- `do_tick()`, `wall_tick()`, `cpu_tick()`, `uptime_msec()`
- `timer()` — called on timer interrupt
- `NAIVE_TIMER` — lazy-static timer wheel
- `serial(c)` — route serial input to TTY

### Phase 4 — process/
Create `kernel/src/process/` with:
- `structs.rs` — `Process`, `Thread`, `ProcessInner`, `ThreadInner` structs
- `proc.rs` — process creation, `fork()`/`exec()`/`exit()`/`wait4()`
- `thread.rs` — thread lifecycle, `spawn()`, `current_thread()`
- `abi.rs` — ELF loading, user stack setup (argv/envp/auxv)
- `futex.rs` — futex syscall backend

### Phase 5 — fs/
Create `kernel/src/fs/` with:
- `mod.rs` — `ROOT_INODE` (SFS + DevFS + RamFS mounts)
- `file.rs` — `File` handle over INode
- `file_like.rs` — `FileLike` enum for dispatch
- `pipe.rs` — pipe implementation
- `epoll.rs` — epoll instance
- `fcntl.rs` — fcntl helpers
- `ioctl.rs` — ioctl dispatch
- `pseudo.rs` — pseudo-files (stdin/stdout/stderr)
- `device.rs` — `MemBuf` device
- `devfs/mod.rs`, `devfs/tty.rs`, `devfs/serial.rs`, `devfs/shm.rs`, `devfs/fbdev.rs`, `devfs/random.rs`

### Phase 6 — ipc/
Create `kernel/src/ipc/` with:
- `semary.rs` — System V semaphore arrays
- `shared_mem.rs` — System V shared memory

## Verification

After each phase, the kernel crate should compile:
```
cargo build -Z build-std=core,alloc --target targets/riscv64.json --features "board_qemu nographic"
```
