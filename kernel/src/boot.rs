//! Boot handoff. The UEFI loader passes a [`BootInfo`] in `rdi`.

use bootinfo::BootInfo;

pub use bootinfo::{Framebuffer, MEMMAP_USABLE, MemmapEntry};

pub struct Boot {
    pub hhdm_offset: u64,
    pub framebuffer: Option<&'static Framebuffer>,
    pub memory_map: &'static [MemmapEntry],
}

/// # Safety
/// `info` must be the pointer the loader passed to `kmain`: non-null, aligned,
/// and pointing at a [`BootInfo`] whose map and framebuffer stay mapped.
pub unsafe fn take(info: *const BootInfo) -> Boot {
    if info.is_null() || (info as usize) % core::mem::align_of::<BootInfo>() != 0 {
        panic!("boot info pointer is missing");
    }
    let info = unsafe { &*info };
    if info.memory_map.is_null() || info.memory_map_len == 0 {
        panic!("boot info has no memory map");
    }
    let memory_map =
        unsafe { core::slice::from_raw_parts(info.memory_map, info.memory_map_len as usize) };
    let framebuffer = if info.framebuffer.is_null() {
        None
    } else {
        Some(unsafe { &*info.framebuffer })
    };
    Boot {
        hhdm_offset: info.hhdm_offset,
        framebuffer,
        memory_map,
    }
}
