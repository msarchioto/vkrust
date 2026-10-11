//! Load the static kernel ELF into freshly allocated frames.
//!
//! The kernel is linked at a fixed address. The bytes are copied to the
//! physical frames now, while the firmware identity map is still in place.
//! The page tables that point the link address at those frames are built later.

use crate::paging::halt;
use crate::serial;

const PT_LOAD: u32 = 1;
const MAX_SEGMENTS: usize = 8;

#[derive(Clone, Copy)]
pub struct Segment {
    pub virt: u64,
    pub phys: u64,
    pub len: u64,
}

pub struct Loaded {
    pub entry: u64,
    pub segments: [Segment; MAX_SEGMENTS],
    pub count: usize,
}

pub fn load(bytes: &[u8], alloc: &mut dyn FnMut(usize) -> u64) -> Loaded {
    if bytes.len() < 64 || bytes[0..4] != [0x7f, b'E', b'L', b'F'] || bytes[4] != 2 {
        serial::write_str("loader: kernel is not an ELF64\r\n");
        halt();
    }
    let entry = read_u64(bytes, 24);
    let phoff = read_u64(bytes, 32) as usize;
    let phentsize = read_u16(bytes, 54) as usize;
    let phnum = read_u16(bytes, 56) as usize;
    if phentsize < 56 || phoff.saturating_add(phentsize.saturating_mul(phnum)) > bytes.len() {
        serial::write_str("loader: bad program headers\r\n");
        halt();
    }

    let mut segments = [Segment {
        virt: 0,
        phys: 0,
        len: 0,
    }; MAX_SEGMENTS];
    let mut count = 0;
    for i in 0..phnum {
        let off = phoff + i * phentsize;
        let kind = read_u32(bytes, off);
        if kind != PT_LOAD {
            continue;
        }
        if count == MAX_SEGMENTS {
            serial::write_str("loader: too many segments\r\n");
            halt();
        }
        let file_off = read_u64(bytes, off + 8) as usize;
        let virt = read_u64(bytes, off + 16);
        let filesz = read_u64(bytes, off + 32) as usize;
        let memsz = read_u64(bytes, off + 40);
        if file_off.saturating_add(filesz) > bytes.len() {
            serial::write_str("loader: segment runs past the file\r\n");
            halt();
        }
        let start = virt & !0xfff;
        let end = (virt + memsz).next_multiple_of(4096);
        let pages = ((end - start) / 4096) as usize;
        let phys = alloc(pages);
        unsafe {
            core::ptr::write_bytes(phys as *mut u8, 0, pages * 4096);
            core::ptr::copy_nonoverlapping(
                bytes[file_off..file_off + filesz].as_ptr(),
                (phys + (virt - start)) as *mut u8,
                filesz,
            );
        }
        segments[count] = Segment {
            virt: start,
            phys,
            len: end - start,
        };
        count += 1;
    }
    if count == 0 {
        serial::write_str("loader: kernel has no loadable segment\r\n");
        halt();
    }
    Loaded {
        entry,
        segments,
        count,
    }
}

fn read_u16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn read_u64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
