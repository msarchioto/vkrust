//! Keyboard and serial input, plus the tty line discipline.

use alloc::collections::VecDeque;
use pc_keyboard::{layouts, DecodedKey, HandleControl, KeyCode, KeyState, Keyboard, ScancodeSet1};
use spin::Mutex;

use crate::console;
use crate::serial;
use abi::{
    KEY_DELETE, KEY_DOWN, KEY_END, KEY_HOME, KEY_LEFT, KEY_RIGHT, KEY_UP,
};

const RING: usize = 512;

struct ByteRing {
    buf: [u8; RING],
    head: usize,
    tail: usize,
}

impl ByteRing {
    const fn new() -> Self {
        Self {
            buf: [0; RING],
            head: 0,
            tail: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        let next = (self.head + 1) % RING;
        if next == self.tail {
            return;
        }
        self.buf[self.head] = byte;
        self.head = next;
    }

    fn pop(&mut self) -> Option<u8> {
        if self.tail == self.head {
            return None;
        }
        let byte = self.buf[self.tail];
        self.tail = (self.tail + 1) % RING;
        Some(byte)
    }

    fn peek(&self) -> Option<u8> {
        if self.tail == self.head {
            None
        } else {
            Some(self.buf[self.tail])
        }
    }
}

enum Esc {
    None,
    Csi,
    Delete,
}

struct Tty {
    raw: bool,
    line: VecDeque<u8>,
    pending: VecDeque<u8>,
    serial: ByteRing,
    scancodes: ByteRing,
    esc: Esc,
    keyboard: Keyboard<layouts::Uk105Key, ScancodeSet1>,
}

impl Tty {
    fn new() -> Self {
        Self {
            raw: false,
            line: VecDeque::new(),
            pending: VecDeque::new(),
            serial: ByteRing::new(),
            scancodes: ByteRing::new(),
            esc: Esc::None,
            keyboard: Keyboard::new(ScancodeSet1::new(), layouts::Uk105Key, HandleControl::Ignore),
        }
    }
}

static TTY: Mutex<Option<Tty>> = Mutex::new(None);

pub fn init() {
    *TTY.lock() = Some(Tty::new());
}

pub fn push_scancode(byte: u8) {
    if let Some(tty) = TTY.lock().as_mut() {
        tty.scancodes.push(byte);
    }
}

pub fn drain_serial_port() {
    if let Some(tty) = TTY.lock().as_mut() {
        while let Some(byte) = serial::read_byte() {
            tty.serial.push(byte);
        }
    }
}

pub fn set_raw(raw: bool) {
    if let Some(tty) = TTY.lock().as_mut() {
        tty.raw = raw;
        if raw {
            tty.line.clear();
        }
    }
}

pub fn read(buf: &mut [u8]) -> usize {
    if buf.is_empty() {
        return 0;
    }
    loop {
        x86_64::instructions::interrupts::disable();
        drain_serial_port();
        let mut guard = TTY.lock();
        let Some(tty) = guard.as_mut() else {
            drop(guard);
            x86_64::instructions::interrupts::enable();
            return 0;
        };
        tty.decode();
        let n = tty.pending.len().min(buf.len());
        for slot in buf.iter_mut().take(n) {
            *slot = tty.pending.pop_front().unwrap();
        }
        drop(guard);
        if n > 0 {
            x86_64::instructions::interrupts::enable();
            return n;
        }
        unsafe {
            core::arch::asm!("sti", "hlt", options(nomem, nostack));
        }
    }
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

impl Tty {
    fn decode(&mut self) {
        self.decode_serial();
        self.decode_keyboard();
    }

    fn decode_serial(&mut self) {
        loop {
            let Some(byte) = self.serial.peek() else {
                return;
            };
            match self.esc {
                Esc::None => {
                    self.serial.pop();
                    if byte == 0x1b {
                        if self.serial.peek() == Some(b'[') {
                            self.serial.pop();
                            self.esc = Esc::Csi;
                        } else {
                            self.push_key(Key::Escape);
                        }
                    } else {
                        self.push_key(ascii_key(byte));
                    }
                }
                Esc::Csi => {
                    self.serial.pop();
                    self.esc = Esc::None;
                    match byte {
                        b'A' => self.push_key(Key::Up),
                        b'B' => self.push_key(Key::Down),
                        b'C' => self.push_key(Key::Right),
                        b'D' => self.push_key(Key::Left),
                        b'H' => self.push_key(Key::Home),
                        b'F' => self.push_key(Key::End),
                        b'3' => self.esc = Esc::Delete,
                        _ => {}
                    }
                }
                Esc::Delete => {
                    self.serial.pop();
                    self.esc = Esc::None;
                    if byte == b'~' {
                        self.push_key(Key::Delete);
                    }
                }
            }
        }
    }

    fn decode_keyboard(&mut self) {
        while let Some(scancode) = self.scancodes.pop() {
            let Ok(Some(event)) = self.keyboard.add_byte(scancode) else {
                continue;
            };
            // Key-up must reach the decoder. Otherwise Shift stays down and
            // the next press of the `.` key produces `>`.
            let state = event.state;
            let Some(decoded) = self.keyboard.process_keyevent(event) else {
                continue;
            };
            if state != KeyState::Down {
                continue;
            }
            let key = match decoded {
                DecodedKey::Unicode('\n') | DecodedKey::Unicode('\r') => Key::Enter,
                DecodedKey::Unicode('\u{8}') | DecodedKey::Unicode('\u{7f}') => Key::Backspace,
                DecodedKey::Unicode(ch) => match latin1(ch) {
                    Some(byte) => Key::Char(byte),
                    None => continue,
                },
                DecodedKey::RawKey(KeyCode::Escape) => Key::Escape,
                DecodedKey::RawKey(KeyCode::ArrowUp) => Key::Up,
                DecodedKey::RawKey(KeyCode::ArrowDown) => Key::Down,
                DecodedKey::RawKey(KeyCode::ArrowLeft) => Key::Left,
                DecodedKey::RawKey(KeyCode::ArrowRight) => Key::Right,
                DecodedKey::RawKey(KeyCode::Home) => Key::Home,
                DecodedKey::RawKey(KeyCode::End) => Key::End,
                DecodedKey::RawKey(KeyCode::Delete) => Key::Delete,
                DecodedKey::RawKey(KeyCode::Backspace) => Key::Backspace,
                DecodedKey::RawKey(KeyCode::Return) => Key::Enter,
                _ => continue,
            };
            self.push_key(key);
        }
    }

    fn push_key(&mut self, key: Key) {
        if self.raw {
            let bytes: &[u8] = match key {
                Key::Char(c) => {
                    self.pending.push_back(c);
                    return;
                }
                Key::Enter => b"\n",
                Key::Backspace => b"\x7f",
                Key::Escape => b"\x1b",
                Key::Up => &[KEY_UP],
                Key::Down => &[KEY_DOWN],
                Key::Left => &[KEY_LEFT],
                Key::Right => &[KEY_RIGHT],
                Key::Home => &[KEY_HOME],
                Key::End => &[KEY_END],
                Key::Delete => &[KEY_DELETE],
            };
            for &byte in bytes {
                self.pending.push_back(byte);
            }
            return;
        }

        match key {
            Key::Char(c) => {
                self.line.push_back(c);
                console::write_bytes(&[c]);
            }
            Key::Enter => {
                self.line.push_back(b'\n');
                console::write_bytes(b"\n");
                while let Some(byte) = self.line.pop_front() {
                    self.pending.push_back(byte);
                }
            }
            Key::Backspace => {
                if self.line.pop_back().is_some() {
                    console::write_bytes(b"\x08 \x08");
                }
            }
            _ => {}
        }
    }
}

/// ASCII, plus the Latin-1 letters a UK keyboard actually produces (`£`, `¬`).
fn latin1(ch: char) -> Option<u8> {
    if ch.is_control() {
        return None;
    }
    let code = u32::from(ch);
    if code <= 0xFF {
        Some(code as u8)
    } else {
        None
    }
}

fn ascii_key(byte: u8) -> Key {
    match byte {
        b'\r' | b'\n' => Key::Enter,
        0x7f | 0x08 => Key::Backspace,
        0x20..=0x7e => Key::Char(byte),
        _ => Key::Char(byte),
    }
}
