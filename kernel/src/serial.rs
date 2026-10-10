//! 16550 UART at COM1. Used for boot logs and as a serial console.

use core::fmt;
use spin::Mutex;
use x86_64::instructions::interrupts;
use x86_64::instructions::port::Port;

const DATA: u16 = 0x3F8;
const IER: u16 = 0x3F9;
const FCR: u16 = 0x3FA;
const LCR: u16 = 0x3FB;
const MCR: u16 = 0x3FC;
const LSR: u16 = 0x3FD;

static SERIAL: Mutex<()> = Mutex::new(());

pub fn init() {
    unsafe {
        Port::<u8>::new(IER).write(0x00);
        Port::<u8>::new(LCR).write(0x80);
        Port::<u8>::new(DATA).write(0x01); // 115200 baud
        Port::<u8>::new(IER).write(0x00);
        Port::<u8>::new(LCR).write(0x03); // 8n1
        Port::<u8>::new(FCR).write(0xC7);
        Port::<u8>::new(MCR).write(0x0B); // RTS, DTR, OUT2 (IRQ enable)
        Port::<u8>::new(IER).write(0x01); // receive interrupt
    }
}

pub fn write_bytes(bytes: &[u8]) {
    interrupts::without_interrupts(|| {
        let _guard = SERIAL.lock();
        write_bytes_raw(bytes);
    });
}

/// Write without taking the lock. Interrupt handlers are already running with
/// interrupts cleared, and must not re-enter the mutex.
pub fn write_bytes_raw(bytes: &[u8]) {
    for &byte in bytes {
        write_byte(byte);
    }
}

fn write_byte(byte: u8) {
    unsafe {
        let mut lsr = Port::<u8>::new(LSR);
        let mut data = Port::<u8>::new(DATA);
        while lsr.read() & 0x20 == 0 {}
        data.write(byte);
    }
}

pub fn read_byte() -> Option<u8> {
    unsafe {
        let mut lsr = Port::<u8>::new(LSR);
        if lsr.read() & 0x01 == 0 {
            return None;
        }
        Some(Port::<u8>::new(DATA).read())
    }
}

struct Writer;

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        write_bytes(s.as_bytes());
        Ok(())
    }
}

pub fn write_fmt(args: fmt::Arguments) {
    use core::fmt::Write;
    let mut writer = Writer;
    let _ = writer.write_fmt(args);
}

macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::write_fmt(format_args!($($arg)*))
    };
}
pub(crate) use serial_print;

macro_rules! serial_println {
    () => { $crate::serial::serial_print!("\n") };
    ($($arg:tt)*) => {
        $crate::serial::serial_print!("{}\n", format_args!($($arg)*))
    };
}
pub(crate) use serial_println;
