pub mod idt;
pub mod gdt;
pub mod pic;
pub mod pit;


pub unsafe fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags),
        );
    }
}

pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            out("al") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags),
        );
    }
    value
}

pub unsafe fn io_wait() {
    unsafe {
        outb(0x80, 0);
    }
}

pub unsafe fn enable_interrupts() {
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
    }
}

pub unsafe fn halt() {
    unsafe {
        core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
    }
}
