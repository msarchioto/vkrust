//! COM1, before the kernel's own driver is running.

use x86_64::instructions::port::Port;

const DATA: u16 = 0x3F8;
const IER: u16 = 0x3F9;
const FCR: u16 = 0x3FA;
const LCR: u16 = 0x3FB;
const MCR: u16 = 0x3FC;
const LSR: u16 = 0x3FD;

pub fn init() {
    unsafe {
        Port::<u8>::new(IER).write(0x00);
        Port::<u8>::new(LCR).write(0x80);
        Port::<u8>::new(DATA).write(0x01);
        Port::<u8>::new(IER).write(0x00);
        Port::<u8>::new(LCR).write(0x03);
        Port::<u8>::new(FCR).write(0xC7);
        Port::<u8>::new(MCR).write(0x0B);
        // Leave the receive interrupt off. The kernel arms it after the PIC exists.
        Port::<u8>::new(IER).write(0x00);
    }
}

pub fn write_str(text: &str) {
    for byte in text.bytes() {
        write_byte(byte);
    }
}

pub fn write_hex(value: u64) {
    write_str("0x");
    for shift in (0..16).rev() {
        let nibble = ((value >> (shift * 4)) & 0xf) as u8;
        let ascii = if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + nibble - 10
        };
        write_byte(ascii);
    }
}

fn write_byte(byte: u8) {
    unsafe {
        while Port::<u8>::new(LSR).read() & 0x20 == 0 {}
        Port::<u8>::new(DATA).write(byte);
    }
}
