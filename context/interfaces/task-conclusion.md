# task/ — Conclusion

## Overall Role

The `task/` module implements the kernel's execution model: **threads, processes, and process groups**. The split of responsibility is:

- **`Thread`** — the *schedulable primitive*. The async executor schedules `Thread`s, never `Process`es. A thread owns a CPU context and a kernel stack.
- **`Process`** — a *shared-resource container*. It owns the address space, the file-descriptor table, signal state, and the System V IPC tables. It is **not** schedulable; it merely hangs off its threads.
- **Process group** — a pure **id grouping** (`pgid`) over the flat set of processes, used for signal delivery / job control. It allocates nothing and has no struct of its own.

Files: `mod.rs`, `process.rs` (≈ rCore `proc.rs`), `thread.rs`, `futex.rs`, `init.rs` (merged rCore `abi.rs` + `structs.rs`).

---

## 0. Ownership Model (read this first)

Everything in this module is wired together by `Arc` / `Weak` / `Mutex`. Getting the ownership direction right is what prevents both **leaks** (reference cycles) and **use-after-free** (dangling). The graph:

```
   PROCESSES (RwLock<BTreeMap<pid, Arc<Mutex<Process>>>>)   ── strong ──┐
                                                                        ▼
   PROCESSORS[cpu] : Option<Arc<Thread>>  ── strong (while running) ─┐  Process
   executor task (future captures Arc<Thread>) ── strong ───────────┤   │  ▲
                                                                     ▼   │  │ strong
   THREADS (RwLock<BTreeMap<tid, Arc<Thread>>>) ── strong ───────▶ Thread │  (Thread.proc)
                                                                     │   │  │
                                          Thread.vm ─┐               └───┘  │
                                                     ▼   same Arc          │
                                    Arc<Mutex<MemorySet>>  ◀── Process.vm ──┘
                                                     ▲
                                          (cloned, not duplicated)

   Process.threads : Vec<Tid>          ── ids only, NO Arc (breaks the cycle)
   Process.parent  : (Pid, Weak<..>)   ── weak (breaks parent↔child cycle)
   Process.children: Vec<(Pid, Weak)>  ── weak
```

**The rules, stated plainly:**

1. **Root owners are the two global tables.** `THREADS` holds a strong `Arc<Thread>` for every live thread; `PROCESSES` holds a strong `Arc<Mutex<Process>>` for every live process. Removing an entry from a table relinquishes that ownership.

2. **Thread → Process is strong; Process → Thread is *not*.** `Thread.proc: Arc<Mutex<Process>>` keeps the process alive as long as any of its threads live. The reverse direction stores only `Process.threads: Vec<Tid>` — **plain ids, never `Arc<Thread>`** — so the process does not keep its threads alive and **no cycle forms**. A thread's lifetime is governed by `THREADS` + `PROCESSORS` + the executor task, not by its process.

3. **The address space is jointly owned via one shared `Arc`.** A single `Arc<Mutex<MemorySet>>` is stored in **both** `Process.vm` and **each** `Thread.vm` (the same `Arc`, cloned). The `MemorySet` (and therefore the page table and all its frames) is dropped only when the last of these handles is gone. `Thread.vm` is a convenience copy so the scheduler can switch page tables without locking the `Process`.

4. **Parent/child use `Weak`.** `parent` and `children` hold `Weak<Mutex<Process>>`, so a parent does not keep dead children alive and vice-versa. Liveness is tested with `.upgrade()`. The `Pid` is stored *alongside* the `Weak` so identity survives even after the process is gone.

5. **Running transfers ownership of the CPU context.** While a thread runs, `ThreadInner.context` is `None`: `begin_running()` **moves** the `ThreadContext` out onto the run-loop's stack; `end_running()` **moves** it back. The context is never aliased.

6. **fork copies, clone shares.** This is the single most important ownership distinction (see §2): `fork` produces a **new** `MemorySet` (new owner) and a **copied** fd table; `clone` (pthread) hands out **the same `Arc`s** for both `vm` and `proc`.

---

## 1. `mod.rs` — module root & CPU-local current thread

### Items

| Item | Ownership / duty |
|---|---|
| `pub fn init()` | Bootstraps the first user process by calling `shell::add_user_shell()`. Owns nothing; the created thread is owned by the tables + executor. |
| `static mut PROCESSORS: [Option<Arc<Thread>>; MAX_CPU_NUM]` | Per-CPU **strong** handle to the thread currently executing on that CPU. Set on entry to a poll, cleared on exit, so it owns the running thread only for the duration of a poll. |
| `pub fn current_thread() -> Option<Arc<Thread>>` | Clones `PROCESSORS[cpu::id()]` → a new strong handle to "the thread running here". Callers get a co-owner; dropping it is harmless. |

**Consumed:** `arch::cpu::id`, `consts::MAX_CPU_NUM`, `shell::add_user_shell`.

---

## 2. `thread.rs` — the schedulable unit

### `type Tid = usize`
A thread id. Also serves as the process id of the main thread (`pid == leader tid`).

### `struct ThreadContext`
Saved CPU state. **Owns** its register file.
| Field | Ownership |
|---|---|
| `user: Box<UserContext>` | Owns the GP/control registers + PC + SP (boxed, heap-owned). |
| `fp: Box<FpState>` | Owns the floating-point register file. |

### `struct ThreadInner`
The mutable, lock-guarded part of a thread.
| Field | Ownership / duty |
|---|---|
| `context: Option<ThreadContext>` | **Owns** the saved context when parked; `None` while running (ownership moved out). |
| `clear_child_tid: usize` | Address the kernel futex-wakes on thread exit (a user pointer, not owned memory). |
| `sig_mask: Sigset` | Per-thread blocked-signal set (value, owned). |
| `signal_alternate_stack: SignalStack` | Alt-stack descriptor (value, owned). |

### `struct Thread`
The schedulable object. Its identity fields are immutable; mutability is isolated in `inner`.
| Field | Ownership |
|---|---|
| `inner: Mutex<ThreadInner>` | **Owns** the mutable thread state. |
| `vm: Arc<Mutex<MemorySet>>` | **Co-owns** the address space (same `Arc` as `proc.vm`). |
| `proc: Arc<Mutex<Process>>` | **Strong** owner of the process — keeps it alive. |
| `tid: Tid` | Identity (value). |

### `static THREADS: RwLock<BTreeMap<usize, Arc<Thread>>>`
**Root owner** of all threads. Inserting = take ownership; removing (in `Process::exit`) = release it.

### `impl Thread`
| Method | Duty & ownership effect |
|---|---|
| `add_to_table(self) -> Arc<Self>` | Assigns a free `tid` (from `Pid::INIT`), wraps `self` in `Arc`, and **transfers ownership into `THREADS`**, returning a co-owning `Arc`. |
| `new_user_vm(inode, args, envs, &mut MemorySet) -> Result<(entry, sp)>` | Loads the ELF into the *caller-owned* `MemorySet` (via `ElfExt`) and writes the init stack (via `ProcInitInfo`). Borrows; owns nothing lasting. |
| `new_user(inode, exec_path, args, envs) -> Arc<Thread>` | Creates a **new `MemorySet` (new owner)**, a **new `Process`**, and the **main `Thread`** together; registers both in `THREADS`/`PROCESSES`. The returned `Arc<Thread>` is the caller's co-ownership. |
| `fork(&self, &UserContext) -> Arc<Thread>` | **Deep-copies** `vm` (`self.vm.lock().clone()` → new `MemorySet`, new owner), **copies** the fd table (`files.clone()`), links child to parent via `Weak`. New process + new main thread. |
| `new_clone(&self, ctx, stack_top, tls, ctid) -> Arc<Thread>` | **Shares** `vm` and `proc` (`Arc::clone` — same owners). Adds the new tid to the existing `Process.threads`. A new thread in the *same* process. |
| `begin_running(&self) -> ThreadContext` | **Moves** the context out of `inner` (`Option::take`). Ownership passes to the run loop. |
| `end_running(&self, cx)` | **Moves** the context back into `inner`. |
| `has_signal_to_handle(&self) -> bool` | Reads (borrows) the process's `sig_queue` against this thread's mask. |

### Scheduling glue
| Item | Ownership / duty |
|---|---|
| `pub fn spawn(thread: Arc<Thread>)` | Builds the per-thread run-loop `Future` (which **captures the `Arc<Thread>`** — another strong owner) and hands it to `executor::spawn`. When the loop `break`s on exit, the future drops, releasing that ownership. |
| `pub fn yield_now() -> impl Future<Output=()>` | Returns a `YieldFuture` that pends once then re-wakes — cooperative preemption. Owns nothing. |
| `struct YieldFuture` | One-shot yield helper (private). |

**Consumed:** `arch::fp::FpState`, `memory::MemorySet`, `signal::{Sigset, SignalStack}`, `sync::SpinNoIrqLock`, `trapframe::UserContext`, `rcore_fs::vfs::INode`, `executor` (external).

---

## 3. `process.rs` — the shared-resource container

### `struct Pid(pub usize)` / `type Pgid = i32`
`Pid` is a value identity (`Copy`). `new()` returns `Pid(0)` as a placeholder; the real value (== main thread's tid) is assigned by `add_to_process_table`. `is_init()` tests `== INIT (1)`.

### `struct Process`
Owns the resources shared by a thread group.
| Field | Ownership |
|---|---|
| `vm: Arc<Mutex<MemorySet>>` | **Co-owns** the address space (shared with the threads' `vm`). |
| `files: BTreeMap<usize, FileLike>` | **Owns** the fd table; each `FileLike` internally `Arc`-shares its open-file description (so `fork`'s `clone()` copies the table but shares the descriptions). |
| `cwd`, `exec_path: String` | Owned strings. |
| `futexes: BTreeMap<usize, Arc<Futex>>` | **Owns** the process's futexes (strong). |
| `semaphores: SemProc` | **Owns** the SysV semaphore handles (from `ipc`). |
| `pid: Pid`, `pgid: Pgid` | Identity values. |
| `parent: (Pid, Weak<Mutex<Process>>)` | **Weak** link up — no ownership of the parent. |
| `children: Vec<(Pid, Weak<Mutex<Process>>)>` | **Weak** links down — no ownership of children. |
| `threads: Vec<Tid>` | **Ids only** — deliberately *not* `Arc<Thread>`, to avoid a cycle. |
| `eventbus: Arc<Mutex<EventBus>>` | **Owns** (co-owns) the event bus for exit/child notifications. |
| `exit_code: usize` | Value. |
| `sig_queue: VecDeque<(Siginfo, isize)>`, `pending_sigset: Sigset` | Owned pending-signal state. |
| `dispositions: [SignalAction; RTMAX+1]` | Owned signal-handler table. |
| `shm_identifiers: ShmProc` | **Owns** the SysV shared-memory attachments (from `ipc`). |

### `static PROCESSES: RwLock<BTreeMap<usize, Arc<Mutex<Process>>>>`
**Root owner** of all processes.

### Free functions
| Function | Duty |
|---|---|
| `process_of(tid)` | Find the (co-owned) `Arc<Mutex<Process>>` whose `threads` contains `tid`. |
| `process(pid)` | Table lookup by pid. |
| `process_group(pgid)` | Collect all processes with matching `pgid` (for group signal delivery). |
| `add_to_process_table(proc, pid)` | Sets `proc.pid = pid` and **transfers a co-owning `Arc` into `PROCESSES`**. |

### `impl Process`
| Method | Duty / ownership effect |
|---|---|
| `get_free_fd()` / `get_free_fd_from(arg)` | Lowest unused fd (≥ arg). Pure read. |
| `add_file(file_like) -> fd` | **Takes ownership** of a `FileLike` into `files`. |
| `get_futex(uaddr) -> Arc<Futex>` | Lazily creates and **owns** a `Futex`, handing back a co-owning `Arc`. |
| `exit(exit_code)` | **Releases ownership**: drops all `files`, sets exit events on its own and the parent's `eventbus`, then **removes its tids from `THREADS`** and clears `threads`. After this, the only remaining owners are external `Arc`s, which drop naturally → process + `MemorySet` freed. |
| `exited() -> bool` | `threads.is_empty()`. |

**Consumed:** `fs::FileLike`, `ipc::{SemProc, ShmProc}`, `memory::MemorySet`, `signal::{Siginfo, Signal, SignalAction, Sigset}`, `sync::{Event, EventBus, SpinNoIrqLock}`.

---

## 4. `futex.rs` — fast user-space mutex backend

### `struct Waiter`
A single blocked thread. `waker: Option<Waker>` (owned), `woken: bool`, `futex: Arc<Futex>` (**strong back-ref** to the futex it waits on).

### `struct FutexInner` / `struct Futex`
`Futex.inner: Mutex<FutexInner>`; `FutexInner.waiters: VecDeque<Arc<Mutex<Waiter>>>` — the futex **owns** its waiters. (The `Waiter → Futex` strong back-ref forms a temporary cycle, broken when `wake` removes the waiter.)

| Method | Duty |
|---|---|
| `new()` | Empty futex. |
| `wake(n) -> usize` | Pop up to `n` waiters, mark `woken`, fire their wakers; returns count woken (releasing those waiters' ownership). |
| `wait(self: &Arc<Self>, timeout) -> impl Future<SysResult>` | Builds a `FutexFuture` that registers a `Waiter` (taking co-ownership of `self`), optionally arming `NAIVE_TIMER`. |
| `struct FutexFuture` | The await-able; on `Pending` it enqueues itself into `inner.waiters`. |

**Consumed:** `sync::SpinNoIrqLock`, `syscall::{SysError, SysResult}`, `arch::timer::timer_now`, `trap::NAIVE_TIMER`.

---

## 5. `init.rs` — creation-time helpers (ELF + initial stack)

Merged here because both run **only when a process-thread is created**.

### `trait ElfExt` (impl for `xmas_elf::ElfFile`)
| Method | Duty |
|---|---|
| `make_memory_set(&self, ms, inode) -> usize` | Maps every `PT_LOAD` segment into the **caller-owned** `ms` as a `File` handler; returns the program break. Owns nothing lasting. |
| `get_interpreter() -> Result<&str, &str>` | Borrows the `PT_INTERP` path. |
| `append_as_interpreter(&self, inode, ms, bias)` | Maps the dynamic linker into `ms` at `bias`. |
| `get_phdr_vaddr() -> Option<u64>` | PHDR vaddr for `AT_PHDR`/TLS. |

### `struct INodeForMap(pub Arc<dyn INode>)`
**Co-owns** an inode so it can back a file-mapped area; `impl Read` forwards `read_at` to the inode (used during demand paging).

### `struct ProcInitInfo`
Owns `args`, `envs`, `auxv`. `unsafe fn push_at(stack_top) -> sp` writes argc/argv/envp/auxv onto the (already-mapped, process-owned) user stack and returns the new `sp`. It writes into memory owned by the new process's `MemorySet`, not memory it owns itself.

### Auxv constants
`AT_PHDR/PHENT/PHNUM/PAGESZ/BASE/ENTRY`.

**Consumed:** `memory::{MemorySet, Read}`, `rcore_fs::vfs::INode`, `xmas_elf::ElfFile`.

---

## Lifetime Walkthrough (ownership in motion)

**Birth (`fork`)**
1. `Thread::fork` clones `vm` → a brand-new `MemorySet` with a fresh owner `Arc`.
2. A new `Process` is built; it co-owns that `vm`, copies `files`, and links to the parent by `Weak`.
3. `add_to_table` puts `Arc<Thread>` into `THREADS`; `add_to_process_table` puts `Arc<Mutex<Process>>` into `PROCESSES`.
4. `spawn` captures another `Arc<Thread>` into the executor future.
   → The thread now has **three** strong owners (THREADS, executor, briefly PROCESSORS); the process has **two** (PROCESSES + the thread's `proc`).

**Death (`exit`)**
1. `Process::exit` drops `files`, sets exit events, and **removes the tids from `THREADS`** (releases one owner per thread).
2. The run-loop `break`s → the executor future completes and drops its `Arc<Thread>`.
3. `PROCESSORS[cpu]` is cleared after the final poll.
   → Last `Arc<Thread>` gone ⇒ `Thread` dropped ⇒ its `proc` `Arc` and `vm` `Arc` drop.
4. When `PROCESSES` entry is also removed (by `wait4` reaping), the last `Arc<Mutex<Process>>` drops ⇒ `Process` dropped ⇒ last `vm` `Arc` drops ⇒ `MemorySet` dropped ⇒ page table + all frames freed.

No cycles survive because **Process→Thread is by id** and **parent↔child is by `Weak`**.
