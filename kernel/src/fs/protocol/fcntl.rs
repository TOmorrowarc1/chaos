pub const F_DUPFD: usize = 0;
pub const F_GETFD: usize = 1;
pub const F_SETFD: usize = 2;
pub const F_GETFL: usize = 3;
pub const F_SETFL: usize = 4;
pub const F_GETLK: usize = 5;
pub const F_SETLK: usize = 6;
pub const F_SETLKW: usize = 7;

const F_LINUX_SPECIFIC_BASE: usize = 1024;

pub const FD_CLOEXEC: usize = 1;
pub const F_DUPFD_CLOEXEC: usize = F_LINUX_SPECIFIC_BASE + 6;

pub const O_NONBLOCK: usize = 0o4000;
pub const O_APPEND: usize = 0o2000;
pub const O_CLOEXEC: usize = 0o2000000;

pub const AT_SYMLINK_NOFOLLOW: usize = 0x100;
