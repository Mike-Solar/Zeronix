# Zeronix 开发计划

> 从零开始实现一个 x86_64 UEFI + GRUB2 启动的 Rust 操作系统内核，覆盖最小 POSIX 子集。

---

## 1. 技术选型

| 层面 | 选择 | 说明 |
|---|---|---|
| 语言 | Rust | `#![no_std]` / `#![no_main]`，避免标准库依赖 |
| 架构 | x86_64 Long Mode | 现代 64 位平台，支持大页、NX 等特性 |
| 启动协议 | Multiboot2 | GRUB2 原生支持，可传递帧缓冲、内存映射、RSDP 等信息 |
| Bootloader | GRUB2 EFI | UEFI 固件 → GRUB2 `bootx64.efi` → 加载内核 ELF |
| 模拟器 | QEMU + OVMF | 使用 `OVMF_CODE.fd` 模拟 UEFI 环境 |
| 调试 | GDB + QEMU `-s -S` | 远程调试内核 |
| 构建 | Cargo + 自定义脚本 | `build.rs` / Makefile 构建镜像、创建 EFI 分区、运行 QEMU |

---

## 2. 分阶段计划

### Phase 0：环境与工具链准备

**目标**：能编译并运行一个 Rust 编写的最小 64 位内核。

- [x] 安装 Rust nightly + `rust-src` 组件
- [x] 配置 `cargo` 使用自定义 target：`x86_64-unknown-none` 或自定义 JSON target
- [x] 安装 QEMU、OVMF（`qemu-ovmf` 或 edk2-ovmf）
- [x] 安装 `grub-mkrescue`、`xorriso`、`mtools`
- [x] 创建 `#![no_std]` / `#![no_main]` 的 `src/main.rs`
- [x] 配置 `Cargo.toml` panic strategy：`panic = "abort"`
- [x] 实现 `panic_handler`，能通过串口/帧缓冲输出 panic 信息
- [x] 编写 linker script（`linker.ld`），将内核放在高半区（higher half）

**里程碑**：`make run` 在 QEMU/OVMF 中启动，能看到 `Hello from Zeronix!`

---

### Phase 1：UEFI + GRUB2 启动与 long mode

**目标**：内核被 GRUB2 正确加载并进入 x86_64 long mode。

- [x] 在 `src/main.rs` 中定义 Multiboot2 header（使用 `multiboot2` crate 或手写）
- [x] 通过 Multiboot2 信息获取：
  - 内存映射（Memory Map）
  - 帧缓冲信息（Framebuffer tag）
  - RSDP（用于 ACPI，可选）
  - ELF sections（用于初始化页表和内存管理）
- [x] 汇编入口设置初始页表、启用 PAE + PGE、进入 long mode
- [x] 跳转到 Rust `kernel_main`
- [x] 初始化早期栈
- [ ] 输出 Multiboot2 标签信息用于调试

**里程碑**：GRUB2 菜单加载内核，屏幕输出 multiboot2 内存布局。

---

### Phase 2：基础输出与异常处理

**目标**：有稳定的调试输出手段，能捕获 CPU 异常。

- [x] 初始化 COM1 串口（115200 8N1），作为 `printk!` 宏
- [ ] 初始化帧缓冲输出（线性帧缓冲 + 字体绘制，或简化为 VGA text mode fallback）
- [ ] 实现格式化输出宏 `printk!`
- [x] 加载 GDT（64-bit code/data）
- [x] 加载 TSS（用于 IST / 双重异常栈）
- [x] 设置 IDT，处理：
  - Page Fault、General Protection Fault、Double Fault
  - Breakpoint（用于调试）
  - Invalid Opcode
- [x] 处理 double fault 时不 triple fault

**里程碑**：触发 `int 3` 或 `panic!` 时能看到堆栈跟踪或寄存器信息。

---

### Phase 3：中断控制器与定时器

**目标**：内核能响应硬件中断。

- [x] 初始化 8259 PIC 或 Local APIC + IO APIC（推荐后者，但 PIC 更简单）
- [x] 实现 PIT 或 APIC timer 作为系统节拍
- [x] 处理键盘中断（PS/2 控制器）
- [x] 实现中断嵌套与 `sti`/`cli` 管理
- [x] 实现基本 `sleep` / 节拍计数

**里程碑**：键盘按键能在屏幕上回显，定时器稳定触发。

---

### Phase 4：内存管理

**目标**：内核拥有完整的物理/虚拟内存管理。

- [x] 解析 Multiboot2 内存映射，标记可用物理页
- [x] 实现物理页分配器（bitmap allocator 或 buddy allocator）
- [x] 实现页表操作：映射、取消映射、修改 flags
- [x] 实现内核堆（使用 `linked_list_allocator` crate 或自写）
- [x] 实现 `alloc` crate 的 `GlobalAlloc` trait
- [ ] 处理页错误，实现 copy-on-write 基础（可选）

**里程碑**：`Box::new`、`Vec`、`String` 可用；能动态分配和释放内存。

---

### Phase 5：进程、线程与调度

**目标**：支持多任务抢占式调度。

- [x] 设计 PCB / TCB 数据结构
- [x] 实现上下文切换（保存/恢复 callee-saved 寄存器、RIP、RSP、CR3）
- [ ] 实现 round-robin 调度器
- [x] 实现 idle task
- [x] 实现 `fork` / `clone`（或简化版）
- [x] 实现 `execve`：替换地址空间、加载 ELF
- [x] 实现 `exit` / `waitpid`
- [ ] 进程树与僵尸进程处理

**里程碑**：多个用户进程并发运行，调度切换正常。

---

### Phase 6：系统调用与最小 POSIX）

**目标**：用户程序能通过 syscall 使用 POSIX 子集。

- [x] syscall 机制：`syscall`/`sysret`
- [ ] 定义 syscall 调用号（参考 Linux x86_64 syscall table）
- [ ] 实现系统调用分发
- [ ] 逐步实现 [posix-subset.md](./posix-subset.md) 中的调用
- [ ] 实现 `errno` 与 C 库 errno 映射
- [ ] 使用 `x86_64` crate 处理 MSR（STAR/LSTAR/SFMASK）

**里程碑**：用 Rust 或 C 编写的用户程序能调用 `write`/`read`/`exit`/`fork`。

---

### Phase 7：文件系统

**目标**：用户程序能读写文件。

- [ ] 设计 VFS：superblock、inode、dentry、file 结构
- [ ] 实现 initramfs：将用户程序嵌入内核镜像（tar 格式或自定义）
- [ ] 实现 `/dev/null`、`/dev/zero`、`/dev/random`、`/dev/tty`
- [ ] 实现 `/proc/self`、`/proc/meminfo` 等伪文件
- [ ] （可选）实现 ext2 只读驱动
- [ ] 实现 open/close/read/write/lseek/stat 的 VFS 路径

**里程碑**：shell 能 `cat /proc/meminfo` 和运行 initramfs 中的程序。

---

### Phase 8：用户态、libc 与 Shell

**目标**：能运行一个简易 Unix shell 和常用工具。

- [ ] 加载并运行静态链接 ELF 用户程序
- [ ] 设置用户栈：`argc`、`argv`、`envp`
- [ ] 移植或自写最小 libc（推荐 newlib + 自定义 syscall stub）
- [ ] 编写简单 shell，支持：
  - 命令解析
  - `cd` / `pwd`
  - 管道 `|`
  - 输入/输出重定向 `<` / `>`
  - 前台进程 `waitpid`
- [ ] 编写测试用用户程序：`echo`、`cat`、`ls`、`ps`、`mkdir` 等

**里程碑**：在 QEMU 中登录后进入 shell，能执行命令流水线。

---

### Phase 9：进阶与优化

根据时间和兴趣选择：

- [ ] **信号**：`sigaction`、`kill`、`SIGINT`、`SIGSEGV`、`SIGCHLD`
- [ ] **多核 SMP**：启动 AP、IPI、per-CPU 数据、调度队列
- [ ] **ACPI**：关机、重启
- [ ] **块设备**：VirtIO-blk 或 AHCI 驱动
- [ ] **网络**：VirtIO-net + smoltcp（Rust 网络栈）
- [ ] **图形**：简单 GUI / 窗口系统
- [ ] **真机验证**：刻录 USB 在物理机上启动
