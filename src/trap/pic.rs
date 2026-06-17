const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;
const PIC_EOI: u8 = 0x20;

const ICW1_ICW4:u8 = 0x01;		/* Indicates that ICW4 will be present */
const ICW1_INIT:u8 = 0x10;		/* Initialization - required! */

const ICW4_8086:u8 = 0x01;		/* 8086/88 (MCS-80/85) mode */

const CASCADE_IRQ:u8 = 2;

use crate::trap::{io_wait, outb};

pub const IRQ_BASE: u8 = 32;
pub const IRQ_COUNT: u8 = 16;

pub fn init()
{
    pic_remap(IRQ_BASE, IRQ_BASE + 8);
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

        // Mask everything first. Individual IRQ lines can be unmasked later.
        outb(PIC1_DATA, 0xff);
        outb(PIC2_DATA, 0xff);
    }
}

pub fn unmask_irq(irq: u8) {
    if irq >= IRQ_COUNT {
        return;
    }

    let (port, bit) = if irq < 8 {
        (PIC1_DATA, irq)
    } else {
        (PIC2_DATA, irq - 8)
    };

    unsafe {
        let mask = crate::trap::inb(port) & !(1 << bit);
        outb(port, mask);

        if irq >= 8 {
            let master_mask = crate::trap::inb(PIC1_DATA) & !(1 << CASCADE_IRQ);
            outb(PIC1_DATA, master_mask);
        }
    }
}

pub fn end_of_interrupt(irq: u8) {
    unsafe {
        if irq >= 8 {
            outb(PIC2_COMMAND, PIC_EOI);
        }
        outb(PIC1_COMMAND, PIC_EOI);
    }
}
