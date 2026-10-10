//! `int 0x80` entry. Arguments are `rdi`, `rsi`, `rdx`, `r10`.

use alloc::string::String;
use alloc::vec;

use abi::{
    DirEntry, Stat, WinSize, EBADF, EFAULT, EINVAL, ENAMETOOLONG, ENOENT, ENOSYS, ENOTTY,
    FD_STDERR, FD_STDOUT, SYS_CHDIR, SYS_CLOSE, SYS_EXIT, SYS_GETCWD, SYS_GETDENTS, SYS_IOCTL,
    SYS_MKDIR, SYS_OPEN, SYS_READ, SYS_REBOOT, SYS_STAT, SYS_UNLINK, SYS_WRITE, TCSETS, TERM_RAW,
    TIOCGWINSZ,
};

use crate::{console, exit_qemu, input, vfs};

const PATH_MAX: usize = 256;
const COPY_MAX: usize = 1024 * 1024;

struct UserMem {
    regions: [(u64, u64); 8],
    count: usize,
}

static USER_MEM: spin::Mutex<UserMem> = spin::Mutex::new(UserMem {
    regions: [(0, 0); 8],
    count: 0,
});

pub fn set_user_regions(regions: &[(u64, u64)]) {
    let mut mem = USER_MEM.lock();
    mem.count = 0;
    for &(start, end) in regions.iter().take(mem.regions.len()) {
        let index = mem.count;
        mem.regions[index] = (start, end);
        mem.count += 1;
    }
}

fn user_ok(addr: u64, len: usize) -> bool {
    if len == 0 {
        return true;
    }
    let Some(end) = addr.checked_add(len as u64) else {
        return false;
    };
    if end > 0x0000_8000_0000_0000 {
        return false;
    }
    let mem = USER_MEM.lock();
    mem.regions[..mem.count]
        .iter()
        .any(|&(start, stop)| addr >= start && end <= stop)
}

fn copy_from_user(addr: u64, len: usize) -> Result<alloc::vec::Vec<u8>, i64> {
    if len > COPY_MAX || !user_ok(addr, len) {
        return Err(EFAULT);
    }
    let mut buf = vec![0u8; len];
    unsafe {
        core::ptr::copy_nonoverlapping(addr as *const u8, buf.as_mut_ptr(), len);
    }
    Ok(buf)
}

fn copy_to_user(addr: u64, bytes: &[u8]) -> Result<(), i64> {
    if !user_ok(addr, bytes.len()) {
        return Err(EFAULT);
    }
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), addr as *mut u8, bytes.len());
    }
    Ok(())
}

fn user_path(addr: u64) -> Result<String, i64> {
    let mut out = String::new();
    for index in 0..PATH_MAX {
        let at = addr.checked_add(index as u64).ok_or(EFAULT)?;
        if !user_ok(at, 1) {
            return Err(EFAULT);
        }
        let byte = unsafe { (at as *const u8).read() };
        if byte == 0 {
            return Ok(out);
        }
        if !byte.is_ascii() {
            return Err(EINVAL);
        }
        out.push(byte as char);
    }
    Err(ENAMETOOLONG)
}

pub fn dispatch(number: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> i64 {
    let result = dispatch_inner(number, a1, a2, a3, a4);
    match result {
        Ok(value) => value as i64,
        Err(err) => -err,
    }
}

fn dispatch_inner(number: u64, a1: u64, a2: u64, a3: u64, _a4: u64) -> Result<usize, i64> {
    match number {
        SYS_READ => sys_read(a1, a2, a3),
        SYS_WRITE => sys_write(a1, a2, a3),
        SYS_OPEN => {
            let path = user_path(a1)?;
            vfs::open(&path, a2)
        }
        SYS_CLOSE => vfs::close(a1 as usize).map(|()| 0),
        SYS_STAT => sys_stat(a1, a2),
        SYS_IOCTL => sys_ioctl(a1, a2, a3),
        SYS_GETCWD => sys_getcwd(a1, a2),
        SYS_CHDIR => {
            let path = user_path(a1)?;
            vfs::chdir(&path).map(|()| 0)
        }
        SYS_MKDIR => {
            let path = user_path(a1)?;
            vfs::mkdir(&path).map(|()| 0)
        }
        SYS_UNLINK => {
            let path = user_path(a1)?;
            vfs::unlink(&path).map(|()| 0)
        }
        SYS_GETDENTS => sys_getdents(a1, a2, a3),
        SYS_EXIT => {
            crate::serial::serial_println!("user exit {a1}");
            exit_qemu(if a1 == 0 { 0x10 } else { 0x11 });
        }
        SYS_REBOOT => {
            crate::serial::serial_println!("poweroff");
            exit_qemu(0x10);
        }
        _ => Err(ENOSYS),
    }
}

fn sys_read(fd: u64, ptr: u64, len: u64) -> Result<usize, i64> {
    let len = (len as usize).min(COPY_MAX);
    if !user_ok(ptr, len) {
        return Err(EFAULT);
    }
    let mut tmp = vec![0u8; len];
    let n = if fd == 0 {
        input::read(&mut tmp)
    } else {
        vfs::read(fd as usize, &mut tmp)?
    };
    copy_to_user(ptr, &tmp[..n])?;
    Ok(n)
}

fn sys_write(fd: u64, ptr: u64, len: u64) -> Result<usize, i64> {
    let len = (len as usize).min(COPY_MAX);
    let bytes = copy_from_user(ptr, len)?;
    if fd == FD_STDOUT || fd == FD_STDERR {
        console::write_bytes(&bytes);
        return Ok(bytes.len());
    }
    vfs::write(fd as usize, &bytes)
}

fn sys_stat(path: u64, out: u64) -> Result<usize, i64> {
    let path = user_path(path)?;
    let (kind, size) = vfs::stat(&path)?;
    let stat = Stat { kind, size };
    let bytes = unsafe {
        core::slice::from_raw_parts(&stat as *const Stat as *const u8, core::mem::size_of::<Stat>())
    };
    copy_to_user(out, bytes)?;
    Ok(0)
}

fn sys_getcwd(buf: u64, len: u64) -> Result<usize, i64> {
    let cwd = vfs::getcwd();
    let bytes = cwd.as_bytes();
    if (len as usize) < bytes.len() + 1 {
        return Err(ENAMETOOLONG);
    }
    copy_to_user(buf, bytes)?;
    copy_to_user(buf + bytes.len() as u64, &[0])?;
    Ok(bytes.len())
}

fn sys_getdents(fd: u64, ptr: u64, len: u64) -> Result<usize, i64> {
    if (len as usize) < core::mem::size_of::<DirEntry>() {
        return Err(EINVAL);
    }
    match vfs::getdents(fd as usize)? {
        None => Ok(0),
        Some((kind, name, name_len)) => {
            let mut entry = DirEntry::empty();
            entry.inode = 1;
            entry.kind = kind;
            entry.name_len = name_len as u64;
            entry.name = name;
            let bytes = unsafe {
                core::slice::from_raw_parts(
                    &entry as *const DirEntry as *const u8,
                    core::mem::size_of::<DirEntry>(),
                )
            };
            copy_to_user(ptr, bytes)?;
            Ok(bytes.len())
        }
    }
}

fn sys_ioctl(fd: u64, request: u64, arg: u64) -> Result<usize, i64> {
    if fd != 0 && fd != 1 {
        return Err(ENOTTY);
    }
    match request {
        TCSETS => {
            input::set_raw(arg == TERM_RAW);
            Ok(0)
        }
        TIOCGWINSZ => {
            let (rows, cols) = console::window_size();
            let winsz = WinSize { rows, cols };
            let bytes = unsafe {
                core::slice::from_raw_parts(
                    &winsz as *const WinSize as *const u8,
                    core::mem::size_of::<WinSize>(),
                )
            };
            copy_to_user(arg, bytes)?;
            Ok(0)
        }
        _ => Err(ENOTTY),
    }
}

#[allow(dead_code)]
fn missing(path: &str) -> i64 {
    if path.is_empty() { ENOENT } else { ENOENT }
}

#[allow(dead_code)]
fn bad_fd() -> i64 {
    EBADF
}
