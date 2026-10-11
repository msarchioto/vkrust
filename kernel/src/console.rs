//! Framebuffer text console. The same byte stream is also copied to the serial port.

use core::fmt;
use core::fmt::Write;
use font8x8::{UnicodeFonts, BASIC_FONTS, LATIN_FONTS};
use spin::Mutex;

use crate::boot::Framebuffer;
use crate::serial;

const DEFAULT_COLS: usize = 80;
const DEFAULT_ROWS: usize = 24;

#[derive(Clone, Copy)]
struct Cell {
    ch: u8,
}

pub struct Console {
    fb: *mut u8,
    width: usize,
    height: usize,
    pitch: usize,
    bpp: usize,
    red_shift: u8,
    green_shift: u8,
    blue_shift: u8,
    scale: usize,
    cols: usize,
    rows: usize,
    col: usize,
    row: usize,
    cells: alloc::vec::Vec<Cell>,
    csi: bool,
    csi_priv: bool,
    args: [usize; 4],
    arg_i: usize,
    arg_any: bool,
    cursor_on: bool,
}

unsafe impl Send for Console {}

static CONSOLE: Mutex<Option<Console>> = Mutex::new(None);
static WINSZ: Mutex<(u16, u16)> = Mutex::new((DEFAULT_ROWS as u16, DEFAULT_COLS as u16));

pub fn init(fb: Option<&Framebuffer>) {
    let Some(fb) = fb else {
        return;
    };
    if fb.width == 0 || fb.height == 0 || fb.bpp < 8 {
        return;
    }
    let bpp = (fb.bpp as usize) / 8;
    if bpp == 0 {
        return;
    }
    let scale = if fb.width >= 900 { 2 } else { 1 };
    let cols = (fb.width as usize / (8 * scale)).max(1);
    let rows = (fb.height as usize / (8 * scale)).max(1);
    let mut console = Console {
        fb: fb.address,
        width: fb.width as usize,
        height: fb.height as usize,
        pitch: fb.pitch as usize,
        bpp,
        red_shift: fb.red_mask_shift,
        green_shift: fb.green_mask_shift,
        blue_shift: fb.blue_mask_shift,
        scale,
        cols,
        rows,
        col: 0,
        row: 0,
        cells: alloc::vec![
            Cell { ch: b' ' };
            cols * rows
        ],
        csi: false,
        csi_priv: false,
        args: [0; 4],
        arg_i: 0,
        arg_any: false,
        cursor_on: true,
    };
    console.fill_screen(rgb(0x14, 0x16, 0x1c));
    *WINSZ.lock() = (rows as u16, cols as u16);
    *CONSOLE.lock() = Some(console);
}

pub fn window_size() -> (u16, u16) {
    *WINSZ.lock()
}

pub fn write_bytes(bytes: &[u8]) {
    if let Some(console) = CONSOLE.lock().as_mut() {
        for &byte in bytes {
            console.byte(byte);
        }
        console.paint_cursor();
    }
    serial::write_bytes(bytes);
}

pub fn write_fmt(args: fmt::Arguments) {
    struct Adapter;
    impl fmt::Write for Adapter {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            write_bytes(s.as_bytes());
            Ok(())
        }
    }
    let mut adapter = Adapter;
    let _ = adapter.write_fmt(args);
}

macro_rules! kprint {
    ($($arg:tt)*) => { $crate::console::write_fmt(format_args!($($arg)*)) };
}
pub(crate) use kprint;

macro_rules! kprintln {
    () => { $crate::console::kprint!("\n") };
    ($($arg:tt)*) => { $crate::console::kprint!("{}\n", format_args!($($arg)*)) };
}
pub(crate) use kprintln;

fn rgb(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    (r, g, b)
}

impl Console {
    fn byte(&mut self, byte: u8) {
        if self.csi {
            self.csi_byte(byte);
            return;
        }
        match byte {
            0x1b => {
                self.csi = false;
                self.csi_priv = false;
                self.args = [0; 4];
                self.arg_i = 0;
                self.arg_any = false;
                self.csi = true;
            }
            b'\n' => self.newline(),
            b'\r' => self.col = 0,
            0x08 => self.cursor_left(),
            b'\t' => {
                let next = (self.col + 8) & !7;
                while self.col < next && self.col < self.cols {
                    self.put(b' ');
                }
            }
            byte => self.put(byte),
        }
    }

    fn csi_byte(&mut self, byte: u8) {
        match byte {
            b'[' if !self.arg_any && self.arg_i == 0 && self.args[0] == 0 => {}
            b'?' => self.csi_priv = true,
            b'0'..=b'9' => {
                self.arg_any = true;
                self.args[self.arg_i] = self.args[self.arg_i]
                    .saturating_mul(10)
                    .saturating_add((byte - b'0') as usize);
            }
            b';' => {
                if self.arg_i + 1 < self.args.len() {
                    self.arg_i += 1;
                }
                self.arg_any = true;
            }
            b'H' | b'f' => {
                let row = self.args[0].saturating_sub(1).min(self.rows.saturating_sub(1));
                let col = if self.arg_i == 0 && !self.arg_any && self.args[0] == 0 {
                    0
                } else {
                    self.args[1].saturating_sub(1).min(self.cols.saturating_sub(1))
                };
                let row = if self.args[0] == 0 { 0 } else { row };
                self.row = row;
                self.col = col;
                self.end_csi();
            }
            b'J' => {
                if self.args[0] == 2 || !self.arg_any {
                    if self.args[0] == 2 {
                        self.clear();
                    }
                }
                if self.args[0] == 2 {
                    self.clear();
                }
                self.end_csi();
            }
            b'K' => {
                self.erase_to_eol();
                self.end_csi();
            }
            b'm' | b'l' | b'h' | b'A' | b'B' | b'C' | b'D' => self.end_csi(),
            0x40..=0x7e => self.end_csi(),
            _ => {}
        }
    }

    fn end_csi(&mut self) {
        self.csi = false;
        self.csi_priv = false;
        self.args = [0; 4];
        self.arg_i = 0;
        self.arg_any = false;
    }

    fn put(&mut self, ch: u8) {
        if self.col >= self.cols {
            self.newline();
        }
        self.cells[self.row * self.cols + self.col].ch = ch;
        self.paint(self.col, self.row, false);
        self.col += 1;
    }

    /// Move onto the previous cell, wrapping to the end of the previous row.
    /// The cursor is drawn over a cell, so that cell is painted normally
    /// before the cursor moves. Otherwise backspace leaves a solid block
    /// where the cursor used to sit.
    fn cursor_left(&mut self) {
        if self.cols == 0 || self.rows == 0 {
            return;
        }
        if self.cursor_on {
            let col = self.col.min(self.cols - 1);
            let row = self.row.min(self.rows - 1);
            self.paint(col, row, false);
        }
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.cols - 1;
        }
    }

    fn newline(&mut self) {
        self.col = 0;
        if self.row + 1 < self.rows {
            self.row += 1;
        } else {
            self.scroll();
        }
    }

    fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.ch = b' ';
        }
        self.col = 0;
        self.row = 0;
        self.fill_screen(rgb(0x14, 0x16, 0x1c));
    }

    fn erase_to_eol(&mut self) {
        for col in self.col..self.cols {
            self.cells[self.row * self.cols + col].ch = b' ';
            self.paint(col, self.row, false);
        }
    }

    fn scroll(&mut self) {
        let cols = self.cols;
        self.cells.copy_within(cols.., 0);
        for cell in &mut self.cells[(self.rows - 1) * cols..] {
            cell.ch = b' ';
        }
        self.redraw();
    }

    fn paint_cursor(&mut self) {
        if !self.cursor_on {
            return;
        }
        let col = self.col.min(self.cols - 1);
        let row = self.row.min(self.rows - 1);
        self.paint(col, row, true);
    }

    fn redraw(&mut self) {
        for row in 0..self.rows {
            for col in 0..self.cols {
                self.paint(col, row, false);
            }
        }
    }

    fn glyph_for(ch: u8) -> [u8; 8] {
        let ch = char::from(ch);
        BASIC_FONTS
            .get(ch)
            .or_else(|| LATIN_FONTS.get(ch))
            .unwrap_or([0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
    }

    fn paint(&mut self, col: usize, row: usize, cursor: bool) {
        let ch = self.cells[row * self.cols + col].ch;
        let (fg, bg) = if cursor {
            (rgb(0x14, 0x16, 0x1c), rgb(0xc8, 0xcc, 0xd4))
        } else {
            (rgb(0xc8, 0xcc, 0xd4), rgb(0x14, 0x16, 0x1c))
        };
        let glyph = Self::glyph_for(ch);
        let origin_x = col * 8 * self.scale;
        let origin_y = row * 8 * self.scale;
        for gy in 0..8 {
            let bits = glyph[gy];
            for gx in 0..8 {
                let on = bits & (1 << gx) != 0;
                let color = if on { fg } else { bg };
                for sy in 0..self.scale {
                    for sx in 0..self.scale {
                        self.pixel(
                            origin_x + gx * self.scale + sx,
                            origin_y + gy * self.scale + sy,
                            color,
                        );
                    }
                }
            }
        }
    }

    fn fill_screen(&mut self, color: (u8, u8, u8)) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.pixel(x, y, color);
            }
        }
    }

    fn pixel(&mut self, x: usize, y: usize, color: (u8, u8, u8)) {
        if x >= self.width || y >= self.height {
            return;
        }
        let offset = y * self.pitch + x * self.bpp;
        let (r, g, b) = color;
        let value = (u32::from(r) << self.red_shift)
            | (u32::from(g) << self.green_shift)
            | (u32::from(b) << self.blue_shift);
        unsafe {
            let ptr = self.fb.add(offset);
            if self.bpp >= 3 {
                ptr.write(value as u8);
                ptr.add(1).write((value >> 8) as u8);
                ptr.add(2).write((value >> 16) as u8);
            }
            if self.bpp >= 4 {
                ptr.add(3).write((value >> 24) as u8);
            }
        }
    }
}
