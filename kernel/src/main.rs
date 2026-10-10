//! fern — a small x86_64 kernel. Limine boots it; the shell runs in ring 3.

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

mod boot;
mod console;
mod gdt;
mod input;
mod interrupts;
mod memory;
mod process;
mod serial;
mod syscall;
mod vfs;

use core::panic::PanicInfo;
use x86_64::instructions::port::Port;
use x86_64::registers::control::{Cr4, Cr4Flags};

use crate::console::kprintln;
use crate::serial::serial_println;

static SHELL_ELF: &[u8] = include_bytes!(env!("USER_ELF"));

#[unsafe(no_mangle)]
extern "C" fn kmain() -> ! {
    serial::init();
    serial_println!("fern kernel");

    // The kernel copies syscall arguments out of user pages.
    unsafe {
        let mut flags = Cr4::read();
        flags.remove(Cr4Flags::SUPERVISOR_MODE_ACCESS_PREVENTION);
        flags.remove(Cr4Flags::SUPERVISOR_MODE_EXECUTION_PROTECTION);
        Cr4::write(flags);
    }

    let boot = boot::take();
    gdt::init();
    input::init();
    interrupts::init();

    let offset = x86_64::VirtAddr::new(boot.hhdm_offset);
    let mut mapper = unsafe { memory::mapper(offset) };
    let mut frames = memory::FrameAlloc::new(boot.memory_map, offset);
    memory::init_heap(&mut mapper, &mut frames).expect("kernel heap");
    console::init(boot.framebuffer);
    vfs::init();

    kprintln!("fern 0.1.0");
    process::enter(&mut mapper, &mut frames, SHELL_ELF);
}

pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

pub fn exit_qemu(code: u32) -> ! {
    unsafe {
        Port::<u32>::new(0xf4).write(code);
    }
    hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("PANIC: {info}");
    exit_qemu(0x11);
}
