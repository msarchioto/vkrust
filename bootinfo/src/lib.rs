//! What the loader passes to `kmain` in `rdi`.
//!
//! The loader writes one [`BootInfo`] and the arrays it points at, then jumps.
//! Those bytes stay mapped for the life of the kernel. Pointers are virtual
//! addresses in the higher-half direct map, not physical addresses.

#![no_std]

/// Virtual address of physical byte 0. `virtual = physical + hhdm_offset`.
pub const HHDM_OFFSET: u64 = 0xffff_8000_0000_0000;

/// `MemmapEntry::entry_type` for RAM the frame allocator may hand out.
pub const MEMMAP_USABLE: u64 = 0;

/// One physical range. The frame allocator uses `base` and `length` when
/// `entry_type` is [`MEMMAP_USABLE`] and ignores every other type.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemmapEntry {
    pub base: u64,
    pub length: u64,
    pub entry_type: u64,
}

/// Pixel buffer. `address` is a mapped virtual pointer. A null framebuffer
/// pointer in [`BootInfo`] means the console is serial only.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Framebuffer {
    pub address: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
    pub unused: [u8; 7],
    pub edid_size: u64,
    pub edid: *mut u8,
}

/// First argument of `kmain`.
#[repr(C)]
pub struct BootInfo {
    pub hhdm_offset: u64,
    pub memory_map: *const MemmapEntry,
    pub memory_map_len: u64,
    /// Null when the firmware has no framebuffer.
    pub framebuffer: *const Framebuffer,
}
