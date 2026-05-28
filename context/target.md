# Target

## Phase 1 — Compilation

Fix all compile errors in `kernel/src/kernel.rs` so that `cargo check` passes with zero errors.

Known error categories (from initial compile):

| Category | Count | Description |
|---|---|---|
| Missing symbol | 1 | `BOOT_EPOCH` not found |
| Ambiguous type | 1 | `{integer}` needs explicit annotation |
| Type mismatch | 6 | `u64`/`usize`, `i32`/`usize`, `u64`/`u32`, `bool`/`usize` |
| Match arm type | 1 | `Option<&u64>` vs `u64` |
| Missing field/member | 3 | `disk` field on Kernel, `allocated` in BuddyAllocator |
| Missing method | 2 | `FHandle::open`, `sort_by` on `MutexGuard<VecDeque>` |
| Wrong argument | 4 | `tasks.find()` gets `Arc<Task>` instead of `usize` |
| Borrow checker | 7 | simultaneous mutable/immutable borrows, moved values, missing `&mut self` |

**Total: ~24 errors.**

## Phase 2 — Tests

All three test suites must pass:

- `cargo test --test basic` — 8 groups (group\_01 through group\_08)
- `cargo test --test advanced`
- `cargo test --test pressure`

## Phase 3 — Rewrite

After all tests pass, rewrite `kernel/src/kernel.rs` for clarity:

- Rename cryptic variables/functions to descriptive names.
- Extract inlined logic into well-named helpers.
- Add meaningful comments.
- Simplify overly complex expressions.
- The rewritten code must still pass all tests.
