use core::arch::asm;
use core::mem::size_of;

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_DATA_SELECTOR: u16 = 0x18 | 3;
pub const USER_CODE_SELECTOR: u16 = 0x20 | 3;
pub const TSS_SELECTOR: u16 = 0x28;

pub const DOUBLE_FAULT_IST_INDEX: u8 = 1;

const GDT_ENTRIES: usize = 7;
const STACK_SIZE: usize = 16 * 1024;

const GDT_TSS_LOW: usize = 5;
const GDT_TSS_HIGH: usize = 6;

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

#[repr(C, packed)]
struct TaskStateSegment {
    _reserved0: u32,
    rsp: [u64; 3],
    _reserved1: u64,
    ist: [u64; 7],
    _reserved2: u64,
    _reserved3: u16,
    iomap_base: u16,
}

#[repr(align(16))]
struct Stack([u8; STACK_SIZE]);

static mut GDT: [u64; GDT_ENTRIES] = [
    0,
    segment_descriptor(0x9A, 0xA), // Ring 0 code: execute/read, long mode
    segment_descriptor(0x92, 0xC), // Ring 0 data: read/write
    segment_descriptor(0xF2, 0xC), // Ring 3 data: read/write
    segment_descriptor(0xFA, 0xA), // Ring 3 code: execute/read, long mode
    0,
    0,
];

static mut TSS: TaskStateSegment = TaskStateSegment {
    _reserved0: 0,
    rsp: [0; 3],
    _reserved1: 0,
    ist: [0; 7],
    _reserved2: 0,
    _reserved3: 0,
    iomap_base: size_of::<TaskStateSegment>() as u16,
};

static mut RING0_STACK: Stack = Stack([0; STACK_SIZE]);
static mut DOUBLE_FAULT_STACK: Stack = Stack([0; STACK_SIZE]);

/// Load the kernel GDT and TSS used after entering long mode.
///
/// The selector layout intentionally keeps the boot selector values:
/// - 0x08: kernel code
/// - 0x10: kernel data
///
/// New selectors for userspace are:
/// - 0x1b: user data
/// - 0x23: user code
/// - 0x28: TSS
pub fn init() {
    unsafe {
        init_tss();
        init_tss_descriptor();
        load_gdt();
        load_data_segments();
        load_tss();
    }
}

/// Update RSP0 for transitions from Ring 3 back into the kernel.
///
/// Call this when switching tasks once each task has its own kernel stack.
pub unsafe fn set_kernel_stack(stack_top: usize) {
    let tss = core::ptr::addr_of_mut!(TSS);
    unsafe {
        (*tss).rsp[0] = stack_top as u64;
    }
}

pub fn ring0_stack_top() -> usize {
    stack_top(core::ptr::addr_of!(RING0_STACK))
}

pub fn double_fault_stack_top() -> usize {
    stack_top(core::ptr::addr_of!(DOUBLE_FAULT_STACK))
}

unsafe fn init_tss() {
    let tss = core::ptr::addr_of_mut!(TSS);
    unsafe {
        (*tss).rsp[0] = ring0_stack_top() as u64;
        (*tss).ist[(DOUBLE_FAULT_IST_INDEX - 1) as usize] = double_fault_stack_top() as u64;
    }
}

unsafe fn init_tss_descriptor() {
    let tss_base = core::ptr::addr_of!(TSS) as u64;
    let tss_limit = (size_of::<TaskStateSegment>() - 1) as u32;
    let (low, high) = tss_descriptor(tss_base, tss_limit);
    let gdt = core::ptr::addr_of_mut!(GDT) as *mut u64;
    unsafe {
        *gdt.add(GDT_TSS_LOW) = low;
        *gdt.add(GDT_TSS_HIGH) = high;
    }
}

unsafe fn load_gdt() {
    let pointer = DescriptorTablePointer {
        limit: (size_of::<[u64; GDT_ENTRIES]>() - 1) as u16,
        base: core::ptr::addr_of!(GDT) as u64,
    };
    unsafe {
        asm!("lgdt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags));
    }
}

unsafe fn load_data_segments() {
    unsafe {
        asm!(
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            in("ax") KERNEL_DATA_SELECTOR,
            options(nostack, preserves_flags),
        );
    }
}

unsafe fn load_tss() {
    unsafe {
        asm!("ltr ax", in("ax") TSS_SELECTOR, options(nostack, preserves_flags));
    }
}

const fn segment_descriptor(access: u8, flags: u8) -> u64 {
    let limit = 0x000F_FFFFu64;
    (limit & 0xFFFF)
        | ((access as u64) << 40)
        | (((limit >> 16) & 0xF) << 48)
        | ((flags as u64) << 52)
}

const fn tss_descriptor(base: u64, limit: u32) -> (u64, u64) {
    let low = ((limit as u64) & 0xFFFF)
        | ((base & 0xFF_FFFF) << 16)
        | (0x89u64 << 40)
        | (((limit as u64 >> 16) & 0xF) << 48)
        | (((base >> 24) & 0xFF) << 56);
    let high = base >> 32;
    (low, high)
}

fn stack_top(stack: *const Stack) -> usize {
    stack as usize + STACK_SIZE
}
