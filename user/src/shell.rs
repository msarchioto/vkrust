use alloc::string::String;
use alloc::vec::Vec;

use abi::{
    EEXIST, EINVAL, EISDIR, ENOENT, ENOTDIR, O_APPEND, O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY, S_DIR,
    S_FILE,
};

use crate::syscall;
use crate::vim;

pub fn run() -> ! {
    syscall::eprint("vkr-user 0.1.0  —  type `help`\n");
    loop {
        prompt();
        let line = syscall::read_line();
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse(line) {
            Ok((argv, redirect)) => execute(argv, redirect),
            Err(message) => {
                syscall::eprint(message);
                syscall::eprint("\n");
            }
        }
    }
}

fn prompt() {
    let cwd = syscall::getcwd().unwrap_or_else(|_| String::from("/"));
    syscall::eprint("vkr-user:");
    syscall::eprint(&cwd);
    syscall::eprint("$ ");
}

struct Redirect {
    path: String,
    append: bool,
}

fn parse(line: &str) -> Result<(Vec<String>, Option<Redirect>), &'static str> {
    let tokens = tokenize(line)?;
    if tokens.is_empty() {
        return Ok((tokens, None));
    }
    if let Some(pos) = tokens.iter().position(|token| token == ">" || token == ">>") {
        if pos + 2 != tokens.len() || pos == 0 {
            return Err("syntax error near redirection");
        }
        let append = tokens[pos] == ">>";
        let path = tokens[pos + 1].clone();
        Ok((
            tokens[..pos].to_vec(),
            Some(Redirect { path, append }),
        ))
    } else {
        Ok((tokens, None))
    }
}

fn tokenize(line: &str) -> Result<Vec<String>, &'static str> {
    let bytes = line.as_bytes();
    let mut index = 0;
    let mut tokens = Vec::new();
    while index < bytes.len() {
        while index < bytes.len() && bytes[index] == b' ' {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        if bytes[index] == b'"' {
            index += 1;
            let start = index;
            while index < bytes.len() && bytes[index] != b'"' {
                index += 1;
            }
            if index >= bytes.len() {
                return Err("unclosed quote");
            }
            tokens.push(String::from_utf8_lossy(&bytes[start..index]).into_owned());
            index += 1;
        } else {
            let start = index;
            while index < bytes.len() && bytes[index] != b' ' {
                index += 1;
            }
            tokens.push(String::from_utf8_lossy(&bytes[start..index]).into_owned());
        }
    }
    Ok(tokens)
}

fn execute(argv: Vec<String>, redirect: Option<Redirect>) {
    if argv.is_empty() {
        return;
    }
    if argv[0] == "vim" {
        let Some(path) = argv.get(1) else {
            syscall::eprint("vim: missing file operand\n");
            return;
        };
        if let Err(err) = vim::edit(path) {
            syscall::error("vim", path, err);
        }
        return;
    }

    let fd = if let Some(redirect) = redirect {
        let mut flags = O_WRONLY | O_CREAT;
        flags |= if redirect.append { O_APPEND } else { O_TRUNC };
        match syscall::open(&redirect.path, flags) {
            Ok(fd) => fd as u64,
            Err(err) => {
                syscall::error("sh", &redirect.path, err);
                return;
            }
        }
    } else {
        1
    };

    let result = dispatch(&argv, fd);
    if fd > 2 {
        let _ = syscall::close(fd as usize);
    }
    if let Err(err) = result {
        let path = argv.get(1).map(String::as_str).unwrap_or("");
        syscall::error(&argv[0], path, err);
    }
}

fn dispatch(argv: &[String], fd: u64) -> Result<(), i64> {
    match argv[0].as_str() {
        "help" => out(fd, HELP),
        "ls" => ls(argv.get(1).map(String::as_str).unwrap_or("."), fd),
        "cd" => cd(argv.get(1).map(String::as_str)),
        "pwd" => {
            let cwd = syscall::getcwd()?;
            out(fd, &cwd)?;
            out(fd, "\n")
        }
        "mkdir" => mkdir(argv),
        "cp" => cp(argv),
        "mv" => mv(argv),
        "touch" => {
            let path = need(argv, 1, "touch")?;
            let fd = syscall::open(path, O_WRONLY | O_CREAT)?;
            syscall::close(fd)
        }
        "cat" => {
            if argv.len() < 2 {
                return missing("cat");
            }
            for path in &argv[1..] {
                cat(path, fd)?;
            }
            Ok(())
        }
        "rm" => {
            let path = need(argv, 1, "rm")?;
            syscall::unlink(path)
        }
        "echo" => {
            for (index, arg) in argv[1..].iter().enumerate() {
                if index > 0 {
                    out(fd, " ")?;
                }
                out(fd, arg)?;
            }
            out(fd, "\n")
        }
        "clear" => out(fd, "\x1b[2J\x1b[H"),
        "uname" => out(fd, "vkr-user 0.1.0 x86_64\n"),
        "poweroff" => syscall::reboot(),
        other => {
            syscall::eprint(other);
            syscall::eprint(": command not found\n");
            Ok(())
        }
    }
}

fn need<'a>(argv: &'a [String], index: usize, cmd: &str) -> Result<&'a str, i64> {
    argv.get(index).map(String::as_str).ok_or_else(|| {
        syscall::eprint(cmd);
        syscall::eprint(": missing operand\n");
        0
    }).and_then(|path| if path.is_empty() { Err(22) } else { Ok(path) })
}

fn missing(cmd: &str) -> Result<(), i64> {
    syscall::eprint(cmd);
    syscall::eprint(": missing operand\n");
    Ok(())
}

fn out(fd: u64, text: &str) -> Result<(), i64> {
    syscall::write_all(fd, text.as_bytes())
}

fn mkdir(argv: &[String]) -> Result<(), i64> {
    let mut parents = false;
    let mut paths = Vec::new();
    for arg in &argv[1..] {
        if arg == "-p" {
            parents = true;
        } else if arg.starts_with('-') {
            syscall::eprint("mkdir: invalid option\n");
            return Err(0);
        } else {
            paths.push(arg.as_str());
        }
    }
    if paths.is_empty() {
        return missing("mkdir");
    }
    for path in paths {
        let result = if parents {
            mkdir_p(path)
        } else {
            syscall::mkdir(path)
        };
        if let Err(err) = result {
            syscall::error("mkdir", path, err);
            return Err(0);
        }
    }
    Ok(())
}

fn mkdir_p(path: &str) -> Result<(), i64> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "/" {
        return Ok(());
    }
    let mut acc = String::new();
    if trimmed.starts_with('/') {
        acc.push('/');
    }
    for part in trimmed.split('/').filter(|part| !part.is_empty()) {
        if part == "." {
            continue;
        }
        if !acc.is_empty() && !acc.ends_with('/') {
            acc.push('/');
        }
        acc.push_str(part);
        if part == ".." {
            continue;
        }
        match syscall::stat(&acc) {
            Ok(info) if info.kind == u64::from(S_DIR) => {}
            Ok(_) => return Err(ENOTDIR),
            Err(err) if err == ENOENT => syscall::mkdir(&acc)?,
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

fn cp(argv: &[String]) -> Result<(), i64> {
    let mut recursive = false;
    let mut ops = Vec::new();
    for arg in &argv[1..] {
        if arg == "-r" || arg == "-R" {
            recursive = true;
        } else if arg.starts_with('-') {
            syscall::eprint("cp: invalid option\n");
            return Err(0);
        } else {
            ops.push(arg.as_str());
        }
    }
    if ops.len() < 2 {
        return missing("cp");
    }
    let dest = ops[ops.len() - 1];
    let sources = &ops[..ops.len() - 1];
    let dest_dir = match syscall::stat(dest) {
        Ok(info) if info.kind == u64::from(S_DIR) => true,
        Ok(_) => false,
        Err(err) if err == ENOENT => false,
        Err(err) => {
            syscall::error("cp", dest, err);
            return Err(0);
        }
    };
    if sources.len() > 1 && !dest_dir {
        syscall::error("cp", dest, ENOTDIR);
        return Err(0);
    }
    for src in sources {
        let target = if dest_dir {
            join_dir(dest, basename(src))
        } else {
            String::from(dest)
        };
        if let Err(err) = copy_entry(src, &target, recursive) {
            syscall::error("cp", src, err);
            return Err(0);
        }
    }
    Ok(())
}

fn mv(argv: &[String]) -> Result<(), i64> {
    if argv.len() < 3 {
        return missing("mv");
    }
    for arg in &argv[1..] {
        if arg.starts_with('-') {
            syscall::eprint("mv: invalid option\n");
            return Err(0);
        }
    }
    let dest = argv[argv.len() - 1].as_str();
    let sources = &argv[1..argv.len() - 1];
    let dest_dir = match syscall::stat(dest) {
        Ok(info) if info.kind == u64::from(S_DIR) => true,
        Ok(_) => false,
        Err(err) if err == ENOENT => false,
        Err(err) => {
            syscall::error("mv", dest, err);
            return Err(0);
        }
    };
    if sources.len() > 1 && !dest_dir {
        syscall::error("mv", dest, ENOTDIR);
        return Err(0);
    }
    for src in sources {
        let target = if dest_dir {
            join_dir(dest, basename(src))
        } else {
            String::from(dest)
        };
        if let Err(err) = syscall::rename(src, &target) {
            syscall::error("mv", src, err);
            return Err(0);
        }
    }
    Ok(())
}

fn copy_entry(src: &str, dest: &str, recursive: bool) -> Result<(), i64> {
    let src_abs = absolute(src)?;
    let dest_abs = absolute(dest)?;
    copy_tree(&src_abs, &dest_abs, recursive)
}

fn copy_tree(src: &str, dest: &str, recursive: bool) -> Result<(), i64> {
    if path_within(dest, src) {
        return Err(EINVAL);
    }
    let info = syscall::stat(src)?;
    if info.kind == u64::from(S_DIR) {
        if !recursive {
            return Err(EISDIR);
        }
        if let Err(err) = syscall::mkdir(dest) {
            if err != EEXIST {
                return Err(err);
            }
            let existing = syscall::stat(dest)?;
            if existing.kind != u64::from(S_DIR) {
                return Err(ENOTDIR);
            }
        }
        let dir = syscall::open(src, O_RDONLY)?;
        let mut names = Vec::new();
        while let Some(entry) = syscall::getdents(dir)? {
            let n = (entry.name_len as usize).min(entry.name.len());
            let name = String::from_utf8_lossy(&entry.name[..n]).into_owned();
            if name.is_empty() || name == "." || name == ".." {
                continue;
            }
            names.push(name);
        }
        syscall::close(dir)?;
        for name in names {
            copy_tree(
                &alloc::format!("{src}/{name}"),
                &join_dir(dest, &name),
                true,
            )?;
        }
        Ok(())
    } else if info.kind == u64::from(S_FILE) {
        let input = syscall::open(src, O_RDONLY)?;
        let output = match syscall::open(dest, O_WRONLY | O_CREAT | O_TRUNC) {
            Ok(fd) => fd,
            Err(err) => {
                let _ = syscall::close(input);
                return Err(err);
            }
        };
        let mut buf = [0u8; 256];
        loop {
            let n = match syscall::read(input as u64, &mut buf) {
                Ok(n) => n,
                Err(err) => {
                    let _ = syscall::close(input);
                    let _ = syscall::close(output);
                    return Err(err);
                }
            };
            if n == 0 {
                break;
            }
            if let Err(err) = syscall::write_all(output as u64, &buf[..n]) {
                let _ = syscall::close(input);
                let _ = syscall::close(output);
                return Err(err);
            }
        }
        syscall::close(input)?;
        syscall::close(output)
    } else {
        Err(EINVAL)
    }
}

fn absolute(path: &str) -> Result<String, i64> {
    let combined = if path.starts_with('/') {
        String::from(path)
    } else {
        let cwd = syscall::getcwd()?;
        if cwd == "/" {
            alloc::format!("/{path}")
        } else {
            alloc::format!("{cwd}/{path}")
        }
    };
    Ok(normalize(&combined))
}

fn normalize(path: &str) -> String {
    let mut parts = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            parts.pop();
            continue;
        }
        parts.push(part);
    }
    if parts.is_empty() {
        return String::from("/");
    }
    let mut out = String::new();
    for part in parts {
        out.push('/');
        out.push_str(part);
    }
    out
}

fn path_within(path: &str, root: &str) -> bool {
    if path == root {
        return true;
    }
    if root == "/" {
        return path.starts_with('/');
    }
    path.starts_with(root) && path.as_bytes().get(root.len()) == Some(&b'/')
}

fn basename(path: &str) -> &str {
    let path = path.trim_end_matches('/');
    match path.rfind('/') {
        Some(index) => &path[index + 1..],
        None => path,
    }
}

fn join_dir(dir: &str, name: &str) -> String {
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() {
        alloc::format!("/{name}")
    } else {
        alloc::format!("{dir}/{name}")
    }
}

fn cd(path: Option<&str>) -> Result<(), i64> {
    syscall::chdir(path.unwrap_or("/home/user"))
}

fn ls(path: &str, fd: u64) -> Result<(), i64> {
    let info = syscall::stat(path)?;
    if info.kind == u64::from(S_FILE) {
        out(fd, path)?;
        return out(fd, "\n");
    }
    if info.kind != u64::from(S_DIR) {
        return Err(20);
    }
    let dir = syscall::open(path, O_RDONLY)?;
    let mut names = Vec::new();
    while let Some(entry) = syscall::getdents(dir)? {
        let n = entry.name_len as usize;
        let name = String::from_utf8_lossy(&entry.name[..n.min(entry.name.len())]).into_owned();
        let suffix = if entry.kind == u64::from(S_DIR) { "/" } else { "" };
        names.push((name, suffix));
    }
    let _ = syscall::close(dir);
    names.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, suffix) in names {
        out(fd, &name)?;
        out(fd, suffix)?;
        out(fd, "\n")?;
    }
    Ok(())
}

fn cat(path: &str, out_fd: u64) -> Result<(), i64> {
    let fd = syscall::open(path, O_RDONLY)?;
    let mut buf = [0u8; 256];
    loop {
        let n = syscall::read(fd as u64, &mut buf)?;
        if n == 0 {
            break;
        }
        syscall::write_all(out_fd, &buf[..n])?;
    }
    syscall::close(fd)
}

const HELP: &str = "\
vkr-user shell

  ls [path]          list a directory
  cd [path]          change directory (home is /home/user)
  pwd                print the working directory
  mkdir [-p] path    create a directory (-p makes parents)
  touch path         create an empty file
  cp [-r] src dest   copy a file, or a directory with -r
  mv src dest        rename, or move into a directory
  cat path...        print files
  rm path            remove a file or an empty directory
  echo text          print text
  echo text > file   write text to a file
  vim path           edit a file
  clear              clear the screen
  uname              print the system name
  poweroff           leave QEMU

vim: arrows or hjkl move, i inserts, Esc returns to normal mode,
d deletes a line, :w writes, :q quits, :wq writes and quits.
";
