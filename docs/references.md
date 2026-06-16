# 参考资料与工具

## 一、核心在线资源

### 1. OSDev Wiki
> https://wiki.osdev.org/

操作系统开发的百科全书，必读页面：

- [Bare Bones](https://wiki.osdev.org/Bare_Bones) — 最小内核入门
- [Multiboot2](https://wiki.osdev.org/Multiboot2) — GRUB2 启动协议
- [Setting Up Long Mode](https://wiki.osdev.org/Setting_Up_Long_Mode) — 进入 x86_64 long mode
- [Global Descriptor Table](https://wiki.osdev.org/Global_Descriptor_Table) / [GDT Tutorial](https://wiki.osdev.org/GDT_Tutorial)
- [Interrupt Descriptor Table](https://wiki.osdev.org/Interrupt_Descriptor_Table)
- [Paging](https://wiki.osdev.org/Paging) / [Page Tables](https://wiki.osdev.org/Page_Tables)
- [Detecting Memory (x86)](https://wiki.osdev.org/Detecting_Memory_(x86))
- [APIC](https://wiki.osdev.org/APIC) / [PIC](https://wiki.osdev.org/PIC)
- [PS/2 Keyboard](https://wiki.osdev.org/PS/2_Keyboard)
- [ELF](https://wiki.osdev.org/ELF) / [Linker Scripts](https://wiki.osdev.org/Linker_Scripts)

### 2. Writing an OS in Rust
> https://os.phil-opp.com/

Rust 内核开发最经典的教程，强烈推荐完整阅读。与本项目直接相关章节：

- A Freestanding Rust Binary
- A Minimal Rust Kernel
- VGA Text Mode / Framebuffer
- CPU Exceptions / Double Faults / Hardware Interrupts
- Paging Implementation
- Heap Allocation
- Allocator Designs
- Async/Await（可选）

GitHub 源码：https://github.com/phil-opp/blog_os

### 3. MIT 6.S081 / 6.1810: Operating System Engineering
> https://pdos.csail.mit.edu/6.1810/2025/index.html

操作系统课程设计的标杆。教学使用 xv6（RISC-V 版），但设计思想完全适用于 x86_64：

- 进程、调度、内存、文件系统、系统调用设计
- xv6 book（RISC-V 版）：https://pdos.csail.mit.edu/6.828/2021/xv6/book-riscv-rev2.pdf
- xv6 x86 版源码：https://github.com/mit-pdos/xv6-public
- xv6 RISC-V 版源码：https://github.com/mit-pdos/xv6-riscv

### 4. OSTEP（Operating Systems: Three Easy Pieces）
> http://pages.cs.wisc.edu/~remzi/OSTEP/

免费的操作系统理论教材，适合作为课程设计报告的理论基础：

- 虚拟化、并发、持久化三大主题
- 进程调度、内存管理、文件系统

---

## 二、Rust 生态与 Crate

| Crate | 用途 |
|---|---|
| [`bootloader`](https://crates.io/crates/bootloader) | 纯 Rust BIOS/UEFI bootloader（如不想用 GRUB2 可先用它跑通） |
| [`multiboot2`](https://crates.io/crates/multiboot2) | 解析 Multiboot2 信息结构 |
| [`multiboot2-header`](https://crates.io/crates/multiboot2-header) | 生成 Multiboot2 header |
| [`x86_64`](https://crates.io/crates/x86_64) | x86_64 寄存器、指令、页表、中断、Port I/O 抽象 |
| [`linked_list_allocator`](https://crates.io/crates/linked_list_allocator) | 内核堆分配器 |
| [`pic8259`](https://crates.io/crates/pic8259) | 8259 PIC 驱动 |
| [`uart_16550`](https://crates.io/crates/uart_16550) | COM 串口驱动 |
| [`pc-keyboard`](https://crates.io/crates/pc-keyboard) | PS/2 键盘扫描码解析 |
| [`spin`](https://crates.io/crates/spin) | `no_std` 自旋锁 |
| [`bitflags`](https://crates.io/crates/bitflags) | 类型安全的 flag 位域 |
| [`virtio-drivers`](https://crates.io/crates/virtio-drivers) | VirtIO 设备驱动（可选） |
| [`smoltcp`](https://crates.io/crates/smoltcp) | 纯 Rust TCP/IP 协议栈（可选） |
| [`uefi-rs`](https://crates.io/crates/uefi) | UEFI 运行时服务（如果内核需要直接调用 UEFI runtime，可选） |

---

## 三、规范与手册

### x86_64 架构手册
- **Intel SDM Volume 3A/3B/3C**: System Programming Guide
  - https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html
- **AMD64 Architecture Programmer’s Manual Volume 2**: System Programming
  - https://www.amd.com/en/support/tech-docs/amd64-architecture-programmers-manual-volumes-1-5

### 启动与 ABI
- **Multiboot2 Specification**: https://www.gnu.org/software/grub/manual/multiboot2/
- **System V AMD64 ABI**: https://wiki.osdev.org/System_V_ABI
  - 函数调用约定、syscall 参数传递规则
- **ELF64 Format**: https://refspecs.linuxfoundation.org/elf/elf.pdf
- **UEFI Specification**: https://uefi.org/specifications

### POSIX
- **POSIX.1-2008 / SUSv4**: https://pubs.opengroup.org/onlinepubs/9699919799/
- **Linux x86_64 syscall table**: https://blog.rchapman.org/posts/Linux_System_Call_Table_for_x86_64/

---

## 四、工具链

### 必需
- **Rust nightly** + `rust-src`
  ```bash
  rustup default nightly
  rustup component add rust-src llvm-tools-preview
  rustup target add x86_64-unknown-none  # 或自定义 target
  ```
- **QEMU** with UEFI support
  ```bash
  sudo apt install qemu-system-x86 qemu-utils ovmf
  ```
- **GRUB2 / xorriso**
  ```bash
  sudo apt install grub2-common grub-pc-bin xorriso mtools
  ```

### 推荐
- `cargo-binutils`：提供 `cargo objdump`、`cargo nm`、`cargo size`
  ```bash
  cargo install cargo-binutils
  ```
- `bootimage`：快速生成可启动磁盘镜像（如使用 bootloader crate）
- `gdb`：远程调试
  ```bash
  gdb -ex "target remote :1234" -ex "symbol-file target/x86_64-zeronix/debug/zeronix"
  ```

---

## 五、可参考的开源项目

| 项目 | 说明 | 链接 |
|---|---|---|
| blog_os | Writing an OS in Rust 官方源码 | https://github.com/phil-opp/blog_os |
| Redox OS | 最完整的 Rust 操作系统 | https://github.com/redox-os/redox |
| Theseus OS | 纯 Rust 安全 OS | https://github.com/theseus-os/Theseus |
| pebble | Rust microkernel | https://github.com/pebble-os/pebble |
| xv6-rust | xv6 的 Rust 重写 | https://github.com/KuangjuX/xv6-rust |
| Linux 0.01 | 早期 Linux 源码，适合参考系统调用与文件系统 | https://www.kernel.org/pub/linux/kernel/Historic/ |
| MINIX 3 | POSIX 兼容教学系统 | http://www.minix3.org/ |

---

## 六、学习路径建议

1. 先通读 **Writing an OS in Rust** 前 10 章，建立 Rust no_std 内核开发基础。
2. 同时阅读 **OSTEP** 对应章节，理解进程、内存、文件系统原理。
3. 在做文件系统前，精读 **xv6 book** 相关章节。
4. 遇到 x86_64 细节问题，查 **Intel SDM Vol 3** 和 **OSDev Wiki**。
5. 实现系统调用时，对照 **Linux x86_64 syscall table** 和 **POSIX.1 标准**。
