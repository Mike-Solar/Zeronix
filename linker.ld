ENTRY(_start)
KERNEL_VMA = 0xFFFF800000000000;

SECTIONS {
    /* 启动代码：物理 1MB，32-bit 阶段必须能用 32-bit 绝对地址引用 */
    . = 0x100000;
    .boot : AT(0x100000) {
        KEEP(*(.multiboot_header))
        *(.text.boot)
    }

    /* 启动数据（页表、栈、GDT）放在低 1GB 内，32-bit 汇编可直接引用 */
    .bootdata 0x400000 : AT(0x400000) {
        _bootdata_start = LOADADDR(.bootdata);
        *(.bootdata)
    }

    /* 内核虚拟基址：高半区 + 当前物理偏移 */
    . = KERNEL_VMA + ALIGN(LOADADDR(.boot) + SIZEOF(.boot), 0x1000);
    _text_start = .;
    .text : AT(ADDR(.text) - KERNEL_VMA) {
        *(.text .text.*)
        *(.ltext .ltext.*)
    }
    . = ALIGN(0x1000);
    _text_end = .;
    _rodata_start = .;
    .rodata : AT(ADDR(.rodata) - KERNEL_VMA) {
        *(.rodata .rodata.*)
        *(.lrodata .lrodata.*)
    }
    . = ALIGN(0x1000);
    _rodata_end = .;
    _data_start = .;
    .data : AT(ADDR(.data) - KERNEL_VMA) {
        *(.data .data.*)
        *(.ldata .ldata.*)
    }
    . = ALIGN(0x1000);
    _data_end = .;
	_bss_start = .;
    .bss : AT(ADDR(.bss) - KERNEL_VMA) {
        *(.bss .bss.*)
        *(.lbss .lbss.*)
        *(COMMON)
    }
    . = ALIGN(0x1000);
    _bss_end = .;
	/* 内核物理起始地址（.boot 的加载地址） */
	_kernel_phys_start = LOADADDR(.boot);

	/* 内核物理结束地址（.bss 结束） */
	_kernel_phys_end = LOADADDR(.bss) + SIZEOF(.bss);

	/* bootdata 结束地址 */
	_bootdata_end = LOADADDR(.bootdata) + SIZEOF(.bootdata);


    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
    }
}
