//! Load the shell ELF and enter it at ring 3.

use abi::{USER_HEAP_SIZE, USER_HEAP_START, USER_STACK_SIZE, USER_STACK_TOP};
use x86_64::structures::paging::{OffsetPageTable, PageTableFlags};

use crate::gdt;
use crate::memory::{self, FrameAlloc};
use crate::serial::serial_println;
use crate::syscall;

const PAGE: u64 = 4096;
const PT_LOAD: u32 = 1;

pub fn enter(mapper: &mut OffsetPageTable, alloc: &mut FrameAlloc, elf: &[u8]) -> ! {
    let entry = load_elf(mapper, alloc, elf);
    let stack_start = USER_STACK_TOP - USER_STACK_SIZE;
    let user = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
    memory::map_zeroed(mapper, alloc, stack_start, USER_STACK_SIZE, user)
        .expect("user stack");
    memory::map_zeroed(mapper, alloc, USER_HEAP_START, USER_HEAP_SIZE, user).expect("user heap");

    let mut regions = [(0u64, 0u64); 8];
    let mut count = 0;
    push_region(&mut regions, &mut count, stack_start, USER_STACK_TOP);
    push_region(
        &mut regions,
        &mut count,
        USER_HEAP_START,
        USER_HEAP_START + USER_HEAP_SIZE,
    );
    // The loader records image regions in the high slots via a side channel on the ELF pass.
    for region in image_regions(elf) {
        push_region(&mut regions, &mut count, region.0, region.1);
    }
    syscall::set_user_regions(&regions[..count]);

    let cs = u64::from(gdt::user_cs());
    let ss = u64::from(gdt::user_ss());
    serial_println!("entering user at {entry:#x} cs={cs:#x} ss={ss:#x}");

    unsafe {
        core::arch::asm!(
            "push {ss}",
            "push {rsp}",
            "push {rflags}",
            "push {cs}",
            "push {rip}",
            "iretq",
            ss = in(reg) ss,
            rsp = in(reg) USER_STACK_TOP,
            rflags = in(reg) 0x202u64,
            cs = in(reg) cs,
            rip = in(reg) entry,
            options(noreturn),
        );
    }
}

fn push_region(regions: &mut [(u64, u64); 8], count: &mut usize, start: u64, end: u64) {
    if *count >= regions.len() || start >= end {
        return;
    }
    regions[*count] = (start, end);
    *count += 1;
}

fn image_regions(elf: &[u8]) -> alloc::vec::Vec<(u64, u64)> {
    let mut out = alloc::vec::Vec::new();
    let phoff = read_u64(elf, 32).unwrap_or(0) as usize;
    let phentsize = read_u16(elf, 54).unwrap_or(0) as usize;
    let phnum = read_u16(elf, 56).unwrap_or(0) as usize;
    for index in 0..phnum {
        let off = phoff + index * phentsize;
        if read_u32(elf, off).unwrap_or(0) != PT_LOAD {
            continue;
        }
        let vaddr = read_u64(elf, off + 16).unwrap_or(0);
        let memsz = read_u64(elf, off + 40).unwrap_or(0);
        if memsz == 0 {
            continue;
        }
        let start = vaddr & !(PAGE - 1);
        let end = (vaddr + memsz + PAGE - 1) & !(PAGE - 1);
        out.push((start, end));
    }
    out
}

fn load_elf(mapper: &mut OffsetPageTable, alloc: &mut FrameAlloc, elf: &[u8]) -> u64 {
    if elf.len() < 64 || &elf[0..4] != b"\x7fELF" || elf[4] != 2 || elf[5] != 1 {
        panic!("shell is not an ELF64 binary");
    }
    let entry = read_u64(elf, 24).expect("elf entry");
    let phoff = read_u64(elf, 32).expect("elf phoff") as usize;
    let phentsize = read_u16(elf, 54).expect("elf phentsize") as usize;
    let phnum = read_u16(elf, 56).expect("elf phnum") as usize;
    let user = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

    for index in 0..phnum {
        let off = phoff + index * phentsize;
        let kind = read_u32(elf, off).expect("phdr type");
        if kind != PT_LOAD {
            continue;
        }
        let offset = read_u64(elf, off + 8).expect("p_offset") as usize;
        let vaddr = read_u64(elf, off + 16).expect("p_vaddr");
        let filesz = read_u64(elf, off + 32).expect("p_filesz") as usize;
        let memsz = read_u64(elf, off + 40).expect("p_memsz");
        if vaddr < 0x400000 || vaddr.saturating_add(memsz) > USER_STACK_TOP - USER_STACK_SIZE {
            panic!("elf segment {vaddr:#x}+{memsz:#x} is outside the user image area");
        }
        let map_start = vaddr & !(PAGE - 1);
        let map_end = (vaddr + memsz + PAGE - 1) & !(PAGE - 1);
        let mut page = map_start;
        while page < map_end {
            memory::map_page(mapper, alloc, page, user).expect("map elf");
            page += PAGE;
        }
        let src = elf.get(offset..offset + filesz).expect("elf segment bytes");
        unsafe {
            core::ptr::copy_nonoverlapping(src.as_ptr(), vaddr as *mut u8, filesz);
        }
        serial_println!("loaded segment {vaddr:#x} file={filesz:#x} mem={memsz:#x}");
    }
    entry
}

fn read_u16(elf: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes(elf.get(off..off + 2)?.try_into().ok()?))
}
fn read_u32(elf: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(elf.get(off..off + 4)?.try_into().ok()?))
}
fn read_u64(elf: &[u8], off: usize) -> Option<u64> {
    Some(u64::from_le_bytes(elf.get(off..off + 8)?.try_into().ok()?))
}
