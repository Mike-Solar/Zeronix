# Re: 从零开始的写操作系统生活
本文档目录包含从零开始实现 x86_64 UEFI + GRUB2 启动的 Rust 操作系统内核的完整计划与参考资料。

## 为什么叫Zeronix

因为是从零开始写的嘛……同时也使用了操作系统通用的-nix后缀。

## 文档结构

| 文件 | 内容 |
|---|---|
| [plan.md](docs/plan.md) | 总体路线图与分阶段实施计划 |
| [references.md](docs/references.md) | 推荐参考资料、工具与 crate |
| [posix-subset.md](docs/posix-subset.md) | 课程设计要实现的最小 POSIX 子集 |

## 项目定位

- **目标平台**：x86_64（long mode）
- **启动方式**：UEFI + GRUB2（Multiboot2 协议）
- **开发语言**：Rust（`#![no_std]` / `#![no_main]`）
- **内核架构**：单体内核（Monolithic Kernel）
- **POSIX 目标**：实现一个最小 POSIX.1 子集，支持用户态 shell 与基础工具
- **运行环境**：QEMU（OVMF UEFI 固件），可扩展到真机

## 快速入口

如果你是第一次阅读，建议按以下顺序：

1. 阅读 [plan.md](docs/plan.md) 了解整体路线
2. 阅读 [posix-subset.md](docs/posix-subset.md) 明确要实现的功能范围
3. 阅读 [references.md](docs/references.md) 准备学习资料与工具链
