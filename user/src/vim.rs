//! A small modal editor. It is not Vim; it covers the motions needed to edit a file.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use abi::{KEY_DELETE, KEY_DOWN, KEY_END, KEY_HOME, KEY_LEFT, KEY_RIGHT, KEY_UP, O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY, S_DIR};

use crate::syscall;

enum Mode {
    Normal,
    Insert,
    Command(String),
}

enum Key {
    Char(u8),
    Enter,
    Backspace,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Delete,
}

/// What the editor should do after handling one key.
enum Step {
    Stay,
    /// Leave command mode and return to normal mode.
    Normal,
    /// Leave the editor.
    Quit,
}

pub fn edit(path: &str) -> Result<(), i64> {
    if let Ok(stat) = syscall::stat(path) {
        if stat.kind == u64::from(S_DIR) {
            return Err(21);
        }
    }
    let mut lines = load(path)?;
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    let mut cx = 0usize;
    let mut cy = 0usize;
    let mut top = 0usize;
    let mut mode = Mode::Normal;
    let mut dirty = false;
    let mut message = String::new();
    let size = syscall::winsize().unwrap_or(abi::WinSize { rows: 24, cols: 80 });
    let rows = size.rows.max(2) as usize;
    let cols = size.cols.max(2) as usize;
    syscall::set_raw(true)?;

    loop {
        fit(&mut cx, &mut cy, &lines);
        if cy < top {
            top = cy;
        }
        let view = rows - 1;
        if cy >= top + view {
            top = cy + 1 - view;
        }
        draw(path, &lines, cx, cy, top, rows, cols, &mode, dirty, &message);
        message.clear();
        let mut step = Step::Stay;
        match read_key() {
            Key::Escape => mode = Mode::Normal,
            key => match &mut mode {
                Mode::Normal => {
                    if normal(&key, path, &mut lines, &mut cx, &mut cy, &mut mode, &mut dirty, &mut message)? {
                        step = Step::Quit;
                    }
                }
                Mode::Insert => insert_at(key, &mut lines, &mut cx, &mut cy, &mut mode, &mut dirty),
                Mode::Command(buf) => {
                    step = command_key(key, buf, path, &mut lines, &mut dirty, &mut message)?;
                }
            },
        }
        match step {
            Step::Stay => {}
            Step::Normal => mode = Mode::Normal,
            Step::Quit => break,
        }
    }

    let _ = syscall::set_raw(false);
    let _ = syscall::write_all(1, b"\x1b[2J\x1b[H");
    Ok(())
}

fn normal(
    key: &Key,
    path: &str,
    lines: &mut Vec<Vec<u8>>,
    cx: &mut usize,
    cy: &mut usize,
    mode: &mut Mode,
    dirty: &mut bool,
    message: &mut String,
) -> Result<bool, i64> {
    match key {
        Key::Char(b'h') | Key::Left => *cx = cx.saturating_sub(1),
        Key::Char(b'j') | Key::Down => {
            if *cy + 1 < lines.len() {
                *cy += 1;
            }
        }
        Key::Char(b'k') | Key::Up => *cy = cy.saturating_sub(1),
        Key::Char(b'l') | Key::Right => *cx += 1,
        Key::Char(b'0') | Key::Home => *cx = 0,
        Key::Char(b'$') | Key::End => *cx = lines[*cy].len(),
        Key::Char(b'i') => *mode = Mode::Insert,
        Key::Char(b'a') => {
            *cx = (*cx + 1).min(lines[*cy].len());
            *mode = Mode::Insert;
        }
        Key::Char(b'A') => {
            *cx = lines[*cy].len();
            *mode = Mode::Insert;
        }
        Key::Char(b'o') => {
            lines.insert(*cy + 1, Vec::new());
            *cy += 1;
            *cx = 0;
            *dirty = true;
            *mode = Mode::Insert;
        }
        Key::Char(b'x') => {
            if *cx < lines[*cy].len() {
                lines[*cy].remove(*cx);
                *dirty = true;
            }
        }
        Key::Delete => {
            if *cx < lines[*cy].len() {
                lines[*cy].remove(*cx);
                *dirty = true;
            }
        }
        Key::Char(b'd') => {
            if lines.len() == 1 {
                lines[0].clear();
            } else {
                lines.remove(*cy);
                if *cy >= lines.len() {
                    *cy = lines.len() - 1;
                }
            }
            *cx = 0;
            *dirty = true;
            message.push_str("deleted line");
        }
        Key::Char(b':') => *mode = Mode::Command(String::new()),
        Key::Char(b'w') => {
            save(path, lines)?;
            *dirty = false;
            message.push_str("written");
        }
        _ => {}
    }
    let _ = path;
    Ok(false)
}

fn insert_at(
    key: Key,
    lines: &mut Vec<Vec<u8>>,
    cx: &mut usize,
    cy: &mut usize,
    mode: &mut Mode,
    dirty: &mut bool,
) {
    match key {
        Key::Escape => *mode = Mode::Normal,
        Key::Left => *cx = cx.saturating_sub(1),
        Key::Right => *cx = (*cx + 1).min(lines[*cy].len()),
        Key::Up => *cy = cy.saturating_sub(1),
        Key::Down => {
            if *cy + 1 < lines.len() {
                *cy += 1;
            }
        }
        Key::Home => *cx = 0,
        Key::End => *cx = lines[*cy].len(),
        Key::Backspace => {
            if *cx > 0 {
                *cx -= 1;
                lines[*cy].remove(*cx);
                *dirty = true;
            } else if *cy > 0 {
                let rest = lines.remove(*cy);
                *cy -= 1;
                *cx = lines[*cy].len();
                lines[*cy].extend_from_slice(&rest);
                *dirty = true;
            }
        }
        Key::Delete => {
            if *cx < lines[*cy].len() {
                lines[*cy].remove(*cx);
                *dirty = true;
            }
        }
        Key::Enter => {
            let rest = lines[*cy].split_off(*cx);
            lines.insert(*cy + 1, rest);
            *cy += 1;
            *cx = 0;
            *dirty = true;
        }
        Key::Char(ch) => {
            lines[*cy].insert(*cx, ch);
            *cx += 1;
            *dirty = true;
        }
    }
}

fn command_key(
    key: Key,
    buf: &mut String,
    path: &str,
    lines: &mut Vec<Vec<u8>>,
    dirty: &mut bool,
    message: &mut String,
) -> Result<Step, i64> {
    match key {
        Key::Escape => return Ok(Step::Normal),
        Key::Backspace => {
            buf.pop();
            return Ok(Step::Stay);
        }
        Key::Char(ch) => {
            buf.push(ch as char);
            return Ok(Step::Stay);
        }
        Key::Enter => {}
        _ => return Ok(Step::Stay),
    }
    let cmd = buf.clone();
    let cmd = cmd.trim();
    match cmd {
        "w" => {
            save(path, lines)?;
            *dirty = false;
            message.push_str("written");
            Ok(Step::Normal)
        }
        "q" => {
            if *dirty {
                message.push_str("no write since last change (:q! to discard)");
                Ok(Step::Normal)
            } else {
                Ok(Step::Quit)
            }
        }
        "q!" => Ok(Step::Quit),
        "wq" | "x" => {
            save(path, lines)?;
            Ok(Step::Quit)
        }
        _ => {
            message.push_str("not a command: ");
            message.push_str(cmd);
            Ok(Step::Normal)
        }
    }
}

fn load(path: &str) -> Result<Vec<Vec<u8>>, i64> {
    let fd = match syscall::open(path, O_RDONLY) {
        Ok(fd) => fd,
        Err(err) if err == abi::ENOENT => return Ok(vec![Vec::new()]),
        Err(err) => return Err(err),
    };
    let mut data = Vec::new();
    let mut buf = [0u8; 256];
    loop {
        let n = syscall::read(fd as u64, &mut buf)?;
        if n == 0 {
            break;
        }
        if data.len() + n > 256 * 1024 {
            let _ = syscall::close(fd);
            return Err(27);
        }
        data.extend_from_slice(&buf[..n]);
    }
    let _ = syscall::close(fd);
    if data.is_empty() {
        return Ok(vec![Vec::new()]);
    }
    let mut lines = Vec::new();
    let mut current = Vec::new();
    for &byte in &data {
        if byte == b'\n' {
            lines.push(current);
            current = Vec::new();
        } else {
            current.push(byte);
        }
    }
    if data.last() != Some(&b'\n') {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    Ok(lines)
}

fn save(path: &str, lines: &[Vec<u8>]) -> Result<(), i64> {
    let fd = syscall::open(path, O_WRONLY | O_CREAT | O_TRUNC)?;
    for (index, line) in lines.iter().enumerate() {
        syscall::write_all(fd as u64, line)?;
        if index + 1 != lines.len() || !line.is_empty() {
            syscall::write_all(fd as u64, b"\n")?;
        }
    }
    syscall::close(fd)
}

fn fit(cx: &mut usize, cy: &mut usize, lines: &[Vec<u8>]) {
    if *cy >= lines.len() {
        *cy = lines.len() - 1;
    }
    if *cx > lines[*cy].len() {
        *cx = lines[*cy].len();
    }
}

fn draw(
    path: &str,
    lines: &[Vec<u8>],
    cx: usize,
    cy: usize,
    top: usize,
    rows: usize,
    cols: usize,
    mode: &Mode,
    dirty: bool,
    message: &str,
) {
    let mut screen = String::new();
    screen.push_str("\x1b[2J\x1b[H");
    let view = rows - 1;
    for row in 0..view {
        let index = top + row;
        if index < lines.len() {
            let line = &lines[index];
            let shown = line.len().min(cols.saturating_sub(1));
            for &byte in &line[..shown] {
                if (0x20..0x7f).contains(&byte) {
                    screen.push(byte as char);
                } else {
                    screen.push('?');
                }
            }
        }
        screen.push_str("\x1b[K");
        if row + 1 < view {
            screen.push('\n');
        }
    }
    screen.push_str(&alloc::format!("\x1b[{rows};1H\x1b[K"));
    match mode {
        Mode::Command(buf) => {
            screen.push(':');
            screen.push_str(buf);
        }
        _ if !message.is_empty() => screen.push_str(message),
        Mode::Insert => screen.push_str("-- INSERT --"),
        Mode::Normal => {
            screen.push_str(path);
            if dirty {
                screen.push_str(" [Modified]");
            }
            screen.push_str(" -- NORMAL --");
        }
    }
    let screen_row = cy - top + 1;
    let screen_col = (cx + 1).min(cols);
    screen.push_str(&alloc::format!("\x1b[{screen_row};{screen_col}H"));
    let _ = syscall::write_all(1, screen.as_bytes());
}

fn read_key() -> Key {
    let mut buf = [0u8; 1];
    loop {
        match syscall::read(0, &mut buf) {
            Ok(1) => {
                return match buf[0] {
                    0x1b => Key::Escape,
                    b'\n' | b'\r' => Key::Enter,
                    0x7f | 0x08 => Key::Backspace,
                    KEY_UP => Key::Up,
                    KEY_DOWN => Key::Down,
                    KEY_LEFT => Key::Left,
                    KEY_RIGHT => Key::Right,
                    KEY_HOME => Key::Home,
                    KEY_END => Key::End,
                    KEY_DELETE => Key::Delete,
                    byte => Key::Char(byte),
                };
            }
            Ok(_) => continue,
            Err(_) => return Key::Escape,
        }
    }
}
