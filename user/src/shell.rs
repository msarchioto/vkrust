use alloc::string::String;
use alloc::vec::Vec;

use abi::{
    O_APPEND, O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY, S_DIR, S_FILE,
};

use crate::syscall;
use crate::vim;

pub fn run() -> ! {
    syscall::eprint("fern 0.1.0  —  type `help`\n");
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
    syscall::eprint("fern:");
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
        "mkdir" => {
            let path = need(argv, 1, "mkdir")?;
            syscall::mkdir(path)
        }
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
        "uname" => out(fd, "fern 0.1.0 x86_64\n"),
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
fern shell

  ls [path]          list a directory
  cd [path]          change directory (home is /home/user)
  pwd                print the working directory
  mkdir path         create a directory
  touch path         create an empty file
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
