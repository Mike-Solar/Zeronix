pub mod exception_handler;

/// # 中断向量表（IDT）相关代码
/// 处理中断。
/// x96_64下CPU中断分为3类：CPU 内部异常、可屏蔽中断、软件自定义中断
/// ## CPU异常
/// | 向量     | 名称                          | 类型         | 说明                     |
/// | ------ | --------------------------- | ---------- | -----------------           |
/// | 0      | **Divide Error**            | Fault      | 除零                         |
/// | 1      | **Debug**                   | Trap/Fault | 调试断点                      |
/// | 2      | **NMI**                     | Interrupt  | 不可屏蔽中断（内存错误、硬件故障） |
/// | 3      | **Breakpoint**              | Trap       | `int3` 指令                  |
/// | 4      | **Overflow**                | Trap       | `into` 指令溢出               |
/// | 5      | **Bound Range**             | Fault      | `bound` 指令越界              |
/// | 6      | **Invalid Opcode**          | Fault      | 遇到非法指令                   |
/// | 7      | **Device Not Available**    | Fault      | FPU 不存在（延迟加载）          |
/// | **8**  | **Double Fault**            | **Abort**  | **处理异常时又出异常，致命**     |
/// | 9      | Coprocessor Segment Overrun | Abort      | 废弃（486 前）                 |
/// | 10     | Invalid TSS                 | Fault      | TSS 描述符错误                 |
/// | 11     | Segment Not Present         | Fault      | 段不存在                      |
/// | 12     | Stack Segment Fault         | Fault      | 栈段错误                      |
/// | **13** | **General Protection**      | **Fault**  | **#GP，权限错误、非法访问**     |
/// | **14** | **Page Fault**              | **Fault**  | **缺页，必须处理**             |
/// | 15     | Reserved                    | —          |                              |
/// | 16     | x87 FPE                     | Fault      | 浮点异常                      |
/// | 17     | Alignment Check             | Fault      | 未对齐访问                     |
/// | 18     | Machine Check               | Abort      | 硬件自检失败（内存/总线）         |
/// | 19     | SIMD FPE                    | Fault      | SSE 异常                     |
/// | 20~31  | Reserved                    | —          | 保留                          |
///
/// 硬件中断（IRQ，可屏蔽）
/// | IRQ     | 默认向量   | 设备                |
/// | ------- | ------ | --------------------- |
/// | IRQ0    | **32** | PIT 时钟               |
/// | IRQ1    | 33     | 键盘                   |
/// | IRQ2    | 34     | 级联（从片）             |
/// | IRQ3~7  | 35~39  | 串口、并口              |
/// | IRQ8    | 40     | RTC                   |
/// | IRQ9~15 | 41~47  | 硬盘、网卡等             |
use core::arch::{asm, global_asm};
use core::mem::size_of;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::trap::gdt::{DOUBLE_FAULT_IST_INDEX, KERNEL_CODE_SELECTOR};
use crate::trap::pic::{self, IRQ_BASE, IRQ_COUNT};
use crate::{printk, stdio::LogLevel};

const IDT_ENTRIES: usize = 256;
const INTERRUPT_GATE: u8 = 0x8E;

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,        // Interrupt Stack Table index (0 = 不用 IST)
    type_attr: u8,  // 0x8E = Interrupt Gate, Present, Ring 0
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    pub const fn missing() -> Self {
        Self{
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    pub fn set_handler(&mut self, handler: Handler, ist: u8) {
        let addr = handler as usize as u64;
        self.offset_low = addr as u16;
        self.selector = KERNEL_CODE_SELECTOR;
        self.ist = ist & 0x7;
        self.type_attr = INTERRUPT_GATE;
        self.offset_mid = (addr >> 16) as u16;
        self.offset_high = (addr >> 32) as u32;
        self.reserved = 0;
    }
}

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

type Handler = unsafe extern "C" fn();

static mut IDT: [IdtEntry; IDT_ENTRIES] = [IdtEntry::missing(); IDT_ENTRIES];
static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

pub fn timer_ticks() -> u64 {
    TIMER_TICKS.load(Ordering::Relaxed)
}

pub fn init() {
    init_entries();
    load();
}

unsafe extern "C" {
    fn __exception_stub_0();
    fn __exception_stub_1();
    fn __exception_stub_2();
    fn __exception_stub_3();
    fn __exception_stub_4();
    fn __exception_stub_5();
    fn __exception_stub_6();
    fn __exception_stub_7();
    fn __exception_stub_8();
    fn __exception_stub_9();
    fn __exception_stub_10();
    fn __exception_stub_11();
    fn __exception_stub_12();
    fn __exception_stub_13();
    fn __exception_stub_14();
    fn __exception_stub_15();
    fn __exception_stub_16();
    fn __exception_stub_17();
    fn __exception_stub_18();
    fn __exception_stub_19();
    fn __exception_stub_20();
    fn __exception_stub_21();
    fn __exception_stub_22();
    fn __exception_stub_23();
    fn __exception_stub_24();
    fn __exception_stub_25();
    fn __exception_stub_26();
    fn __exception_stub_27();
    fn __exception_stub_28();
    fn __exception_stub_29();
    fn __exception_stub_30();
    fn __exception_stub_31();
    fn __irq_stub_32();
    fn __irq_stub_33();
    fn __irq_stub_34();
    fn __irq_stub_35();
    fn __irq_stub_36();
    fn __irq_stub_37();
    fn __irq_stub_38();
    fn __irq_stub_39();
    fn __irq_stub_40();
    fn __irq_stub_41();
    fn __irq_stub_42();
    fn __irq_stub_43();
    fn __irq_stub_44();
    fn __irq_stub_45();
    fn __irq_stub_46();
    fn __irq_stub_47();
}

const EXCEPTION_HANDLERS: [Handler; 32] = [
    __exception_stub_0,
    __exception_stub_1,
    __exception_stub_2,
    __exception_stub_3,
    __exception_stub_4,
    __exception_stub_5,
    __exception_stub_6,
    __exception_stub_7,
    __exception_stub_8,
    __exception_stub_9,
    __exception_stub_10,
    __exception_stub_11,
    __exception_stub_12,
    __exception_stub_13,
    __exception_stub_14,
    __exception_stub_15,
    __exception_stub_16,
    __exception_stub_17,
    __exception_stub_18,
    __exception_stub_19,
    __exception_stub_20,
    __exception_stub_21,
    __exception_stub_22,
    __exception_stub_23,
    __exception_stub_24,
    __exception_stub_25,
    __exception_stub_26,
    __exception_stub_27,
    __exception_stub_28,
    __exception_stub_29,
    __exception_stub_30,
    __exception_stub_31,
];

const IRQ_HANDLERS: [Handler; IRQ_COUNT as usize] = [
    __irq_stub_32,
    __irq_stub_33,
    __irq_stub_34,
    __irq_stub_35,
    __irq_stub_36,
    __irq_stub_37,
    __irq_stub_38,
    __irq_stub_39,
    __irq_stub_40,
    __irq_stub_41,
    __irq_stub_42,
    __irq_stub_43,
    __irq_stub_44,
    __irq_stub_45,
    __irq_stub_46,
    __irq_stub_47,
];

fn init_entries() {
    unsafe {
        let idt = core::ptr::addr_of_mut!(IDT) as *mut IdtEntry;
        let mut vector = 0;
        while vector < EXCEPTION_HANDLERS.len() {
            let ist = if vector == 8 {
                DOUBLE_FAULT_IST_INDEX
            } else {
                0
            };
            (*idt.add(vector)).set_handler(EXCEPTION_HANDLERS[vector], ist);
            vector += 1;
        }

        let mut irq = 0;
        while irq < IRQ_HANDLERS.len() {
            (*idt.add(IRQ_BASE as usize + irq)).set_handler(IRQ_HANDLERS[irq], 0);
            irq += 1;
        }
    }
}

fn load() {
    unsafe {
        let pointer = DescriptorTablePointer {
            limit: (size_of::<[IdtEntry; IDT_ENTRIES]>() - 1) as u16,
            base: core::ptr::addr_of!(IDT) as u64,
        };
        asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags));
    }
}


#[unsafe(no_mangle)]
pub extern "C" fn __cpu_exception_handler(vector: u64, error_code: u64, rip: u64) -> ! {
    printk!(
    LogLevel::Error,
    "CPU exception #{} ({}) error={:#x} rip={:#x}",
    vector,
    exception_name(vector as usize),
    error_code,
    rip
 );
    panic!("unhandled CPU exception");
}

#[unsafe(no_mangle)]
pub extern "C" fn __irq_handler(vector: u64) {
    let irq = vector.saturating_sub(IRQ_BASE as u64) as u8;

    match irq {
        0 => {
            TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
            pic::end_of_interrupt(irq);
            crate::task::proc::on_timer_tick();
            return;
        }
        1 => {
            let scancode = unsafe { crate::trap::inb(0x60) };
            printk!(LogLevel::Info, "keyboard scancode {:#x}", scancode);
        }
        4 => {
            crate::stdio::handle_serial_rx_interrupt();
        }
        _ => {
            printk!(LogLevel::Warning, "unexpected IRQ {} vector {}", irq, vector);
        }
    }

    pic::end_of_interrupt(irq);
}

fn exception_name(vector: usize) -> &'static str {
    match vector {
        0 => "Divide Error",
        1 => "Debug",
        2 => "NMI",
        3 => "Breakpoint",
        4 => "Overflow",
        5 => "Bound Range Exceeded",
        6 => "Invalid Opcode",
        7 => "Device Not Available",
        8 => "Double Fault",
        9 => "Coprocessor Segment Overrun",
        10 => "Invalid TSS",
        11 => "Segment Not Present",
        12 => "Stack Segment Fault",
        13 => "General Protection Fault",
        14 => "Page Fault",
        15 => "Reserved",
        16 => "x87 Floating-Point Exception",
        17 => "Alignment Check",
        18 => "Machine Check",
        19 => "SIMD Floating-Point Exception",
        20 => "Virtualization Exception",
        21 => "Control Protection Exception",
        22..=27 => "Reserved",
        28 => "Hypervisor Injection Exception",
        29 => "VMM Communication Exception",
        30 => "Security Exception",
        31 => "Reserved",
        _ => "Unknown",
    }
}

global_asm!(
    r#"
    .macro EXCEPTION_NO_ERROR vector
    .global __exception_stub_\vector
    __exception_stub_\vector:
        cld
        mov rdi, \vector
        xor rsi, rsi
        mov rdx, [rsp]
        and rsp, -16
        sub rsp, 8
        jmp __cpu_exception_handler
    .endm

    .macro EXCEPTION_WITH_ERROR vector
    .global __exception_stub_\vector
    __exception_stub_\vector:
        cld
        mov rdi, \vector
        mov rsi, [rsp]
        mov rdx, [rsp + 8]
        and rsp, -16
        sub rsp, 8
        jmp __cpu_exception_handler
    .endm

    EXCEPTION_NO_ERROR 0
    EXCEPTION_NO_ERROR 1
    EXCEPTION_NO_ERROR 2
    EXCEPTION_NO_ERROR 3
    EXCEPTION_NO_ERROR 4
    EXCEPTION_NO_ERROR 5
    EXCEPTION_NO_ERROR 6
    EXCEPTION_NO_ERROR 7
    EXCEPTION_WITH_ERROR 8
    EXCEPTION_NO_ERROR 9
    EXCEPTION_WITH_ERROR 10
    EXCEPTION_WITH_ERROR 11
    EXCEPTION_WITH_ERROR 12
    EXCEPTION_WITH_ERROR 13
    EXCEPTION_WITH_ERROR 14
    EXCEPTION_NO_ERROR 15
    EXCEPTION_NO_ERROR 16
    EXCEPTION_WITH_ERROR 17
    EXCEPTION_NO_ERROR 18
    EXCEPTION_NO_ERROR 19
    EXCEPTION_NO_ERROR 20
    EXCEPTION_WITH_ERROR 21
    EXCEPTION_NO_ERROR 22
    EXCEPTION_NO_ERROR 23
    EXCEPTION_NO_ERROR 24
    EXCEPTION_NO_ERROR 25
    EXCEPTION_NO_ERROR 26
    EXCEPTION_NO_ERROR 27
    EXCEPTION_NO_ERROR 28
    EXCEPTION_WITH_ERROR 29
    EXCEPTION_WITH_ERROR 30
    EXCEPTION_NO_ERROR 31

    .macro IRQ_STUB vector
    .global __irq_stub_\vector
    __irq_stub_\vector:
        cld
        push rax
        push rcx
        push rdx
        push rbx
        push rbp
        push rsi
        push rdi
        push r8
        push r9
        push r10
        push r11
        push r12
        push r13
        push r14
        push r15
        mov rbx, rsp
        and rsp, -16
        mov rdi, \vector
        call __irq_handler
        mov rsp, rbx
        pop r15
        pop r14
        pop r13
        pop r12
        pop r11
        pop r10
        pop r9
        pop r8
        pop rdi
        pop rsi
        pop rbp
        pop rbx
        pop rdx
        pop rcx
        pop rax
        iretq
    .endm

    IRQ_STUB 32
    IRQ_STUB 33
    IRQ_STUB 34
    IRQ_STUB 35
    IRQ_STUB 36
    IRQ_STUB 37
    IRQ_STUB 38
    IRQ_STUB 39
    IRQ_STUB 40
    IRQ_STUB 41
    IRQ_STUB 42
    IRQ_STUB 43
    IRQ_STUB 44
    IRQ_STUB 45
    IRQ_STUB 46
    IRQ_STUB 47
    "#
);
