use spin::Lazy;
use x86_64::instructions::segmentation::{CS, Segment, SS};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;
use x86_64::PrivilegeLevel;

pub const DOUBLE_FAULT_IST: u16 = 0;
const STACK_SIZE: usize = 64 * 1024;

#[repr(C, align(16))]
struct Stack([u8; STACK_SIZE]);

// Rust would put never-written statics in `.rodata`. The CPU pushes interrupt
// frames here, so the stacks have to be writable.
#[unsafe(link_section = ".bss")]
static RSP0_STACK: Stack = Stack([0; STACK_SIZE]);
#[unsafe(link_section = ".bss")]
static DF_STACK: Stack = Stack([0; STACK_SIZE]);

fn top(stack: &'static Stack) -> VirtAddr {
    let end = unsafe { stack.0.as_ptr().add(STACK_SIZE) };
    VirtAddr::new(end as u64)
}

pub struct Selectors {
    pub kernel_code: SegmentSelector,
    pub kernel_data: SegmentSelector,
    pub user_code: SegmentSelector,
    pub user_data: SegmentSelector,
}

static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    tss.privilege_stack_table[0] = top(&RSP0_STACK);
    tss.interrupt_stack_table[DOUBLE_FAULT_IST as usize] = top(&DF_STACK);
    tss
});

struct Gdt {
    table: GlobalDescriptorTable,
    selectors: Selectors,
    tss: SegmentSelector,
}

static GDT: Lazy<Gdt> = Lazy::new(|| {
    let mut table = GlobalDescriptorTable::new();
    let kernel_code = table.append(Descriptor::kernel_code_segment());
    let kernel_data = table.append(Descriptor::kernel_data_segment());
    let user_data = table.append(Descriptor::user_data_segment());
    let user_code = table.append(Descriptor::user_code_segment());
    let tss = table.append(Descriptor::tss_segment(&TSS));
    Gdt {
        table,
        selectors: Selectors {
            kernel_code,
            kernel_data,
            user_code,
            user_data,
        },
        tss,
    }
});

pub fn init() {
    GDT.table.load();
    unsafe {
        CS::set_reg(GDT.selectors.kernel_code);
        SS::set_reg(GDT.selectors.kernel_data);
        load_tss(GDT.tss);
    }
}

pub fn user_cs() -> u16 {
    SegmentSelector::new(GDT.selectors.user_code.index(), PrivilegeLevel::Ring3).0
}

pub fn user_ss() -> u16 {
    SegmentSelector::new(GDT.selectors.user_data.index(), PrivilegeLevel::Ring3).0
}
