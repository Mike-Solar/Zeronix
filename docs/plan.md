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

### 推荐仓库结构

```text
zeronix/
├── Cargo.toml
├── Cargo.lock
├── build.rs              # 构建脚本（可选）
├── Makefile              # 顶层构建入口
├── src/
│   ├── main.rs           # 内核入口
│   ├── lib.rs            # 模块组织（可选）
│   ├── arch/
│   │   └── x86_64/
│   │       ├── boot.rs       # multiboot2 解析、long mode 早期初始化
│   │       ├── gdt.rs        # 全局描述符表
│   │       ├── idt.rs        # 中断描述符表
│   │       ├── interrupts.rs # 异常/IRQ 处理
│   │       ├── io.rs         # port I/O、串口
│   │       ├── tss.rs        # 任务状态段
│   │       └── syscall.rs    # syscall/sysret 入口
│   ├── mm/
│   │   ├── pmm.rs            # 物理内存管理
│   │   ├── vmm.rs            # 页表 / 虚拟内存
│   │   ├── heap.rs           # 内核堆
│   │   └── addr.rs           # 物理/虚拟地址类型
│   ├── sched/
│   │   ├── process.rs        # PCB
│   │   ├── thread.rs         # 线程
│   │   ├── context.rs        # 上下文切换
│   │   ├── scheduler.rs      # 调度器
│   │   └── syscall_proc.rs   # fork/exec/wait 等
│   ├── syscall/
│   │   ├── table.rs          # 系统调用表
│   │   ├── dispatch.rs       # 分发逻辑
│   │   └── errno.rs          # 错误码
│   ├── fs/
│   │   ├── vfs.rs            # 虚拟文件系统
│   │   ├── ramdisk.rs        # initrd / tar
│   │   ├── devfs.rs          # /dev
│   │   ├── procfs.rs         # /proc
│   │   └── ext2.rs           # ext2 只读（可选）
│   ├── drivers/
│   │   ├── serial.rs         # COM1 串口
│   │   ├── vga.rs            # 帧缓冲输出
│   │   ├── keyboard.rs       # PS/2 键盘
│   │   ├── timer.rs          # APIC/PIT 定时器
│   │   └── pci.rs            # PCI 枚举（可选）
│   ├── user/
│   │   └── elf.rs            # ELF 加载
│   └── lib/
│       ├── spinlock.rs       # 自旋锁
│       ├── bitmap.rs         # 位图分配器辅助
│       └── list.rs           # 侵入式链表
├── boot/
│   └── grub.cfg          # GRUB2 配置
├── scripts/
│   ├── build.sh          # 构建镜像
│   └── run.sh            # 运行 QEMU
├── iso/                  # 生成的 ISO 内容
│   └── boot/
│       └── grub/
├── target/               # Cargo 输出
└── docs/                 # 本文档
```

---

## 2. 分阶段计划

### Phase 0：环境与工具链准备（1–2 周）

**目标**：能编译并运行一个 Rust 编写的最小 64 位内核。

- [ ] 安装 Rust nightly + `rust-src` 组件
- [ ] 配置 `cargo` 使用自定义 target：`x86_64-unknown-none` 或自定义 JSON target
- [ ] 安装 QEMU、OVMF（`qemu-ovmf` 或 edk2-ovmf）
- [ ] 安装 `grub-mkrescue`、`xorriso`、`mtools`
- [ ] 创建 `#![no_std]` / `#![no_main]` 的 `src/main.rs`
- [ ] 配置 `Cargo.toml` panic strategy：`panic = "abort"`
- [ ] 实现 `panic_handler`，能通过串口/帧缓冲输出 panic 信息
- [ ] 编写 linker script（`linker.ld`），将内核放在高半区（higher half）

**里程碑**：`make run` 在 QEMU/OVMF 中启动，能看到 `Hello from Zeronix!`

---

### Phase 1：UEFI + GRUB2 启动与 long mode（2 周）

**目标**：内核被 GRUB2 正确加载并进入 x86_64 long mode。

- [ ] 在 `src/main.rs` 中定义 Multiboot2 header（使用 `multiboot2` crate 或手写）
- [ ] 通过 Multiboot2 信息获取：
  - 内存映射（Memory Map）
  - 帧缓冲信息（Framebuffer tag）
  - RSDP（用于 ACPI，可选）
  - ELF sections（用于初始化页表和内存管理）
- [ ] 汇编入口设置初始页表、启用 PAE + PGE、进入 long mode
- [ ] 跳转到 Rust `kernel_main`
- [ ] 初始化早期栈
- [ ] 输出 Multiboot2 标签信息用于调试

**里程碑**：GRUB2 菜单加载内核，屏幕输出 multiboot2 内存布局。

---

### Phase 2：基础输出与异常处理（2 周）

**目标**：有稳定的调试输出手段，能捕获 CPU 异常。

- [ ] 初始化 COM1 串口（115200 8N1），作为 `serial_print!` 宏
- [ ] 初始化帧缓冲输出（线性帧缓冲 + 字体绘制，或简化为 VGA text mode fallback）
- [ ] 实现格式化输出宏 `print!` / `println!`
- [ ] 加载 GDT（64-bit code/data）
- [ ] 加载 TSS（用于 IST / 双重异常栈）
- [ ] 设置 IDT，处理：
  - Page Fault、General Protection Fault、Double Fault
  - Breakpoint（用于调试）
  - Invalid Opcode
- [ ] 处理 double fault 时不 triple fault

**里程碑**：触发 `int 3` 或 `panic!` 时能看到堆栈跟踪或寄存器信息。

---

### Phase 3：中断控制器与定时器（2 周）

**目标**：内核能响应硬件中断。

- [ ] 初始化 8259 PIC 或 Local APIC + IO APIC（推荐后者，但 PIC 更简单）
- [ ] 实现 PIT 或 APIC timer 作为系统节拍
- [ ] 处理键盘中断（PS/2 控制器）
- [ ] 实现中断嵌套与 `sti`/`cli` 管理
- [ ] 实现基本 `sleep` / 节拍计数

**里程碑**：键盘按键能在屏幕上回显，定时器稳定触发。

---

### Phase 4：内存管理（3–4 周）

**目标**：内核拥有完整的物理/虚拟内存管理。

- [ ] 解析 Multiboot2 内存映射，标记可用物理页
- [ ] 实现物理页分配器（bitmap allocator 或 buddy allocator）
- [ ] 实现页表操作：映射、取消映射、修改 flags
- [ ] 使用 recursive mapping 或 dedicated page table helper 访问页表
- [ ] 实现内核堆（使用 `linked_list_allocator` crate 或自写）
- [ ] 实现 `alloc` crate 的 `GlobalAlloc` trait
- [ ] 处理页错误，实现 copy-on-write 基础（可选）

**里程碑**：`Box::new`、`Vec`、`String` 可用；能动态分配和释放内存。

---

### Phase 5：进程、线程与调度（3–4 周）

**目标**：支持多任务抢占式调度。

- [ ] 设计 PCB / TCB 数据结构
- [ ] 实现上下文切换（保存/恢复 callee-saved 寄存器、RIP、RSP、CR3）
- [ ] 实现 round-robin 调度器
- [ ] 实现 idle task
- [ ] 实现 `fork` / `clone`（或简化版）
- [ ] 实现 `execve`：替换地址空间、加载 ELF
- [ ] 实现 `exit` / `waitpid`
- [ ] 进程树与僵尸进程处理

**里程碑**：多个用户进程并发运行，调度切换正常。

---

### Phase 6：系统调用与最小 POSIX（3–4 周）

**目标**：用户程序能通过 syscall 使用 POSIX 子集。

- [ ] 选择 syscall 机制：`syscall`/`sysret`（推荐）或 `int 0x80`
- [ ] 定义 syscall 调用号（参考 Linux x86_64 syscall table）
- [ ] 实现系统调用分发
- [ ] 逐步实现 [posix-subset.md](./posix-subset.md) 中的调用
- [ ] 实现 `errno` 与 C 库 errno 映射
- [ ] 使用 `x86_64` crate 处理 MSR（STAR/LSTAR/SFMASK）

**里程碑**：用 Rust 或 C 编写的用户程序能调用 `write`/`read`/`exit`/`fork`。

---

### Phase 7：文件系统（2–3 周）

**目标**：用户程序能读写文件。

- [ ] 设计 VFS：superblock、inode、dentry、file 结构
- [ ] 实现 initramfs：将用户程序嵌入内核镜像（tar 格式或自定义）
- [ ] 实现 `/dev/null`、`/dev/zero`、`/dev/random`、`/dev/tty`
- [ ] 实现 `/proc/self`、`/proc/meminfo` 等伪文件
- [ ] （可选）实现 ext2 只读驱动
- [ ] 实现 open/close/read/write/lseek/stat 的 VFS 路径

**里程碑**：shell 能 `cat /proc/meminfo` 和运行 initramfs 中的程序。

---

### Phase 8：用户态、libc 与 Shell（2–3 周）

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

### Phase 9：进阶与优化（可选，2–4 周）

根据时间和兴趣选择：

- [ ] **信号**：`sigaction`、`kill`、`SIGINT`、`SIGSEGV`、`SIGCHLD`
- [ ] **多核 SMP**：启动 AP、IPI、per-CPU 数据、调度队列
- [ ] **ACPI**：关机、重启
- [ ] **块设备**：VirtIO-blk 或 AHCI 驱动
- [ ] **网络**：VirtIO-net + smoltcp（Rust 网络栈）
- [ ] **图形**：简单 GUI / 窗口系统
- [ ] **真机验证**：刻录 USB 在物理机上启动

---

### Phase 10：测试、文档与答辩（2 周）

- [ ] 编写自动化测试脚本（QEMU + serial output 检查）
- [ ] 整理设计文档、开发日志、用户手册
- [ ] 录制演示视频或准备 live demo
- [ ] 性能/稳定性测试（例如连续运行 shell 10 分钟）
- [ ] 准备答辩 PPT

---

## 3. 风险与建议

| 风险 | 应对 |
|---|---|
| Rust no_std 生态不熟悉 | 先完成 Philipp Oppermann 的 Writing an OS in Rust 前几章 |
| UEFI + GRUB2 配置复杂 | 先用 Limine 或 bootloader crate 跑通，再迁移到 GRUB2 |
| long mode 分页调试困难 | 保留串口日志，实现页错误时打印 CR2、error code、RIP |
| 用户态 libc 工作量大 | 使用 newlib 并只实现底层 syscall stub |
| 文件系统复杂 | 先用 initramfs + VFS，ext2 作为加分项 |
| 多核 SMP 难度大 | 作为可选挑战，不建议纳入基础目标 |

## 4. 时间估算

| 阶段 | 周数 | 累计 |
|---|---|---|
| Phase 0 | 1–2 | 2 |
| Phase 1 | 2 | 4 |
| Phase 2 | 2 | 6 |
| Phase 3 | 2 | 8 |
| Phase 4 | 3–4 | 12 |
| Phase 5 | 3–4 | 16 |
| Phase 6 | 2–3 | 19 |
| Phase 7 | 2–3 | 22 |
| Phase 8-9 | 2–4 | 26 |
| Phase 10 | 2 | 28 |

> 课程设计通常 16 周，建议把 Phase 0–7 作为**必做**，Phase 8 之后作为**选做加分项**。
