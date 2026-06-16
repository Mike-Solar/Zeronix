/*
 * Zeronix x86_64 Higher-Half Kernel Linker Script
 *
 * 该脚本将一个 x86_64 ELF64 内核链接到高半区虚拟地址空间：
 *   虚拟基址：0xffffffff80000000
 *   物理加载地址从 2 MiB (0x200000) 开始
 *
 * 使用 Multiboot2 协议，GRUB2 会读取 .multiboot2 段中的 header。
 */

/* 高半区偏移 */
KERNEL_VIRT_OFFSET = 0xffffffff80000000;

/* 物理加载起始地址 */
KERNEL_PHYS_START = 0x200000;

/* 指定输出格式为 64 位 ELF */
OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)

/* 入口符号，由启动汇编提供 */
ENTRY(_start)

SECTIONS
{
    /* 虚拟地址从 KERNEL_VIRT_OFFSET + KERNEL_PHYS_START 开始 */
    . = KERNEL_VIRT_OFFSET + KERNEL_PHYS_START;

    /*
     * .multiboot2 段必须位于内核镜像的前 32752 字节内，
     * 并且按 8 字节对齐。放在最开头最稳妥。
     */
    .multiboot2 : AT(ADDR(.multiboot2) - KERNEL_VIRT_OFFSET)
    {
        KEEP(*(.multiboot2))
    }

    /*
     * .text 段：可执行代码
     * 按页对齐，方便虚拟内存映射时设置 RX 权限
     */
    . = ALIGN(0x1000);
    __text_start = .;
    .text : AT(ADDR(.text) - KERNEL_VIRT_OFFSET)
    {
        *(.text._start)          /* 启动汇编入口优先放置 */
        *(.text .text.*)
    }
    . = ALIGN(0x1000);
    __text_end = .;

    /*
     * .rodata 段：只读数据
     * 按页对齐，方便设置 RO 权限
     */
    . = ALIGN(0x1000);
    __rodata_start = .;
    .rodata : AT(ADDR(.rodata) - KERNEL_VIRT_OFFSET)
    {
        *(.rodata .rodata.*)
    }
    . = ALIGN(0x1000);
    __rodata_end = .;

    /*
     * .data 段：已初始化全局/静态变量
     */
    . = ALIGN(0x1000);
    __data_start = .;
    .data : AT(ADDR(.data) - KERNEL_VIRT_OFFSET)
    {
        *(.data .data.*)
    }
    . = ALIGN(0x1000);
    __data_end = .;

    /*
     * .bss 段：未初始化全局/静态变量
     * 启动代码需要在进入 Rust 前将其清零
     */
    . = ALIGN(0x1000);
    __bss_start = .;
    .bss : AT(ADDR(.bss) - KERNEL_VIRT_OFFSET)
    {
        *(.bss .bss.*)
        *(COMMON)
    }
    . = ALIGN(0x1000);
    __bss_end = .;

    /*
     * 内核结束符号
     */
    . = ALIGN(0x1000);
    __kernel_end = .;

    /*
     * 物理地址相关符号，供启动汇编和内存管理使用
     */
    __kernel_physical_start = KERNEL_PHYS_START;
    __kernel_physical_end = __kernel_end - KERNEL_VIRT_OFFSET;

    /*
     * 丢弃不需要的段，避免链接进内核镜像
     */
    /DISCARD/ :
    {
        *(.comment)
        *(.note)
        *(.note.*)
        *(.eh_frame)
        *(.gnu.hash)
        *(.hash)
    }
}
