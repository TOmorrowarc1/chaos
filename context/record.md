# Record

## 2026-05-28

### Initial compile

Ran `cargo build` in `chaos-tests/`. Result: **24 errors**.

Key observations:
- `chaos-tests/src/lib.rs` is a symlink to `kernel/src/kernel.rs` — all fixes go in the kernel source.
- The kernel is a monolithic simulation with subsystems: scheduler, VMM, IPC, signals, file handles, memory allocator.
- Many errors are simple type mismatches (`usize` vs `u64`, `i32` vs `usize`) — suggests the code was ported from a different pointer width or architecture.
- Borrow-checker errors in event-dispatch closures are a recurring pattern.

### Documents created

- `context/rules.md` — agent rules of engagement.
- `context/target.md` — phased goal breakdown.
- `context/record.md` — this file, for tracking progress and decisions.

### 11:21 — Changed `FdState.off` from `u64` to `usize`

`off` is exclusively used as a byte index into `Vec<u8>` data buffers, which requires `usize`. The `u64` type was causing noisy casts everywhere and a missing-cast bug in `splice_to`. Changed:

- `FdState.off: u64` → `usize`
- `FSeek::Start(u64)` → `Start(usize)`, `End(i64)` → `End(isize)`, `Cur(i64)` → `Cur(isize)`
- `FHandle::seek()` return `Result<u64, …>` → `Result<usize, …>`
- Removed all `as usize` / `as u64` casts on the cursor in `read`, `write`, `splice_to`, `FLike::File` variants

Result: 2 of 24 compile errors resolved (down to 22).

### 11:38 — Annotated `order` in `defragment_frame_pool` as `usize`

`let mut order = 0` had no type constraint, and `saturating_sub` exists on all integer types, causing `E0689` (ambiguous numeric type). Annotated `order: usize` to fix.

Result: 3 of 24 compile errors resolved (down to 21).

### 11:42 — Changed `depth` in `IoQueue::submit_batch` from `i32` to inferred `usize`

`let depth: i32 = q.len()` forced a type mismatch — `q.len()` returns `usize`, the downstream comparison `depth > IOQUEUE_DEPTH as i32` was a workaround. Changed to `let depth = q.len();` and `depth > IOQUEUE_DEPTH` directly.

Result: 4 of 24 compile errors resolved (down to 20).

### 11:45 — Changed `result` in `SigSet::coalesce_pending` from `u32` to `u64`

`let mut result: u32 = 0` was a correctness bug — `NSIG = 64` requires 64 bits to represent all signals; `u32` would silently truncate signals 32–63. Changed to `u64` to match the return type and the `pending`/`blocked` bitmask width.

Result: 5 of 24 compile errors resolved (down to 19).

### 11:53 — Fixed `Context::reg_class` fallback arm

`self.r.get(idx)` returned `Option<&u64>` incompatible with the other `u64` arms. Since the bounds check at line 3711 already guarantees `idx < N_REGS`, replaced with `v` (the already-extracted register value).

Result: 6 of 24 compile errors resolved (down to 18).

### 11:57 — Changed `exceeds_any` to boolean chain

`exceeds_any` returned `violations: usize` but declared `-> bool`. Since the function is a predicate ("does any limit exceed?"), replaced the accumulator with a `||` chain: `fds > max_fds || threads > max_threads || stack > max_stack_size`.

Result: 7 of 24 compile errors resolved (down to 17).

### 14:58 — Defined `BOOT_EPOCH` as `const usize = 0`

`BOOT_EPOCH` was used in `SYS_CLOCK_GETTIME` (`clk_id == 1`, CLOCK_MONOTONIC) but never defined. Neither rCore nor the test suite define or reference it. Set to `0` since `CLK` starts at 0 at boot and `ticks + 0 = ticks` gives correct monotonic ticks-since-boot. Added a `TODO` comment noting it needs proper boot-time capture semantics.

Result: 8 of 24 compile errors resolved (down to 16).

### 15:08 — Added `disk: Disk` field to `Kernel` struct

`Kernel` was missing the `disk: Disk` field referenced by `SYS_WRITE` (`fd <= 2` tracking) and `SYS_CLOSE` (cache eviction tracking). Added the field and initialized it in `new()` as `Disk::new("main")`. The underlying read/write/close logic remains messy (block cache manipulation in syscall handlers) — deferred for future refactoring.

Result: 10 of 24 compile errors resolved (down to 14).

### 15:13 — Added `FHandle::open` factory method + fixed `Arc<FHandle>` wrapper

Added `FHandle::open(path, opt) -> Self` as a convenience constructor wrapping `new` with `pipe: false, cloexec: false`, matching POSIX `open` semantics. Also fixed `FLike::File(Arc::new(fh))` at the call site — `FLike::File` takes a plain `FHandle`, not `Arc<FHandle>`.

Result: 11 of 24 compile errors resolved (down to 13).

### 15:23 — Fixed `WaitQueue::reorder_by_priority` sort

`VecDeque` doesn't have `sort_by` — it's a ring buffer, not a contiguous slice. Changed `q.sort_by(...)` to `q.make_contiguous().sort_by(...)`, which rearranges the buffer in-place then sorts the resulting `&mut [T]`.

Result: 12 of 24 compile errors resolved (down to 12).
