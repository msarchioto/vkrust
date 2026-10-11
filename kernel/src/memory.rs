//! Physical frame allocator and kernel heap, on top of the loader's page tables.

use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB,
    Translate,
};
use x86_64::{PhysAddr, VirtAddr};

use crate::boot::{MemmapEntry, MEMMAP_USABLE};

pub const HEAP_START: u64 = 0x0000_4444_4444_0000;
pub const HEAP_SIZE: u64 = 4 * 1024 * 1024;

const PAGE: u64 = 4096;

#[derive(Clone, Copy)]
struct Region {
    start: u64,
    end: u64,
}

pub struct FrameAlloc {
    regions: [Region; 64],
    count: usize,
    next: usize,
    offset: VirtAddr,
}

impl FrameAlloc {
    pub fn new(entries: &[MemmapEntry], offset: VirtAddr) -> Self {
        let mut regions = [Region { start: 0, end: 0 }; 64];
        let mut count = 0;
        for entry in entries {
            if entry.entry_type != MEMMAP_USABLE || entry.length < PAGE {
                continue;
            }
            let mut start = (entry.base + PAGE - 1) & !(PAGE - 1);
            let end = (entry.base + entry.length) & !(PAGE - 1);
            if start < 0x10_0000 {
                start = 0x10_0000;
            }
            if start >= end || count >= regions.len() {
                continue;
            }
            regions[count] = Region { start, end };
            count += 1;
        }
        Self {
            regions,
            count,
            next: 0,
            offset,
        }
    }

    fn usable(&self) -> impl Iterator<Item = PhysFrame> + '_ {
        self.regions[..self.count]
            .iter()
            .flat_map(|region| (region.start..region.end).step_by(PAGE as usize))
            .map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for FrameAlloc {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable().nth(self.next);
        if frame.is_some() {
            self.next += 1;
        }
        frame
    }
}

pub unsafe fn mapper(offset: VirtAddr) -> OffsetPageTable<'static> {
    let (frame, _) = Cr3::read();
    let virt = offset + frame.start_address().as_u64();
    let table = unsafe { &mut *virt.as_mut_ptr::<PageTable>() };
    unsafe { OffsetPageTable::new(table, offset) }
}

pub fn map_zeroed(
    mapper: &mut OffsetPageTable,
    alloc: &mut FrameAlloc,
    start: u64,
    size: u64,
    flags: PageTableFlags,
) -> Result<(), &'static str> {
    if size == 0 {
        return Ok(());
    }
    let map_start = start & !(PAGE - 1);
    let map_end = (start + size + PAGE - 1) & !(PAGE - 1);
    let mut addr = map_start;
    while addr < map_end {
        map_page(mapper, alloc, addr, flags)?;
        addr += PAGE;
    }
    Ok(())
}

pub fn map_page(
    mapper: &mut OffsetPageTable,
    alloc: &mut FrameAlloc,
    addr: u64,
    flags: PageTableFlags,
) -> Result<(), &'static str> {
    let page = Page::<Size4KiB>::containing_address(VirtAddr::new(addr));
    if mapper.translate_addr(page.start_address()).is_some() {
        // Keep the existing frame (overlapping ELF segments share a page) but
        // make sure the requested flags, including user access, are set.
        unsafe {
            mapper
                .update_flags(page, flags)
                .map_err(|_| "failed to update page flags")?
                .flush();
        }
        return Ok(());
    }
    let frame = alloc.allocate_frame().ok_or("out of physical frames")?;
    unsafe {
        core::ptr::write_bytes(
            (alloc.offset + frame.start_address().as_u64()).as_mut_ptr::<u8>(),
            0,
            PAGE as usize,
        );
        mapper
            .map_to(page, frame, flags, alloc)
            .map_err(|_| "failed to map a page")?
            .flush();
    }
    Ok(())
}

#[global_allocator]
static ALLOCATOR: linked_list_allocator::LockedHeap = linked_list_allocator::LockedHeap::empty();

pub fn init_heap(mapper: &mut OffsetPageTable, alloc: &mut FrameAlloc) -> Result<(), &'static str> {
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    map_zeroed(mapper, alloc, HEAP_START, HEAP_SIZE, flags)?;
    unsafe {
        ALLOCATOR.lock().init(HEAP_START as *mut u8, HEAP_SIZE as usize);
    }
    Ok(())
}
