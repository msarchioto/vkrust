#![no_std]
#![no_main]

extern crate alloc;

mod shell;
mod syscall;
mod vim;

use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;

use abi::{USER_HEAP_SIZE, USER_HEAP_START};

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    unsafe {
        ALLOCATOR
            .lock()
            .init(USER_HEAP_START as *mut u8, USER_HEAP_SIZE as usize);
    }
    shell::run();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let message = alloc::format!("user panic: {info}\n");
    let _ = syscall::write_all(1, message.as_bytes());
    syscall::exit(101);
}
