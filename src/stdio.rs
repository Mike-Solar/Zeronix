use core::arch::asm;
use core::fmt::{Arguments, Write};
static mut CURRENT_LEVEL: LogLevel = LogLevel::Info;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel{
    Debug = 0,
    Info = 1,
    Warning = 2,
    Error = 3,
}
const COM1_DATA: u16 = 0x3F8;      // 数据寄存器
const COM1_IER: u16 = 0x3F9;       // 中断使能寄存器
const COM1_FCR: u16 = 0x3FA;       // FIFO 控制寄存器
const COM1_LCR: u16 = 0x3FB;       // 线控制寄存器
const COM1_MCR: u16 = 0x3FC;       // modem 控制寄存器
const COM1_LSR: u16 = 0x3FD;       // 线状态寄存器
const LSR_DATA_READY: u8 = 0x01;   // 接收缓冲区有数据
const LSR_THRE: u8 = 0x20;         // 发送保持寄存器空位


#[macro_export]
macro_rules! printk {
    // 匹配: log!(LogLevel::Info, "format", args...)
    ($level:expr, $fmt:literal $(, $($arg:tt)*)?) => {
        $crate::stdio::_printk($level, format_args!($fmt $(, $($arg)*)?))
    };
}

pub fn _printk(level:LogLevel, arguments: Arguments){
    if !should_print(level){
        return;
    }
    let prefix = match level {
        LogLevel::Error => "[ERROR] ",
        LogLevel::Warning  => "[WARN]  ",
        LogLevel::Info  => "[INFO]  ",
        LogLevel::Debug => "[DEBUG] ",
    };

    let _ = Stdout.write_str(prefix);
    let _ = Stdout.write_fmt(arguments);
    let _ = Stdout.write_str("\n");

}

pub fn init_serial_interrupts() {
    unsafe {
        crate::trap::outb(COM1_IER, 0x00);

        // 115200 baud: divisor = 1.
        crate::trap::outb(COM1_LCR, 0x80);
        crate::trap::outb(COM1_DATA, 0x01);
        crate::trap::outb(COM1_IER, 0x00);

        // 8 data bits, no parity, 1 stop bit.
        crate::trap::outb(COM1_LCR, 0x03);

        // Enable FIFO, clear RX/TX queues, 14-byte threshold.
        crate::trap::outb(COM1_FCR, 0xC7);

        // DTR + RTS + OUT2. OUT2 is required for IRQ delivery on PC UARTs.
        crate::trap::outb(COM1_MCR, 0x0B);

        // Enable received-data-available interrupt.
        crate::trap::outb(COM1_IER, 0x01);

        // Drain any stale byte so enabling IRQ4 does not immediately report old input.
        while crate::trap::inb(COM1_LSR) & LSR_DATA_READY != 0 {
            let _ = crate::trap::inb(COM1_DATA);
        }
    }
}

pub fn handle_serial_rx_interrupt() {
    unsafe {
        while crate::trap::inb(COM1_LSR) & LSR_DATA_READY != 0 {
            let byte = crate::trap::inb(COM1_DATA);
            match byte {
                b'\r' => {
                    serial_putc(b'\r');
                    serial_putc(b'\n');
                }
                byte => serial_putc(byte),
            }
        }
    }
}

struct Stdout;
impl Write for Stdout {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        unsafe {
            serial_puts(s);
            Ok(())
        }
    }
}
fn should_print(log_level: LogLevel) -> bool{
    unsafe {
        if log_level as usize >= CURRENT_LEVEL as usize {
            return true;
        }
        false
    }
}

#[unsafe(no_mangle)]
pub unsafe fn serial_puts(text: &str){
    for &c in text.as_bytes() {
        if c == 0 {
            break;
        }
        unsafe { serial_putc(c) };
    }
}

#[unsafe(no_mangle)]
/// 等待发送缓冲区就绪，然后输出
pub unsafe fn serial_putc(c: u8) {
    // 等待发送缓冲区为空
    loop {
        let lsr: u8;
        unsafe {
            asm!(
            "in al, dx",           // 从 LSR 端口读取状态
            out("al") lsr,
            in("dx") COM1_LSR,
            options(nomem, nostack)
            );
        }
        if lsr & LSR_THRE != 0 {
            break;
        }
    }

    // 输出字符
    unsafe {
        asm!(
        "out dx, al",
        in("dx") COM1_DATA,
        in("al") c,
        options(nomem, nostack)
        );
    }
}
