# Rules

## General

- All changes go into `kernel/src/kernel.rs`. The file `chaos-tests/src/lib.rs` is a symlink to it.
- Never modify test files under `chaos-tests/tests/` unless explicitly asked.
- Never format the entire file — only edit specific targeted sections.
- Keep changes minimal and surgical. Fix one bug at a time.
- After each fix, verify the relevant test group compiles and passes before moving on.
- **Do not modify any file unless the user explicitly tells you to.** Wait for instructions.
- **Every time you modify a file, write an entry into `context/record.md`** with the current timestamp and a concise, precise description of what was changed and why.

## Compilation

- The project uses `cargo test --test <name>` to run tests (`basic`, `advanced`, `pressure`).
- Use `cargo check` to verify compilation without running.
- Use `cargo test --test basic -- group_01` to run a single test group.

## Borrowing & Mutability

- Prefer `&mut self` when a method needs to mutate state.
- Avoid cloning when a reference suffices.
- When using `.retain()` with a closure that reads the struct, extract the field first to avoid simultaneous mutable and immutable borrows.

## Conventions

- Use `as` or `.into()` for numeric type conversions, not `try_into().unwrap()` unless overflow is a real concern.
- Prefer `saturating_*` / `wrapping_*` arithmetic over raw `+` / `-` when overflow is possible.
- Name conversions explicitly: `x as u64`, `val.into()`.

## Commit Style

- One commit per logical bug fix.
- Commit message format: `fix(subsystem): brief description`.
- No `--no-verify`, no force-push.
