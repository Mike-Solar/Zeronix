.code32

# === Multiboot2 Header ===
.section .multiboot_header, "a"
.align 8
header_start:
    .long 0xe85250d6
    .long 0
    .long header_end - header_start
    .long -(0xe85250d6 + 0 + (header_end - header_start))
    .word 0, 0
    .long 8
header_end:

# === 32-bit Protected Mode Entry ===
.section .text.boot, "ax"
.global _start
.type _start, @function

.set KERNEL_VMA, 0xFFFF800000000000

_start:
    # 设置临时栈（物理低地址，.bootdata 中）
    movl $stack_top, %esp

    # 保存 GRUB2 参数（物理地址）
    movl %ebx, mbi_ptr
    movl %eax, magic_num

    # --- CPUID + Long Mode 检查（省略，与之前相同）---

    # === 建立双映射页表 ===
    # 清零 PML4 + PDPT + PD (12KB)
    movl $pml4, %edi
    xorl %eax, %eax
    movl $0x3000, %ecx
    rep stosb

    # PML4[0] -> PDPT  (Identity Map，低地址访问用)
    movl $pml4, %edi
    movl $pdpt, %eax
    orl $0x03, %eax       # Present + Writable
    movl %eax, (%edi)

    # PML4[256] -> PDPT  (Higher Half: 0xFFFF800000000000)
    # 256 * 8 = 2048 字节偏移
    movl $pml4, %edi
    addl $2048, %edi
    movl $pdpt, %eax
    orl $0x03, %eax
    movl %eax, (%edi)

    # PDPT[0] -> PD
    movl $pdpt, %edi
    movl $pd, %eax
    orl $0x03, %eax
    movl %eax, (%edi)

    # PD[0..3] -> 前 8MB 的 2MB Huge Page
    # 这同时服务于 Identity 和 Higher Half：
    #   VA 0x00000000..0x00800000 -> PA 0x00000000..0x00800000
    #   VA KERNEL_VMA + phys -> PA phys
    # 需要覆盖内核物理镜像、BSS metadata 和 .bootdata 栈。
    movl $pd, %edi
    movl $0x83, %eax      # Present + Writable + Huge
    movl %eax, 0(%edi)
    addl $0x200000, %eax
    movl %eax, 8(%edi)
    addl $0x200000, %eax
    movl %eax, 16(%edi)
    addl $0x200000, %eax
    movl %eax, 24(%edi)

    # 加载页表到 CR3
    movl $pml4, %eax
    movl %eax, %cr3

    # 加载 64-bit GDT。这里仍在 32-bit protected mode，使用 6-byte GDTR 描述符。
    lgdt gdt64_ptr

    # 启用 PAE + PGE
    movl %cr4, %eax
    orl $0xA0, %eax
    movl %eax, %cr4

    # 启用 Long Mode (EFER.LME) + NXE
    movl $0xC0000080, %ecx
    rdmsr
    orl $0x900, %eax
    wrmsr

    # 启用分页（进入兼容模式）
    movl %cr0, %eax
    orl $0x80000000, %eax
    movl %eax, %cr0

    # 重新加载 CS 进入 64-bit code segment。
    # 用 far return 避开 0xEA immediate far jump 在调试器中的 BAD 解码问题。
    pushl $0x08
    pushl $long_mode_entry
    lret

no_cpuid:
no_longmode:
    cli
    hlt
    jmp no_longmode

# === 64-bit 入口（仍在低地址执行）===
.section .text.boot, "ax"
.code64
long_mode_entry:
    # 加载数据段选择子
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %fs
    movw %ax, %gs
    movw %ax, %ss

    # 绝对跳转到高地址的代码
    # long_mode_high 的完整 64-bit 地址由链接器填入
    movabs $long_mode_high, %rax
    jmp *%rax

# === 64-bit 高地址执行 ===
.section .text, "ax"
.code64
long_mode_high:
    # 现在 RIP 是高地址！设置栈（仍在低地址，Identity Map 有效）
    movabs $stack_top, %rsp

    # 恢复 GRUB 参数到 SystemV ABI 寄存器
    # mbi_ptr 是物理低地址，Identity Map 下直接可用
    movl mbi_ptr, %edi        # RDI = MBI 指针 (u64，零扩展)
    movl magic_num, %esi      # RSI = Magic (u32，零扩展)

    # 进入 Rust！kernel_main 链接在高地址，RIP-relative 调用有效
    call kernel_main

    cli
1:  hlt
    jmp 1b

# === 启动数据（低地址，32-bit 安全访问）===
.section .bootdata, "aw"
.align 4096
pml4:
    .space 4096
pdpt:
    .space 4096
pd:
    .space 4096

.align 16
stack_bottom:
    .space 16384
stack_top:

mbi_ptr:
    .space 4
magic_num:
    .space 4

# === GDT ===
.section .bootdata, "aw"
.align 8
gdt64:
    .quad 0x0000000000000000      # 0x00: Null
    .quad 0x00af9a000000ffff      # 0x08: 64-bit Code (Execute/Read)
    .quad 0x00cf92000000ffff      # 0x10: Data (Read/Write)
gdt64_ptr:
    .word (gdt64_ptr - gdt64 - 1)
    .long gdt64
