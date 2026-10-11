use abi::{
    DirEntry, Stat, WinSize, ENOENT, FD_STDERR, SYS_CHDIR, SYS_CLOSE, SYS_EXIT, SYS_GETCWD,
    SYS_GETDENTS, SYS_IOCTL, SYS_MKDIR, SYS_OPEN, SYS_READ, SYS_REBOOT, SYS_RENAME, SYS_STAT,
    SYS_UNLINK, SYS_WRITE, TCSETS, TIOCGWINSZ,
};

unsafe fn raw(number: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "int 0x80",
            inlateout("rax") number => ret,
            in("rdi") a1,
            in("rsi") a2,
            in("rdx") a3,
            in("r10") a4,
            // The kernel may use XMM. Linux keeps those registers across a syscall.
            clobber_abi("sysv64"),
            options(nostack),
        );
    }
    ret
}

fn check(ret: i64) -> Result<usize, i64> {
    if ret < 0 { Err(-ret) } else { Ok(ret as usize) }
}

pub fn read(fd: u64, buf: &mut [u8]) -> Result<usize, i64> {
    check(unsafe { raw(SYS_READ, fd, buf.as_mut_ptr() as u64, buf.len() as u64, 0) })
}

pub fn write(fd: u64, buf: &[u8]) -> Result<usize, i64> {
    check(unsafe { raw(SYS_WRITE, fd, buf.as_ptr() as u64, buf.len() as u64, 0) })
}

pub fn write_all(fd: u64, mut buf: &[u8]) -> Result<(), i64> {
    while !buf.is_empty() {
        let n = write(fd, buf)?;
        if n == 0 {
            return Err(5);
        }
        buf = &buf[n..];
    }
    Ok(())
}

pub fn open(path: &str, flags: u64) -> Result<usize, i64> {
    let mut bytes = alloc::vec::Vec::with_capacity(path.len() + 1);
    bytes.extend_from_slice(path.as_bytes());
    bytes.push(0);
    check(unsafe { raw(SYS_OPEN, bytes.as_ptr() as u64, flags, 0, 0) })
}

pub fn close(fd: usize) -> Result<(), i64> {
    check(unsafe { raw(SYS_CLOSE, fd as u64, 0, 0, 0) }).map(|_| ())
}

pub fn stat(path: &str) -> Result<Stat, i64> {
    let mut bytes = alloc::vec::Vec::with_capacity(path.len() + 1);
    bytes.extend_from_slice(path.as_bytes());
    bytes.push(0);
    let mut out = Stat { kind: 0, size: 0 };
    check(unsafe {
        raw(
            SYS_STAT,
            bytes.as_ptr() as u64,
            &mut out as *mut Stat as u64,
            0,
            0,
        )
    })?;
    Ok(out)
}

pub fn mkdir(path: &str) -> Result<(), i64> {
    cstr_call(SYS_MKDIR, path).map(|_| ())
}

pub fn unlink(path: &str) -> Result<(), i64> {
    cstr_call(SYS_UNLINK, path).map(|_| ())
}

pub fn rename(from: &str, to: &str) -> Result<(), i64> {
    let source = cstring(from);
    let dest = cstring(to);
    check(unsafe { raw(SYS_RENAME, source.as_ptr() as u64, dest.as_ptr() as u64, 0, 0) }).map(|_| ())
}

pub fn chdir(path: &str) -> Result<(), i64> {
    cstr_call(SYS_CHDIR, path).map(|_| ())
}

pub fn getcwd() -> Result<alloc::string::String, i64> {
    let mut buf = [0u8; 256];
    let n = check(unsafe { raw(SYS_GETCWD, buf.as_mut_ptr() as u64, buf.len() as u64, 0, 0) })?;
    Ok(alloc::string::String::from_utf8_lossy(&buf[..n]).into_owned())
}

pub fn getdents(fd: usize) -> Result<Option<DirEntry>, i64> {
    let mut entry = DirEntry::empty();
    let n = check(unsafe {
        raw(
            SYS_GETDENTS,
            fd as u64,
            &mut entry as *mut DirEntry as u64,
            core::mem::size_of::<DirEntry>() as u64,
            0,
        )
    })?;
    if n == 0 {
        Ok(None)
    } else {
        Ok(Some(entry))
    }
}

pub fn set_raw(enabled: bool) -> Result<(), i64> {
    let mode = if enabled { 1 } else { 0 };
    check(unsafe { raw(SYS_IOCTL, 0, TCSETS, mode, 0) }).map(|_| ())
}

pub fn winsize() -> Result<WinSize, i64> {
    let mut out = WinSize { rows: 24, cols: 80 };
    check(unsafe { raw(SYS_IOCTL, 1, TIOCGWINSZ, &mut out as *mut WinSize as u64, 0) })?;
    if out.rows == 0 {
        out.rows = 24;
    }
    if out.cols == 0 {
        out.cols = 80;
    }
    Ok(out)
}

pub fn reboot() -> ! {
    unsafe { raw(SYS_REBOOT, 0, 0, 0, 0) };
    loop {}
}

pub fn exit(code: u64) -> ! {
    unsafe { raw(SYS_EXIT, code, 0, 0, 0) };
    loop {}
}

fn cstr_call(number: u64, path: &str) -> Result<usize, i64> {
    let bytes = cstring(path);
    check(unsafe { raw(number, bytes.as_ptr() as u64, 0, 0, 0) })
}

fn cstring(path: &str) -> alloc::vec::Vec<u8> {
    let mut bytes = alloc::vec::Vec::with_capacity(path.len() + 1);
    bytes.extend_from_slice(path.as_bytes());
    bytes.push(0);
    bytes
}

pub fn read_line() -> alloc::string::String {
    let mut out = alloc::string::String::new();
    let mut buf = [0u8; 128];
    loop {
        match read(0, &mut buf) {
            Ok(0) => continue,
            Ok(n) => {
                for &byte in &buf[..n] {
                    out.push(byte as char);
                }
                if out.contains('\n') {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    out
}

pub fn eprint(text: &str) {
    let _ = write_all(FD_STDERR, text.as_bytes());
}

pub fn error(cmd: &str, path: &str, err: i64) {
    if err == ENOENT || err != 0 {
        eprint(cmd);
        eprint(": ");
        if !path.is_empty() {
            eprint(path);
            eprint(": ");
        }
        eprint(abi::errno_name(err));
        eprint("\n");
    }
}
