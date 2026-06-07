# fs/ Module

## Overall Role

The `fs/` module provides the **file descriptor abstraction** to processes. Its job is to make everything accessible through the POSIX fd interface (`read`/`write`/`poll`/`ioctl`/`mmap`) by wrapping heterogeneous kernel objects (disk files, pipes, terminals, framebuffers, random generators, epoll instances, network sockets) into a single `FileLike` enum that lives in `Process.files: BTreeMap<usize, FileLike>`.

It does NOT manage page cache, block I/O scheduling, or on-disk structures — those are delegated to the `rcore_fs` external crates.

---

## Five Sub-Parts

### 1. `FileHandle` + `FileLike` — The fd Table Abstraction

**Concept**: A process holds file descriptors (integers) mapping to kernel objects. `FileLike` is the closed union of all possible fd types. `FileHandle` wraps an `Arc<dyn INode>` with an offset cursor and open flags.

**Files**: `file.rs`, `file_like.rs`

**Exports**:
```rust
pub struct FileHandle {
    inode: Arc<dyn INode>,           // the actual thing being read/written
    description: Arc<RwLock<OpenFileDescription>>,  // offset + options (shared on fork)
    pub path: String,                // debug label
    pub pipe: bool,                  // true → lseek returns ESPIPE
    pub fd_cloexec: bool,            // true → close on exec
}

pub struct OpenOptions { pub read: bool, pub write: bool, pub append: bool, pub nonblock: bool }
pub enum SeekFrom { Start(u64), End(i64), Current(i64) }

impl FileHandle {
    pub fn new(inode, options, path, pipe, cloexec) -> Self;
    pub fn dup(cloexec) -> Self;
    pub fn read(buf) -> Result<usize>;       // uses internal offset, advances it
    pub fn read_at(offset, buf) -> Result<usize>;  // explicit offset, no advance
    pub fn write(buf) -> Result<usize>;
    pub fn write_at(offset, buf) -> Result<usize>;
    pub fn seek(pos: SeekFrom) -> Result<u64>;
    pub fn set_len(len) -> Result<()>;       // ftruncate
    pub fn sync_all() / sync_data() -> Result<()>;
    pub fn metadata() -> Result<Metadata>;   // fstat
    pub fn lookup_follow(path, max_follow) -> Result<Arc<dyn INode>>;  // openat resolution
    pub fn read_entry() -> Result<String>;   // getdents
    pub fn read_entry_with_metadata() -> Result<(Metadata, String)>;
    pub fn poll() -> Result<PollStatus>;
    pub fn async_poll() -> impl Future<Output = Result<PollStatus>>;
    pub fn io_control(cmd: u32, data: usize) -> Result<usize>;
    pub fn mmap(area: MMapArea) -> Result<()>;
    pub fn inode() -> Arc<dyn INode>;
}

pub enum FileLike {
    File(FileHandle),                     // everything backed by an INode
    Socket(Box<dyn Socket>),              // network socket
    EpollInstance(EpollInstance),          // epoll fd
}

impl FileLike {
    pub fn dup(cloexec) -> Self;
    pub fn read(buf) -> SysResult;         // File/Socket only, Epoll → ENOSYS
    pub fn write(buf) -> SysResult;
    pub fn ioctl(request, arg1, arg2, arg3) -> SysResult;
    pub fn mmap(area) -> SysResult;        // File only
    pub fn poll() -> Result<PollStatus, SysError>;
    pub fn async_poll() -> Result<PollStatus, SysError>;
}
```

**External interfaces consumed**:
- `rcore_fs::vfs::INode` — `FileHandle` delegates every operation to it
- `rcore_fs::vfs::{FsError, Result, Metadata, PollStatus, MMapArea}` — return types and error propagation
- `crate::net::Socket` — trait for the `Socket` variant in `FileLike`
- `spin::RwLock` — sharing `OpenFileDescription` across fork

---

### 2. INode Implementations — Concrete "File-Like" Objects

**Concept**: Every kernel object that can be read/written/polled implements the `INode` trait. These are the concrete types that `FileHandle` wraps.

**Files**: `pipe.rs`, `pseudo.rs`, `devfs/tty.rs`, `devfs/serial.rs`, `devfs/fbdev.rs`, `devfs/random.rs`, `devfs/shm.rs`

**The INode trait (from external crate)**:
```rust
pub trait INode: Any + Sync + Send {
    // REQUIRED (no default):
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize>;
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize>;
    fn poll(&self) -> Result<PollStatus>;
    fn as_any_ref(&self) -> &dyn Any;

    // OPTIONAL (have defaults returning Err(NotSupported)):
    fn metadata(&self) -> Result<Metadata>;
    fn resize(&self, len: usize) -> Result<()>;
    fn create(&self, name: &str, type_: FileType, mode: u32) -> Result<Arc<dyn INode>>;
    fn find(&self, name: &str) -> Result<Arc<dyn INode>>;
    fn link(&self, name: &str, other: &Arc<dyn INode>) -> Result<()>;
    fn unlink(&self, name: &str) -> Result<()>;
    fn move_(&self, old_name: &str, target: &Arc<dyn INode>, new_name: &str) -> Result<()>;
    fn get_entry(&self, id: usize) -> Result<String>;
    fn io_control(&self, cmd: u32, data: usize) -> Result<usize>;
    fn mmap(&self, area: MMapArea) -> Result<()>;
    fn fs(&self) -> Arc<dyn FileSystem>;     // panic default
    fn sync_all(&self) -> Result<()>;
    fn sync_data(&self) -> Result<()>;
}
```

**Implementations and their override pattern**:

| INode | read_at | write_at | poll | metadata | io_control | mmap | as_any_ref | Other |
|-------|---------|----------|------|----------|------------|------|------------|-------|
| **Pipe** | pop from VecDeque | push to VecDeque | can_read/can_write | — | — | — | self | — |
| **Pseudo** | copy from static Vec | NotSupported | read:true | type_, size | — | — | self | — |
| **TtyINode** | pop from buffer | print! | can_read/true | CharDevice, 0o666 | TCGETS/TCSETS/TIOCGPGRP/WINSZ | — | self | push(c) for serial input |
| **Serial** | driver.try_read() | driver.write() | true/true | CharDevice | — | — | self | — |
| **Fbdev** | read from fb memory | write to fb memory | read:true | CharDevice | FBIOGET_* | Linear handler | self | — |
| **RandomINode** | LCG PRNG | NotSupported | read:true | CharDevice | — | — | self | — |
| **ShmINode** | NotSupported | NotSupported | — | Dir | — | — | self | stub for /dev/shm mount |

**External interfaces consumed**:
- `rcore_fs::vfs::INode` — the trait each device implements
- `rcore_fs::vfs::{FsError, Result, Metadata, PollStatus, Timespec, FileType, MMapArea, make_rdev}` — supporting types
- `rcore_fs::dev::Device` — only used by `Fbdev` indirectly (framebuffer read/write)
- `crate::drivers::SerialDriver` — `Serial` wraps this
- `crate::drivers::SERIAL_ACTIVITY` — serial async wakeup
- `crate::drivers::gpu::fb::FRAME_BUFFER` — `Fbdev` reads from this
- `crate::sync::EventBus` — `Pipe` and `TtyINode` use it for async poll wakeup
- `crate::process::process_group` — `TtyINode` delivers SIGINT on Ctrl-C
- `crate::signal::{send_signal, Signal::SIGINT}` — signal delivery from TTY

---

### 3. Protocol Constants — fcntl and ioctl

**Concept**: Linux ABI constants.

**Files**: `fcntl.rs`, `ioctl.rs`

**Exports**:
```rust
// fcntl.rs
pub const F_DUPFD: usize = 0;   pub const F_GETFD: usize = 1;
pub const F_SETFD: usize = 2;   pub const F_GETFL: usize = 3;
pub const F_SETFL: usize = 4;   pub const FD_CLOEXEC: usize = 1;
pub const F_DUPFD_CLOEXEC: usize = 1030;
pub const O_NONBLOCK: usize = 0o4000;   pub const O_APPEND: usize = 0o2000;
pub const O_CLOEXEC: usize = 0o2000000;
pub const AT_SYMLINK_NOFOLLOW: usize = 0x100;

// ioctl.rs
pub const TCGETS: usize = 0x5401;   pub const TCSETS: usize = 0x5402;
pub const TIOCGPGRP: usize = 0x540F;  pub const TIOCSPGRP: usize = 0x5410;
pub const TIOCGWINSZ: usize = 0x5413;
pub const FIONCLEX: usize = 0x5450;   pub const FIOCLEX: usize = 0x5451;
pub const FIONBIO: usize = 0x5421;

pub struct Termios { /* iflag, oflag, cflag, lflag, line, cc[32], ispeed, ospeed */ }
pub struct Winsize { row: u16, ws_col: u16, xpixel: u16, ypixel: u16 }
bitflags! { pub struct LocalModes: u32 { const ISIG, ICANON, ECHO, ... } }
```

**Consumed by**: `syscall/fs.rs` (fcntl, ioctl dispatch), `devfs/tty.rs` (TCGETS/TCSETS/TIOCGPGRP handling)

---

### 4. `MemBuf` — RAM-backed Device for Embedded SFS

**Concept**: Wraps a static memory region (the SFS image embedded in the kernel binary via `incbin!`) as a `Device` so `SimpleFileSystem::open()` can read from it without a real block driver.

**File**: `device.rs`

**Exports**:
```rust
pub struct MemBuf(RwLock<&'static mut [u8]>);
impl MemBuf {
    pub unsafe fn new(begin: extern "C" fn(), end: extern "C" fn()) -> Self;
}
impl Device for MemBuf {
    fn read_at(&self, offset, buf) -> Result<usize>;   // copy from static slice
    fn write_at(&self, offset, buf) -> Result<usize>;  // copy to static slice
    fn sync(&self) -> Result<()>;
}
```

**External interfaces consumed**:
- `rcore_fs::dev::Device` — the trait `MemBuf` implements
- `spin::RwLock` — interior mutability for the `&'static mut [u8]`

**Used by**: Only in `fs/mod.rs` under `#[cfg(feature = "link_user")]`, passed to `SimpleFileSystem::open()`.

---

### 5. EpollInstance — I/O Event Notification Container

**Concept**: A per-fd event interest set + ready list. It provides the data structure for epoll but the actual notification mechanism lives in `syscall/fs.rs` (condvar-based polling).

**File**: `epoll.rs`

**Exports**:
```rust
pub struct EpollInstance {
    pub events: BTreeMap<usize, EpollEvent>,    // registered fds and their interest events
    pub ready_list: SpinNoIrqLock<BTreeSet<usize>>,  // fds with pending events
    pub new_ctl_list: SpinNoIrqLock<BTreeSet<usize>>,  // fds just registered (assumed ready once)
}

pub struct EpollEvent {
    pub events: u32,     // EPOLLIN, EPOLLOUT, EPOLLERR, EPOLLET, etc.
    pub data: EpollData, // user cookie
}

impl EpollInstance {
    pub fn new(flags: usize) -> Self;
    pub fn control(&mut self, op: usize, fd: usize, event: &EpollEvent) -> SysResult;
    // op = EPOLL_CTL_ADD → events.insert(fd, event)
    // op = EPOLL_CTL_MOD → events.remove(fd); events.insert(fd, event)
    // op = EPOLL_CTL_DEL → events.remove(fd)
}

// Methods on Process (defined in epoll.rs via inherent impl):
impl Process {
    pub fn get_epoll_instance(&self, fd: usize) -> Result<&EpollInstance, SysError>;
    pub fn get_epoll_instance_mut(&mut self, fd: usize) -> Result<&mut EpollInstance, SysError>;
}
```

**External interfaces consumed**:
- `crate::sync::SpinNoIrqLock` — for ready_list and new_ctl_list
- `crate::process::Process` — inherent impl methods attached here

**Important**: `EpollInstance` has NO dependency on `INode`, `Device`, or any I/O trait. It is purely a data structure. The event loop lives in `syscall/fs.rs:358-528` and uses `Condvar::wait_events(&[TICK_ACTIVITY, SOCKET_ACTIVITY], ...)` to poll.

---
