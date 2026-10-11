//! In-memory filesystem. Nothing here survives a reboot.

use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use abi::{
    EBADF, EEXIST, EINVAL, EISDIR, ENAMETOOLONG, ENOENT, ENOSPC, ENOTDIR, ENOTEMPTY, O_APPEND,
    O_CREAT, O_DIRECTORY, O_TRUNC, O_WRONLY, S_DIR, S_FILE,
};

const NAME_MAX: usize = 64;
const FD_MAX: usize = 32;

struct Node {
    name: String,
    parent: Option<usize>,
    kind: Kind,
    live: bool,
}

enum Kind {
    Dir { children: Vec<usize> },
    File { data: Vec<u8> },
}

struct OpenFile {
    node: usize,
    offset: usize,
    dir_pos: usize,
    readable: bool,
    writable: bool,
}

pub struct Vfs {
    nodes: Vec<Node>,
    cwd: usize,
    files: [Option<OpenFile>; FD_MAX],
}

impl Vfs {
    fn new() -> Self {
        let mut vfs = Self {
            nodes: Vec::new(),
            cwd: 0,
            files: core::array::from_fn(|_| None),
        };
        vfs.nodes.push(Node {
            name: String::new(),
            parent: None,
            kind: Kind::Dir {
                children: Vec::new(),
            },
            live: true,
        });
        vfs
    }

    pub fn seed(&mut self) {
        let _ = self.mkdir_at(0, "home");
        let home = self.child(0, "home").unwrap();
        let _ = self.mkdir_at(home, "user");
        let _ = self.mkdir_at(0, "tmp");
        let _ = self.create_file(
            "/README",
            b"Welcome to vkr-user.\n\n\
The shell is a ring-3 program. The filesystem is in memory and is\n\
cleared when the machine reboots.\n\n\
Commands: help, ls, cd, pwd, mkdir, touch, cp, mv, cat, rm, echo, vim,\n\
clear, uname, poweroff.\n",
        );
        let _ = self.create_file(
            "/home/user/welcome.txt",
            b"You are in /home/user.\nEdit me with: vim welcome.txt\n",
        );
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), i64> {
        let (parent, name) = self.parent_and_name(path)?;
        if self.child(parent, name).is_some() {
            return Err(EEXIST);
        }
        self.mkdir_at(parent, name).map(|_| ())
    }

    pub fn unlink(&mut self, path: &str) -> Result<(), i64> {
        let id = self.resolve(path)?;
        if id == 0 {
            return Err(EINVAL);
        }
        match &self.nodes[id].kind {
            Kind::Dir { children } if !children.is_empty() => return Err(ENOTEMPTY),
            _ => {}
        }
        self.detach(id)
    }

    /// Move or rename `from` onto `to`. An existing file is replaced. An existing
    /// directory is replaced only when it is empty and `from` is also a directory.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), i64> {
        let src = self.resolve(from)?;
        if src == 0 {
            return Err(EINVAL);
        }
        let (parent, name) = self.parent_and_name(to)?;
        self.check_name(name)?;
        if self.is_dir(src) && self.contains(src, parent) {
            return Err(EINVAL);
        }
        if let Some(existing) = self.child(parent, name) {
            if existing == src {
                return Ok(());
            }
            match (self.is_dir(src), self.is_dir(existing)) {
                (true, true) => {
                    if let Kind::Dir { children } = &self.nodes[existing].kind {
                        if !children.is_empty() {
                            return Err(ENOTEMPTY);
                        }
                    }
                    self.detach(existing)?;
                }
                (false, false) => self.detach(existing)?,
                (true, false) => return Err(ENOTDIR),
                (false, true) => return Err(EISDIR),
            }
        }
        let old_parent = self.nodes[src].parent.ok_or(EINVAL)?;
        if old_parent != parent {
            if let Kind::Dir { children } = &mut self.nodes[old_parent].kind {
                children.retain(|child| *child != src);
            }
            if let Kind::Dir { children } = &mut self.nodes[parent].kind {
                children.push(src);
            }
            self.nodes[src].parent = Some(parent);
        }
        self.nodes[src].name = String::from(name);
        Ok(())
    }

    pub fn chdir(&mut self, path: &str) -> Result<(), i64> {
        let id = self.resolve(path)?;
        if !self.is_dir(id) {
            return Err(ENOTDIR);
        }
        self.cwd = id;
        Ok(())
    }

    pub fn getcwd(&self) -> String {
        let mut names = Vec::new();
        let mut cur = self.cwd;
        while let Some(parent) = self.nodes[cur].parent {
            names.push(self.nodes[cur].name.clone());
            cur = parent;
        }
        if names.is_empty() {
            return String::from("/");
        }
        names.reverse();
        let mut out = String::from("/");
        for (i, name) in names.iter().enumerate() {
            if i > 0 {
                out.push('/');
            }
            out.push_str(name);
        }
        out
    }

    pub fn stat(&self, path: &str) -> Result<(u64, u64), i64> {
        let id = self.resolve(path)?;
        Ok(self.stat_node(id))
    }

    pub fn open(&mut self, path: &str, flags: u64) -> Result<usize, i64> {
        let creat = flags & O_CREAT != 0;
        let trunc = flags & O_TRUNC != 0;
        let append = flags & O_APPEND != 0;
        let directory = flags & O_DIRECTORY != 0;
        let writable = flags & 3 == O_WRONLY || flags & 3 == 2;
        let readable = flags & 3 != O_WRONLY;

        let id = match self.resolve(path) {
            Ok(id) => id,
            Err(err) if err == ENOENT && creat => self.create_empty(path)?,
            Err(err) => return Err(err),
        };

        match &mut self.nodes[id].kind {
            Kind::Dir { .. } => {
                if writable || trunc {
                    return Err(EISDIR);
                }
            }
            Kind::File { data } => {
                if directory {
                    return Err(ENOTDIR);
                }
                if trunc && writable {
                    data.clear();
                }
            }
        }

        let offset = if append && self.is_file(id) {
            match &self.nodes[id].kind {
                Kind::File { data } => data.len(),
                Kind::Dir { .. } => 0,
            }
        } else {
            0
        };

        // Slots 0, 1 and 2 are the tty.
        let slot = (3..FD_MAX)
            .find(|fd| self.files[*fd].is_none())
            .ok_or(ENOSPC)?;
        self.files[slot] = Some(OpenFile {
            node: id,
            offset,
            dir_pos: 0,
            readable,
            writable,
        });
        Ok(slot)
    }

    pub fn close(&mut self, fd: usize) -> Result<(), i64> {
        if fd < 3 {
            return Ok(());
        }
        if fd >= FD_MAX || self.files[fd].take().is_none() {
            return Err(EBADF);
        }
        Ok(())
    }

    pub fn read(&mut self, fd: usize, buf: &mut [u8]) -> Result<usize, i64> {
        let file = self.file_mut(fd)?;
        if !file.readable {
            return Err(EBADF);
        }
        let node = file.node;
        let offset = file.offset;
        match &self.nodes[node].kind {
            Kind::Dir { .. } => Err(EISDIR),
            Kind::File { data } => {
                if offset >= data.len() {
                    return Ok(0);
                }
                let n = buf.len().min(data.len() - offset);
                buf[..n].copy_from_slice(&data[offset..offset + n]);
                self.files[fd].as_mut().unwrap().offset += n;
                Ok(n)
            }
        }
    }

    pub fn write(&mut self, fd: usize, buf: &[u8]) -> Result<usize, i64> {
        let file = self.file_mut(fd)?;
        if !file.writable {
            return Err(EBADF);
        }
        let node = file.node;
        let offset = file.offset;
        match &mut self.nodes[node].kind {
            Kind::Dir { .. } => Err(EISDIR),
            Kind::File { data } => {
                let end = offset + buf.len();
                if data.len() < end {
                    data.resize(end, 0);
                }
                data[offset..end].copy_from_slice(buf);
                self.files[fd].as_mut().unwrap().offset = end;
                Ok(buf.len())
            }
        }
    }

    pub fn getdents(&mut self, fd: usize, name: &mut [u8; NAME_MAX]) -> Result<Option<(u64, u64, usize)>, i64> {
        let file = self.file(fd)?;
        let node = file.node;
        let pos = file.dir_pos;
        let children = match &self.nodes[node].kind {
            Kind::Dir { children } => children.clone(),
            Kind::File { .. } => return Err(ENOTDIR),
        };
        if pos >= children.len() {
            return Ok(None);
        }
        let child = children[pos];
        let child_name = self.nodes[child].name.as_bytes();
        let n = child_name.len().min(NAME_MAX);
        name[..n].copy_from_slice(&child_name[..n]);
        let (kind, size) = self.stat_node(child);
        self.files[fd].as_mut().unwrap().dir_pos += 1;
        Ok(Some((kind, size, n)))
    }

    fn stat_node(&self, id: usize) -> (u64, u64) {
        match &self.nodes[id].kind {
            Kind::Dir { .. } => (u64::from(S_DIR), 0),
            Kind::File { data } => (u64::from(S_FILE), data.len() as u64),
        }
    }

    fn create_empty(&mut self, path: &str) -> Result<usize, i64> {
        let (parent, name) = self.parent_and_name(path)?;
        if let Some(id) = self.child(parent, name) {
            return Ok(id);
        }
        self.add_file(parent, name)
    }

    fn create_file(&mut self, path: &str, bytes: &[u8]) -> Result<(), i64> {
        let id = self.create_empty(path)?;
        if let Kind::File { data } = &mut self.nodes[id].kind {
            data.extend_from_slice(bytes);
            Ok(())
        } else {
            Err(EISDIR)
        }
    }

    fn mkdir_at(&mut self, parent: usize, name: &str) -> Result<usize, i64> {
        self.check_name(name)?;
        if !self.is_dir(parent) {
            return Err(ENOTDIR);
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            name: String::from(name),
            parent: Some(parent),
            kind: Kind::Dir {
                children: Vec::new(),
            },
            live: true,
        });
        if let Kind::Dir { children } = &mut self.nodes[parent].kind {
            children.push(id);
        }
        Ok(id)
    }

    fn add_file(&mut self, parent: usize, name: &str) -> Result<usize, i64> {
        self.check_name(name)?;
        if !self.is_dir(parent) {
            return Err(ENOTDIR);
        }
        let id = self.nodes.len();
        self.nodes.push(Node {
            name: String::from(name),
            parent: Some(parent),
            kind: Kind::File { data: Vec::new() },
            live: true,
        });
        if let Kind::Dir { children } = &mut self.nodes[parent].kind {
            children.push(id);
        }
        Ok(id)
    }

    fn parent_and_name<'a>(&self, path: &'a str) -> Result<(usize, &'a str), i64> {
        let path = path.trim_end_matches('/');
        if path.is_empty() {
            return Err(EEXIST);
        }
        let (parent_path, name) = match path.rfind('/') {
            Some(0) => ("/", &path[1..]),
            Some(index) => (&path[..index], &path[index + 1..]),
            None => (".", path),
        };
        if name.is_empty() || name == "." || name == ".." {
            return Err(EINVAL);
        }
        let parent = self.resolve(parent_path)?;
        if !self.is_dir(parent) {
            return Err(ENOTDIR);
        }
        Ok((parent, name))
    }

    fn resolve(&self, path: &str) -> Result<usize, i64> {
        if path.is_empty() {
            return Err(ENOENT);
        }
        let mut cur = if path.starts_with('/') { 0 } else { self.cwd };
        for part in path.split('/') {
            if part.is_empty() || part == "." {
                continue;
            }
            if part == ".." {
                cur = self.nodes[cur].parent.unwrap_or(cur);
                continue;
            }
            if !self.is_dir(cur) {
                return Err(ENOTDIR);
            }
            cur = self.child(cur, part).ok_or(ENOENT)?;
        }
        if !self.nodes[cur].live {
            return Err(ENOENT);
        }
        Ok(cur)
    }

    fn child(&self, parent: usize, name: &str) -> Option<usize> {
        let Kind::Dir { children } = &self.nodes[parent].kind else {
            return None;
        };
        children.iter().copied().find(|id| {
            self.nodes[*id].live && self.nodes[*id].name == name
        })
    }

    fn is_dir(&self, id: usize) -> bool {
        matches!(self.nodes[id].kind, Kind::Dir { .. })
    }

    fn is_file(&self, id: usize) -> bool {
        matches!(self.nodes[id].kind, Kind::File { .. })
    }

    fn contains(&self, ancestor: usize, node: usize) -> bool {
        let mut cur = Some(node);
        while let Some(id) = cur {
            if id == ancestor {
                return true;
            }
            cur = self.nodes[id].parent;
        }
        false
    }

    fn detach(&mut self, id: usize) -> Result<(), i64> {
        if id == 0 {
            return Err(EINVAL);
        }
        let parent = self.nodes[id].parent.ok_or(EINVAL)?;
        if let Kind::Dir { children } = &mut self.nodes[parent].kind {
            children.retain(|child| *child != id);
        }
        self.nodes[id].live = false;
        if self.cwd == id {
            self.cwd = parent;
        }
        Ok(())
    }

    fn check_name(&self, name: &str) -> Result<(), i64> {
        if name.is_empty() || name.len() > NAME_MAX || name.contains('/') {
            Err(ENAMETOOLONG)
        } else {
            Ok(())
        }
    }

    fn file(&self, fd: usize) -> Result<&OpenFile, i64> {
        self.files.get(fd).and_then(|slot| slot.as_ref()).ok_or(EBADF)
    }

    fn file_mut(&mut self, fd: usize) -> Result<&mut OpenFile, i64> {
        self.files.get_mut(fd).and_then(|slot| slot.as_mut()).ok_or(EBADF)
    }
}

static VFS: Mutex<Option<Vfs>> = Mutex::new(None);

pub fn init() {
    let mut vfs = Vfs::new();
    vfs.seed();
    *VFS.lock() = Some(vfs);
}

fn with<T>(f: impl FnOnce(&mut Vfs) -> T) -> T {
    let mut guard = VFS.lock();
    f(guard.as_mut().expect("vfs"))
}

pub fn mkdir(path: &str) -> Result<(), i64> {
    with(|vfs| vfs.mkdir(path))
}
pub fn unlink(path: &str) -> Result<(), i64> {
    with(|vfs| vfs.unlink(path))
}
pub fn rename(from: &str, to: &str) -> Result<(), i64> {
    with(|vfs| vfs.rename(from, to))
}
pub fn chdir(path: &str) -> Result<(), i64> {
    with(|vfs| vfs.chdir(path))
}
pub fn getcwd() -> String {
    with(|vfs| vfs.getcwd())
}
pub fn stat(path: &str) -> Result<(u64, u64), i64> {
    with(|vfs| vfs.stat(path))
}
pub fn open(path: &str, flags: u64) -> Result<usize, i64> {
    with(|vfs| vfs.open(path, flags))
}
pub fn close(fd: usize) -> Result<(), i64> {
    with(|vfs| vfs.close(fd))
}
pub fn read(fd: usize, buf: &mut [u8]) -> Result<usize, i64> {
    with(|vfs| vfs.read(fd, buf))
}
pub fn write(fd: usize, buf: &[u8]) -> Result<usize, i64> {
    with(|vfs| vfs.write(fd, buf))
}
pub fn getdents(fd: usize) -> Result<Option<(u64, [u8; NAME_MAX], usize)>, i64> {
    with(|vfs| {
        let mut name = [0u8; NAME_MAX];
        match vfs.getdents(fd, &mut name)? {
            Some((kind, _size, len)) => Ok(Some((kind, name, len))),
            None => Ok(None),
        }
    })
}
