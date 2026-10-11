//! Syscall numbers and data layouts shared by the vkr-user kernel and its shell.
//!
//! The calling convention matches the Linux x86_64 syscall registers, invoked
//! with `int 0x80` instead of the `syscall` instruction:
//! `rax` is the number, `rdi`/`rsi`/`rdx`/`r10` are the arguments, and `rax`
//! returns a non-negative byte count or `-errno`.

#![no_std]

pub const SYS_READ: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_OPEN: u64 = 2;
pub const SYS_CLOSE: u64 = 3;
pub const SYS_STAT: u64 = 4;
pub const SYS_IOCTL: u64 = 16;
pub const SYS_EXIT: u64 = 60;
pub const SYS_GETCWD: u64 = 79;
pub const SYS_CHDIR: u64 = 80;
pub const SYS_MKDIR: u64 = 83;
pub const SYS_RENAME: u64 = 82;
pub const SYS_UNLINK: u64 = 87;
pub const SYS_REBOOT: u64 = 169;
pub const SYS_GETDENTS: u64 = 217;

pub const O_RDONLY: u64 = 0;
pub const O_WRONLY: u64 = 1;
pub const O_RDWR: u64 = 2;
pub const O_CREAT: u64 = 0x40;
pub const O_TRUNC: u64 = 0x200;
pub const O_APPEND: u64 = 0x400;
pub const O_DIRECTORY: u64 = 0x1_0000;

/// `ioctl` request: set the tty to canonical (`0`) or raw (`1`) mode.
pub const TCSETS: u64 = 0x5402;
/// `ioctl` request: write a [`WinSize`] to the third argument.
pub const TIOCGWINSZ: u64 = 0x5413;
pub const TERM_CANON: u64 = 0;
pub const TERM_RAW: u64 = 1;

pub const S_FILE: u8 = 1;
pub const S_DIR: u8 = 2;

pub const ENOENT: i64 = 2;
pub const EIO: i64 = 5;
pub const EBADF: i64 = 9;
pub const EFAULT: i64 = 14;
pub const EEXIST: i64 = 17;
pub const ENOTDIR: i64 = 20;
pub const EISDIR: i64 = 21;
pub const EINVAL: i64 = 22;
pub const ENOTTY: i64 = 25;
pub const ENOSPC: i64 = 28;
pub const ENAMETOOLONG: i64 = 36;
pub const ENOSYS: i64 = 38;
pub const ENOTEMPTY: i64 = 39;

pub const FD_STDIN: u64 = 0;
pub const FD_STDOUT: u64 = 1;
pub const FD_STDERR: u64 = 2;

/// Top of the user stack (exclusive). The stack grows down.
pub const USER_STACK_TOP: u64 = 0x0070_0000;
pub const USER_STACK_SIZE: u64 = 64 * 1024;
/// Pre-mapped userspace heap used by the shell's allocator.
pub const USER_HEAP_START: u64 = 0x0080_0000;
pub const USER_HEAP_SIZE: u64 = 2 * 1024 * 1024;

/// Single-byte codes the kernel uses for special keys in raw tty mode.
/// Arrow keys arrive from the keyboard and from serial ANSI sequences, and the
/// kernel collapses both into these values so userspace does not reassemble them.
pub const KEY_UP: u8 = 0x80;
pub const KEY_DOWN: u8 = 0x81;
pub const KEY_RIGHT: u8 = 0x82;
pub const KEY_LEFT: u8 = 0x83;
pub const KEY_HOME: u8 = 0x84;
pub const KEY_END: u8 = 0x85;
pub const KEY_DELETE: u8 = 0x86;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DirEntry {
    pub inode: u64,
    pub kind: u64,
    pub name_len: u64,
    pub name: [u8; 64],
}

impl DirEntry {
    pub const fn empty() -> Self {
        Self {
            inode: 0,
            kind: 0,
            name_len: 0,
            name: [0; 64],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Stat {
    pub kind: u64,
    pub size: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct WinSize {
    pub rows: u16,
    pub cols: u16,
}

pub fn errno_name(err: i64) -> &'static str {
    match err {
        ENOENT => "No such file or directory",
        EIO => "Input/output error",
        EBADF => "Bad file descriptor",
        EFAULT => "Bad address",
        EEXIST => "File exists",
        ENOTDIR => "Not a directory",
        EISDIR => "Is a directory",
        EINVAL => "Invalid argument",
        ENOTTY => "Inappropriate ioctl",
        ENOSPC => "No space left",
        ENAMETOOLONG => "File name too long",
        ENOSYS => "Function not implemented",
        ENOTEMPTY => "Directory not empty",
        _ => "Error",
    }
}
