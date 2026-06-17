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
const COM1_LSR: u16 = 0x3FD;       // 线状态寄存器
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
