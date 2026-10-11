//! UEFI loader. It reads `\boot\kernel`, builds page tables, and jumps to `kmain`.

#![no_std]
#![no_main]

extern crate alloc;

mod elf;
mod paging;
mod serial;

use alloc::vec::Vec;
use bootinfo::{BootInfo, Framebuffer, HHDM_OFFSET, MEMMAP_USABLE, MemmapEntry};
use core::fmt::Write;
use paging::Mapper;
use uefi::mem::memory_map::MemoryMap;
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
use uefi::proto::media::fs::SimpleFileSystem;

const PAGE: u64 = 4096;
const STACK_PAGES: usize = 32;
const INFO_PAGES: usize = 4;
const TABLE_POOL: usize = 256;
const MAX_MAP: usize = 256;

#[repr(C)]
struct Handoff {
    info: BootInfo,
    fb: Framebuffer,
    map: [MemmapEntry; MAX_MAP],
}

struct FirmwareFb {
    phys: u64,
    size: u64,
    fb: Framebuffer,
}

#[entry]
fn main() -> Status {
    serial::init();
    if let Err(err) = uefi::helpers::init() {
        let _ = err;
        serial::write_str("loader: uefi init failed\r\n");
        paging::halt();
    }
    let _ = uefi::boot::set_watchdog_timer(0, 0x10000, None);
    boot();
}

fn boot() -> ! {
    serial::write_str("loader: read \\boot\\kernel\r\n");
    let bytes = read_kernel();
    serial::write_str("loader: kernel bytes ");
    serial::write_hex(bytes.len() as u64);
    serial::write_str("\r\n");

    let mut alloc_pages = |count: usize| uefi_pages(count);
    let loaded = elf::load(&bytes, &mut alloc_pages);
    drop(bytes);

    let stack = uefi_pages(STACK_PAGES);
    let info_phys = uefi_pages(INFO_PAGES);
    let trampoline = low_page();
    let fb = firmware_fb();
    let pool = uefi_pages(TABLE_POOL);

    let ranges = snapshot_ranges();
    let mut mapper = Mapper::new(pool, TABLE_POOL);
    for &(base, len) in &ranges {
        mapper.map_hhdm(base, len);
    }
    if let Some(fb) = &fb {
        let (base, len) = page_span(fb.phys, fb.size);
        mapper.map_hhdm(base, len);
    }
    for seg in loaded.segments.iter().take(loaded.count) {
        mapper.map_range(seg.virt, seg.phys, seg.len);
    }
    mapper.map_identity(trampoline, PAGE);
    install_trampoline(trampoline);

    serial::write_str("loader: entry ");
    serial::write_hex(loaded.entry);
    serial::write_str(" cr3 ");
    serial::write_hex(mapper.pml4());
    serial::write_str("\r\n");

    // Firmware interrupt handlers die with the boot services.
    unsafe { core::arch::asm!("cli") };
    let mmap = unsafe { uefi::boot::exit_boot_services(None) };
    let map_len = fill_handoff(info_phys, &mmap, fb.as_ref());
    core::mem::forget(mmap);

    serial::write_str("loader: jump, usable ranges ");
    serial::write_hex(map_len as u64);
    serial::write_str("\r\n");

    let stack_top = stack + HHDM_OFFSET + STACK_PAGES as u64 * PAGE;
    let stack_top = (stack_top & !0xf) - 8;
    jump(
        trampoline + 0x20,
        trampoline + 0x30,
        mapper.pml4(),
        stack_top,
        loaded.entry,
        info_phys + HHDM_OFFSET,
    );
}

fn read_kernel() -> Vec<u8> {
    let image = uefi::boot::image_handle();
    if let Ok(proto) = uefi::boot::get_image_file_system(image) {
        let mut fs = uefi::fs::FileSystem::new(proto);
        if let Ok(bytes) = fs.read(cstr16!("\\boot\\kernel")) {
            return bytes;
        }
    }
    serial::write_str("loader: boot volume has no \\boot\\kernel, scanning\r\n");
    let handles = match uefi::boot::find_handles::<SimpleFileSystem>() {
        Ok(handles) => handles,
        Err(_) => {
            serial::write_str("loader: no file system on the boot volume\r\n");
            paging::halt();
        }
    };
    for handle in handles {
        let Ok(proto) = uefi::boot::open_protocol_exclusive::<SimpleFileSystem>(handle) else {
            continue;
        };
        let mut fs = uefi::fs::FileSystem::new(proto);
        if let Ok(bytes) = fs.read(cstr16!("\\boot\\kernel")) {
            return bytes;
        }
    }
    serial::write_str("loader: \\boot\\kernel is missing\r\n");
    paging::halt();
}

fn firmware_fb() -> Option<FirmwareFb> {
    let handle = uefi::boot::get_handle_for_protocol::<GraphicsOutput>().ok()?;
    let mut gop = uefi::boot::open_protocol_exclusive::<GraphicsOutput>(handle).ok()?;
    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();
    let stride = mode.stride();
    if width == 0 || height == 0 || stride == 0 {
        return None;
    }
    let (red_shift, red_size, green_shift, green_size, blue_shift, blue_size) =
        match mode.pixel_format() {
            PixelFormat::Rgb => (0, 8, 8, 8, 16, 8),
            PixelFormat::Bgr => (16, 8, 8, 8, 0, 8),
            PixelFormat::Bitmask => {
                let mask = mode.pixel_bitmask()?;
                let (r_shift, r_size) = mask_parts(mask.red);
                let (g_shift, g_size) = mask_parts(mask.green);
                let (b_shift, b_size) = mask_parts(mask.blue);
                (r_shift, r_size, g_shift, g_size, b_shift, b_size)
            }
            PixelFormat::BltOnly => return None,
        };
    let mut frame = gop.frame_buffer();
    let phys = frame.as_mut_ptr() as u64;
    let size = frame.size() as u64;
    if phys == 0 || size == 0 {
        return None;
    }
    Some(FirmwareFb {
        phys,
        size,
        fb: Framebuffer {
            address: (phys + HHDM_OFFSET) as *mut u8,
            width: width as u64,
            height: height as u64,
            pitch: (stride * 4) as u64,
            bpp: 32,
            memory_model: 1,
            red_mask_size: red_size,
            red_mask_shift: red_shift,
            green_mask_size: green_size,
            green_mask_shift: green_shift,
            blue_mask_size: blue_size,
            blue_mask_shift: blue_shift,
            unused: [0; 7],
            edid_size: 0,
            edid: core::ptr::null_mut(),
        },
    })
}

fn mask_parts(mask: u32) -> (u8, u8) {
    if mask == 0 {
        return (0, 0);
    }
    let shift = mask.trailing_zeros() as u8;
    let size = (mask >> shift).trailing_ones() as u8;
    (shift, size)
}

fn snapshot_ranges() -> Vec<(u64, u64)> {
    let map = match uefi::boot::memory_map(uefi::boot::MemoryType::LOADER_DATA) {
        Ok(map) => map,
        Err(_) => {
            serial::write_str("loader: memory map failed\r\n");
            paging::halt();
        }
    };
    let mut ranges = Vec::new();
    for desc in map.entries() {
        if desc.page_count == 0 || !direct_mapped(desc.ty) {
            continue;
        }
        ranges.push((desc.phys_start, desc.page_count * PAGE));
    }
    ranges
}

fn direct_mapped(ty: uefi::boot::MemoryType) -> bool {
    ty == uefi::boot::MemoryType::CONVENTIONAL
        || ty == uefi::boot::MemoryType::BOOT_SERVICES_CODE
        || ty == uefi::boot::MemoryType::BOOT_SERVICES_DATA
        || ty == uefi::boot::MemoryType::LOADER_CODE
        || ty == uefi::boot::MemoryType::LOADER_DATA
}

fn usable(ty: uefi::boot::MemoryType) -> bool {
    ty == uefi::boot::MemoryType::CONVENTIONAL
        || ty == uefi::boot::MemoryType::BOOT_SERVICES_CODE
        || ty == uefi::boot::MemoryType::BOOT_SERVICES_DATA
}

fn fill_handoff(info_phys: u64, map: &impl MemoryMap, fb: Option<&FirmwareFb>) -> usize {
    let handoff = info_phys as *mut Handoff;
    unsafe {
        core::ptr::write_bytes(handoff as *mut u8, 0, INFO_PAGES * PAGE as usize);
    }
    let mut len = 0;
    let fb_range = fb.map(|fb| page_span(fb.phys, fb.size));
    'entries: for desc in map.entries() {
        if desc.page_count == 0 || !usable(desc.ty) {
            continue;
        }
        let base = desc.phys_start;
        let length = desc.page_count * PAGE;
        for piece in subtract(base, length, fb_range) {
            let Some((piece_base, piece_len)) = piece else {
                continue;
            };
            if len == MAX_MAP {
                serial::write_str("loader: memory map truncated\r\n");
                break 'entries;
            }
            unsafe {
                (*handoff).map[len] = MemmapEntry {
                    base: piece_base,
                    length: piece_len,
                    entry_type: MEMMAP_USABLE,
                };
            }
            len += 1;
        }
    }
    if len == 0 {
        serial::write_str("loader: no usable memory\r\n");
        paging::halt();
    }
    let map_virt = info_phys + core::mem::offset_of!(Handoff, map) as u64 + HHDM_OFFSET;
    let fb_virt = info_phys + core::mem::offset_of!(Handoff, fb) as u64 + HHDM_OFFSET;
    unsafe {
        if let Some(fb) = fb {
            (*handoff).fb = fb.fb;
        }
        (*handoff).info = BootInfo {
            hhdm_offset: HHDM_OFFSET,
            memory_map: map_virt as *const MemmapEntry,
            memory_map_len: len as u64,
            framebuffer: if fb.is_some() {
                fb_virt as *const Framebuffer
            } else {
                core::ptr::null()
            },
        };
    }
    len
}

fn page_span(base: u64, len: u64) -> (u64, u64) {
    let start = base & !(PAGE - 1);
    let end = (base + len).next_multiple_of(PAGE);
    (start, end - start)
}

/// Cut `hole` out of `[base, base + len)`, yielding the pieces that remain.
fn subtract(base: u64, len: u64, hole: Option<(u64, u64)>) -> [Option<(u64, u64)>; 2] {
    let Some((hole_base, hole_len)) = hole else {
        return [Some((base, len)), None];
    };
    let end = base + len;
    let hole_end = hole_base + hole_len;
    if hole_end <= base || hole_base >= end {
        return [Some((base, len)), None];
    }
    let left = if hole_base > base {
        Some((base, hole_base - base))
    } else {
        None
    };
    let right = if hole_end < end {
        Some((hole_end, end - hole_end))
    } else {
        None
    };
    [left, right]
}

fn uefi_pages(count: usize) -> u64 {
    match uefi::boot::allocate_pages(
        uefi::boot::AllocateType::AnyPages,
        uefi::boot::MemoryType::LOADER_DATA,
        count,
    ) {
        Ok(ptr) => {
            unsafe { core::ptr::write_bytes(ptr.as_ptr(), 0, count * PAGE as usize) };
            ptr.as_ptr() as u64
        }
        Err(_) => {
            serial::write_str("loader: allocate_pages failed\r\n");
            paging::halt();
        }
    }
}

fn low_page() -> u64 {
    match uefi::boot::allocate_pages(
        uefi::boot::AllocateType::MaxAddress(0x8_0000),
        uefi::boot::MemoryType::LOADER_DATA,
        1,
    ) {
        Ok(ptr) => {
            let phys = ptr.as_ptr() as u64;
            if phys >= 0x8_0000 {
                serial::write_str("loader: trampoline is too high\r\n");
                paging::halt();
            }
            unsafe { core::ptr::write_bytes(ptr.as_ptr(), 0, PAGE as usize) };
            phys
        }
        Err(_) => {
            serial::write_str("loader: low page failed\r\n");
            paging::halt();
        }
    }
}

fn install_trampoline(page: u64) {
    unsafe {
        let gdt = page as *mut u64;
        gdt.write(0);
        gdt.add(1).write(0x00af_9a00_0000_ffff);
        gdt.add(2).write(0x00cf_9200_0000_ffff);
        let gdtr = (page + 0x20) as *mut u8;
        (gdtr as *mut u16).write(3 * 8 - 1);
        (gdtr.add(2) as *mut u64).write_unaligned(page);
        // mov cr3, rcx; mov ax, 0x10; mov ss, ax; mov rsp, rsi; jmp rdx
        let stub: &[u8] = &[
            0x48, 0x0f, 0x22, 0xd9, 0x66, 0xb8, 0x10, 0x00, 0x8e, 0xd0, 0x48, 0x89, 0xf4, 0xff,
            0xe2,
        ];
        core::ptr::copy_nonoverlapping(stub.as_ptr(), (page + 0x30) as *mut u8, stub.len());
    }
}

fn jump(gdtr: u64, stub: u64, cr3: u64, stack: u64, entry: u64, info: u64) -> ! {
    unsafe {
        core::arch::asm!(
            "cli",
            "lgdt [r8]",
            "push r9",
            "push r10",
            "retfq",
            in("r8") gdtr,
            in("r9") 0x08u64,
            in("r10") stub,
            in("rcx") cr3,
            in("rsi") stack,
            in("rdx") entry,
            in("rdi") info,
            options(noreturn),
        );
    }
}

struct SerialWriter;

impl Write for SerialWriter {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        serial::write_str(text);
        Ok(())
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let _ = write!(SerialWriter, "loader panic: {info}\r\n");
    paging::halt();
}
