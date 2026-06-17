use crate::trap::{outb};

const PIT_CHANNEL0: u16 = 0x40;
const PIT_COMMAND: u16 = 0x43;
const PIT_BASE_FREQ: u32 = 1_193_182;

pub fn init_100hz() {
    unsafe {
        init(PIT_BASE_FREQ / 100);
    }
}

pub unsafe fn init(divisor: u32) {
    let divisor = divisor as u16;

    // channel 0, access mode lobyte/hibyte, mode 3 square wave, binary
    unsafe {
        outb(PIT_COMMAND, 0b0011_0110);
        outb(PIT_CHANNEL0, (divisor & 0xff) as u8);
        outb(PIT_CHANNEL0, (divisor >> 8) as u8);
    }
}