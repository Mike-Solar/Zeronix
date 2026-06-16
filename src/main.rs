#![no_std]
#![no_main]

mod stdio;

use stdio::LogLevel;

#[unsafe(no_mangle)]
unsafe extern "C"  fn _start() -> ! {
    printk!(LogLevel::Info, "Welcome to Zeronix!");
    loop {}
}
