const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const ICW1_ICW4:u8 = 0x01;		/* Indicates that ICW4 will be present */
const ICW1_SINGLE:u16 = 0x02;		/* Single (cascade) mode */
const ICW1_INTERVAL:u16 = 0x04;		/* Call address interval 4 (8) */
const ICW1_LEVEL:u16 = 0x08;		/* Level triggered (edge) mode */
const ICW1_INIT:u8 = 0x10;		/* Initialization - required! */

const ICW4_8086:u8 = 0x01;		/* 8086/88 (MCS-80/85) mode */
const ICW4_AUTO:u16 = 0x02;		/* Auto (normal) EOI */
const ICW4_BUF_SLAVE:u16 = 0x08;		/* Buffered mode/slave */
const ICW4_BUF_MASTER:u16 = 0x0C;		/* Buffered mode/master */
const ICW4_SFNM:u16 = 0x10;		/* Special fully nested (not) */

const CASCADE_IRQ:u8 = 2;

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

pub unsafe fn io_wait() {
    unsafe {
        outb(0x80, 0);
    }
}

pub fn pic_remap(offset1: u8, offset2: u8)
{
    unsafe {
        outb(PIC1_COMMAND, ICW1_INIT | ICW1_ICW4);   // starts the initialization sequence (in cascade mode)
        io_wait();
        outb(PIC2_COMMAND, ICW1_INIT | ICW1_ICW4);
        io_wait();
        outb(PIC1_DATA, offset1);                 // ICW2: Master PIC vector offset
        io_wait();
        outb(PIC2_DATA, offset2);                 // ICW2: Slave PIC vector offset
        io_wait();
        outb(PIC1_DATA, 1 << CASCADE_IRQ);        // ICW3: tell Master PIC that there is a slave PIC at IRQ2
        io_wait();
        outb(PIC2_DATA, 2);                       // ICW3: tell Slave PIC its cascade identity (0000 0010)
        io_wait();

        outb(PIC1_DATA, ICW4_8086);               // ICW4: have the PICs use 8086 mode (and not 8080 mode)
        io_wait();
        outb(PIC2_DATA, ICW4_8086);
        io_wait();

        // Unmask both PICs.
        outb(PIC1_DATA, 0);
        outb(PIC2_DATA, 0);
    }
}