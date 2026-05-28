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
