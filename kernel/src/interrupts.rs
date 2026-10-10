use pic8259::ChainedPics;
use spin::Lazy;
use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::PrivilegeLevel;
use x86_64::VirtAddr;

use crate::gdt::DOUBLE_FAULT_IST;
use crate::serial::serial_println;
use crate::{exit_qemu, input};

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = 40;
const KEYBOARD_INTERRUPT: u8 = PIC_1_OFFSET + 1;
const SERIAL_INTERRUPT: u8 = PIC_1_OFFSET + 4;

pub static PICS: spin::Mutex<ChainedPics> =
    spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    idt.breakpoint.set_handler_fn(breakpoint);
    idt.invalid_opcode.set_handler_fn(invalid_opcode);
    idt.page_fault.set_handler_fn(page_fault);
    idt.general_protection_fault.set_handler_fn(general_protection);
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault)
            .set_stack_index(DOUBLE_FAULT_IST);
        idt[KEYBOARD_INTERRUPT].set_handler_fn(keyboard);
        idt[SERIAL_INTERRUPT].set_handler_fn(serial);
        idt[0x80]
            .set_handler_addr(VirtAddr::new(syscall_entry as *const () as u64))
            .set_privilege_level(PrivilegeLevel::Ring3);
    }
    idt
});

pub fn init() {
    IDT.load();
    unsafe {
        PICS.lock().initialize();
        // Unmask keyboard (IRQ1) and serial (IRQ4). Mask everything else so the
        // timer does not sit unacknowledged and block the PIC.
        Port::<u8>::new(0x21).write(0xED);
        Port::<u8>::new(0xA1).write(0xFF);
    }
    x86_64::instructions::interrupts::enable();
}

extern "x86-interrupt" fn breakpoint(frame: InterruptStackFrame) {
    serial_println!("breakpoint at {:?}", frame.instruction_pointer);
}

extern "x86-interrupt" fn invalid_opcode(frame: InterruptStackFrame) {
    serial_println!("invalid opcode at {:?}", frame.instruction_pointer);
    exit_qemu(0x11);
}

extern "x86-interrupt" fn page_fault(frame: InterruptStackFrame, code: PageFaultErrorCode) {
    let addr = x86_64::registers::control::Cr2::read();
    serial_println!(
        "page fault at {:?} addr={:?} code={:?}",
        frame.instruction_pointer,
        addr,
        code
    );
    exit_qemu(0x11);
}

extern "x86-interrupt" fn general_protection(frame: InterruptStackFrame, code: u64) {
    serial_println!(
        "general protection at {:?} code={:#x} cs={:?} ss={:?}",
        frame.instruction_pointer,
        code,
        frame.code_segment,
        frame.stack_segment
    );
    exit_qemu(0x11);
}

extern "x86-interrupt" fn double_fault(frame: InterruptStackFrame, _code: u64) -> ! {
    serial_println!("double fault at {:?}", frame.instruction_pointer);
    exit_qemu(0x11);
}

extern "x86-interrupt" fn keyboard(_frame: InterruptStackFrame) {
    let scancode: u8 = unsafe { Port::new(0x60).read() };
    input::push_scancode(scancode);
    unsafe { PICS.lock().notify_end_of_interrupt(KEYBOARD_INTERRUPT) };
}

extern "x86-interrupt" fn serial(_frame: InterruptStackFrame) {
    input::drain_serial_port();
    unsafe { PICS.lock().notify_end_of_interrupt(SERIAL_INTERRUPT) };
}

#[repr(C)]
pub struct SyscallRegs {
    pub rbx: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rax: u64,
}

/// Interrupt entry from ring 3. Ten pushes keep `RSP ≡ 8 (mod 16)` before `call`.
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        "push rax",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push rbx",
        "mov rdi, rsp",
        "call {handler}",
        "pop rbx",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rax",
        "iretq",
        handler = sym syscall_trampoline,
    );
}

extern "C" fn syscall_trampoline(regs: *mut SyscallRegs) {
    let regs = unsafe { &mut *regs };
    regs.rax = crate::syscall::dispatch(regs.rax, regs.rdi, regs.rsi, regs.rdx, regs.r10) as u64;
}


