//! 4-level page tables built while the firmware identity map is still active.
//!
//! Every frame here is a physical address, and the firmware lets us write it
//! directly. After `CR3` changes, the kernel reaches these tables through the
//! higher-half direct map.

use crate::serial;
use bootinfo::HHDM_OFFSET;

const PAGE: u64 = 4096;
const HUGE: u64 = 2 * 1024 * 1024;
const PRESENT: u64 = 1;
const WRITE: u64 = 1 << 1;
const HUGE_PAGE: u64 = 1 << 7;
const PHYS_MASK: u64 = 0x000f_ffff_ffff_f000;

pub struct Mapper {
    pml4: u64,
    pool: u64,
    next: usize,
    count: usize,
}

impl Mapper {
    pub fn new(pool: u64, count: usize) -> Self {
        let mut mapper = Self {
            pml4: 0,
            pool,
            next: 0,
            count,
        };
        mapper.pml4 = mapper.frame();
        mapper
    }

    pub fn pml4(&self) -> u64 {
        self.pml4
    }

    pub fn map_hhdm(&mut self, phys: u64, len: u64) {
        if len == 0 {
            return;
        }
        self.map_range(phys.wrapping_add(HHDM_OFFSET), phys, len);
    }

    pub fn map_identity(&mut self, phys: u64, len: u64) {
        self.map_range(phys, phys, len);
    }

    pub fn map_range(&mut self, mut virt: u64, mut phys: u64, mut len: u64) {
        if len == 0 {
            return;
        }
        // Round outward so a range that starts mid-page still has a mapping.
        let lead = virt & (PAGE - 1);
        if phys < lead {
            serial::write_str("loader: range is not page aligned\r\n");
            halt();
        }
        virt -= lead;
        phys -= lead;
        len = (len + lead).next_multiple_of(PAGE);
        while len > 0 {
            if virt % HUGE == 0 && phys % HUGE == 0 && len >= HUGE {
                self.map(virt, phys, true);
                virt += HUGE;
                phys += HUGE;
                len -= HUGE;
            } else {
                self.map(virt, phys, false);
                virt += PAGE;
                phys += PAGE;
                len -= PAGE;
            }
        }
    }

    fn map(&mut self, virt: u64, phys: u64, huge: bool) {
        let i4 = ((virt >> 39) & 0x1ff) as usize;
        let i3 = ((virt >> 30) & 0x1ff) as usize;
        let i2 = ((virt >> 21) & 0x1ff) as usize;
        let i1 = ((virt >> 12) & 0x1ff) as usize;
        let pdpt = self.ensure(self.pml4, i4);
        let pd = self.ensure(pdpt, i3);
        let pd_entry = unsafe { slot(pd, i2).read() };
        if huge {
            if pd_entry & PRESENT != 0 {
                let have = pd_entry & PHYS_MASK & !(HUGE - 1);
                if pd_entry & HUGE_PAGE != 0 && have == (phys & !(HUGE - 1)) {
                    return;
                }
                serial::write_str("loader: page table split\r\n");
                halt();
            }
            unsafe {
                slot(pd, i2).write(phys | PRESENT | WRITE | HUGE_PAGE);
            }
            return;
        }
        if pd_entry & PRESENT != 0 && pd_entry & HUGE_PAGE != 0 {
            let have = pd_entry & PHYS_MASK & !(HUGE - 1);
            let offset = virt & (HUGE - 1);
            if have + offset == (phys & !(PAGE - 1)) {
                return;
            }
            serial::write_str("loader: page table split\r\n");
            halt();
        }
        let pt = self.ensure(pd, i2);
        let leaf = unsafe { slot(pt, i1).read() };
        if leaf & PRESENT != 0 {
            if (leaf & PHYS_MASK) == (phys & PHYS_MASK) {
                return;
            }
            serial::write_str("loader: page already mapped\r\n");
            halt();
        }
        unsafe {
            slot(pt, i1).write(phys | PRESENT | WRITE);
        }
    }

    fn ensure(&mut self, table: u64, index: usize) -> u64 {
        let entry = unsafe { slot(table, index).read() };
        if entry & PRESENT != 0 {
            if entry & HUGE_PAGE != 0 {
                serial::write_str("loader: page table split\r\n");
                halt();
            }
            return entry & 0x000f_ffff_ffff_f000;
        }
        let frame = self.frame();
        unsafe {
            slot(table, index).write(frame | PRESENT | WRITE);
        }
        frame
    }

    fn frame(&mut self) -> u64 {
        if self.next >= self.count {
            serial::write_str("loader: page table pool is empty\r\n");
            halt();
        }
        let phys = self.pool + self.next as u64 * PAGE;
        self.next += 1;
        unsafe {
            core::ptr::write_bytes(phys as *mut u8, 0, PAGE as usize);
        }
        phys
    }
}

fn slot(table: u64, index: usize) -> *mut u64 {
    unsafe { (table as *mut u64).add(index) }
}

pub fn halt() -> ! {
    loop {
        unsafe { core::arch::asm!("hlt") }
    }
}
