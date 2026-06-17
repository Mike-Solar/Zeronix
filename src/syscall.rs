use alloc::vec::Vec;
use core::arch::{asm, global_asm};

use crate::fs::ramfs::{FsError, RamFs};
use crate::lock::spin_lock::SpinLock;

pub mod fd;

pub type SysResult = Result<usize, SysError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysError {
    NoSuchSyscall,
    InvalidArgument,
    NoEntry,
    Exists,
    NotDir,
    IsDir,
    NotEmpty,
    BadFd,
    Permission,
    Unsupported,
}

impl SysError {
    pub fn errno(self) -> isize {
        match self {
            SysError::NoSuchSyscall => -38,   // ENOSYS
            SysError::InvalidArgument => -22, // EINVAL
            SysError::NoEntry => -2,          // ENOENT
            SysError::Exists => -17,          // EEXIST
            SysError::NotDir => -20,          // ENOTDIR
            SysError::IsDir => -21,           // EISDIR
            SysError::NotEmpty => -39,        // ENOTEMPTY
            SysError::BadFd => -9,            // EBADF
            SysError::Permission => -13,      // EACCES
            SysError::Unsupported => -95,     // EOPNOTSUPP
        }
    }
}

impl From<FsError> for SysError {
    fn from(value: FsError) -> Self {
        match value {
            FsError::InvalidPath => SysError::InvalidArgument,
            FsError::NotFound => SysError::NoEntry,
            FsError::AlreadyExists => SysError::Exists,
            FsError::NotDirectory => SysError::NotDir,
            FsError::IsDirectory => SysError::IsDir,
            FsError::DirectoryNotEmpty => SysError::NotEmpty,
        }
    }
}

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallNumber {
    Fork = 1,
    Exec = 2,
    Exit = 3,
    WaitPid = 4,
    Read = 5,
    Write = 6,
    Open = 7,
    Close = 8,
    Dup2 = 9,
    Unlink = 10,
    Rename = 11,
    Mkdir = 12,
}

const IA32_EFER: u32 = 0xC000_0080;
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_FMASK: u32 = 0xC000_0084;
const EFER_SCE: u64 = 1;
const RFLAGS_IF: u64 = 1 << 9;
const KERNEL_CODE_SELECTOR: u16 = 0x08;
const USER_DATA_SELECTOR: u16 = 0x18 | 3;
const MAX_USER_STRING: usize = 256;
const SYSCALL_STACK_SIZE: usize = 16 * 1024;

#[repr(align(16))]
#[allow(dead_code)]
struct SyscallStack([u8; SYSCALL_STACK_SIZE]);

#[unsafe(no_mangle)]
static mut __SYSCALL_STACK: SyscallStack = SyscallStack([0; SYSCALL_STACK_SIZE]);

#[unsafe(no_mangle)]
static mut __SYSCALL_USER_RSP: u64 = 0;

static KERNEL_FS: SpinLock<Option<RamFs>> = SpinLock::new(None);
static KERNEL_FD_TABLE: SpinLock<Option<fd::FileDescriptorTable>> = SpinLock::new(None);
static CONSOLE_WRITE: SpinLock<Option<fn(&[u8])>> = SpinLock::new(None);
static EXEC_PROGRAM: SpinLock<Option<fn(&[u8]) -> SysResult>> = SpinLock::new(None);
static EXIT_CURRENT: SpinLock<Option<fn(usize) -> !>> = SpinLock::new(None);

unsafe extern "C" {
    fn __syscall_entry();
}

/// 初始化 x86_64 `syscall/sysret` 入口。
///
/// `syscall` 和普通中断不同：CPU 不会查 IDT，也不会自动切换到 TSS.rsp0。
/// 它只做几件固定动作：
/// - 把用户 RIP 保存到 RCX；
/// - 把用户 RFLAGS 保存到 R11；
/// - 从 LSTAR 读取内核入口 RIP；
/// - 根据 STAR 切换 CS/SS；
/// - 根据 FMASK 清除指定 RFLAGS 位。
///
/// 因为不会自动换栈，本实现先使用一个单核全局 syscall 栈，并用 FMASK 清 IF，
/// 避免 syscall handler 还没保存现场时被时钟中断打断。后续多进程/多核版本应改成
/// 每 CPU 或每进程内核栈，并在入口处通过 per-cpu 数据选择栈。
pub fn init() {
    unsafe {
        let efer = rdmsr(IA32_EFER) | EFER_SCE;
        wrmsr(IA32_EFER, efer);

        let user_star = (USER_DATA_SELECTOR as u64).wrapping_sub(8);
        let star = ((user_star) << 48) | ((KERNEL_CODE_SELECTOR as u64) << 32);
        wrmsr(IA32_STAR, star);
        wrmsr(IA32_LSTAR, __syscall_entry as *const () as usize as u64);
        wrmsr(IA32_FMASK, RFLAGS_IF);
    }
}

pub fn init_runtime_fs() {
    let mut fs = RamFs::new();
    seed_runtime_fs(&mut fs);
    KERNEL_FS.lock().replace(fs);
    KERNEL_FD_TABLE.lock().replace(fd::FileDescriptorTable::new());
}

pub fn set_console_write(writer: fn(&[u8])) {
    CONSOLE_WRITE.lock().replace(writer);
}

pub fn set_process_hooks(exec_program: fn(&[u8]) -> SysResult, exit_current: fn(usize) -> !) {
    EXEC_PROGRAM.lock().replace(exec_program);
    EXIT_CURRENT.lock().replace(exit_current);
}

pub fn with_runtime_fs<T>(f: impl FnOnce(&mut RamFs) -> T) -> Option<T> {
    let mut guard = KERNEL_FS.lock();
    let fs = guard.as_mut()?;
    Some(f(fs))
}

fn seed_runtime_fs(fs: &mut RamFs) {
    // /bin 里的文件使用 ELF64 格式。当前 shell 先解析 ELF 并按程序 ID 分发，
    // 后续 exec 可以复用同一份解析结果去映射 LOAD 段。
    crate::user::elf::install_programs(fs);
    let _ = fs.write("/home/readme.txt", b"Welcome to Zeronix shell.\n");
}

unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags),
        );
    }
    ((high as u64) << 32) | low as u64
}

unsafe fn wrmsr(msr: u32, value: u64) {
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nomem, nostack, preserves_flags),
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __syscall_dispatch(
    number: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
) -> isize {
    match dispatch(number, arg0, arg1, arg2, arg3, arg4) {
        Ok(value) => value as isize,
        Err(err) => err.errno(),
    }
}

fn dispatch(
    number: usize,
    arg0: usize,
    arg1: usize,
    arg2: usize,
    _arg3: usize,
    _arg4: usize,
) -> SysResult {
    match number {
        x if x == SyscallNumber::Read as usize => syscall_read(arg0, arg1, arg2),
        x if x == SyscallNumber::Write as usize => syscall_write(arg0, arg1, arg2),
        x if x == SyscallNumber::Open as usize => syscall_open(arg0, arg1),
        x if x == SyscallNumber::Close as usize => {
            with_fd_table(|table| SyscallTable::close(table, arg0))
        }
        x if x == SyscallNumber::Dup2 as usize => {
            with_fd_table(|table| SyscallTable::dup2(table, arg0, arg1))
        }
        x if x == SyscallNumber::Unlink as usize => {
            let path = unsafe { copy_user_cstr(arg0 as *const u8)? };
            with_fs(|fs| SyscallTable::unlink(fs, &path))
        }
        x if x == SyscallNumber::Rename as usize => {
            let old = unsafe { copy_user_cstr(arg0 as *const u8)? };
            let new = unsafe { copy_user_cstr(arg1 as *const u8)? };
            with_fs(|fs| SyscallTable::rename(fs, &old, &new))
        }
        x if x == SyscallNumber::Mkdir as usize => {
            let path = unsafe { copy_user_cstr(arg0 as *const u8)? };
            with_fs(|fs| SyscallTable::mkdir(fs, &path))
        }
        x if x == SyscallNumber::Fork as usize => SyscallTable::fork(),
        x if x == SyscallNumber::Exec as usize => {
            let path = unsafe { copy_user_cstr(arg0 as *const u8)? };
            with_fs(|fs| SyscallTable::exec(fs, &path, &[]))
        }
        x if x == SyscallNumber::Exit as usize => {
            if let Some(exit_current) = *EXIT_CURRENT.lock() {
                exit_current(arg0)
            }
            Err(SysError::Unsupported)
        }
        _ => Err(SysError::NoSuchSyscall),
    }
}

fn syscall_open(path_ptr: usize, raw_flags: usize) -> SysResult {
    let path = unsafe { copy_user_cstr(path_ptr as *const u8)? };
    let flags = decode_open_flags(raw_flags);
    with_fs(|fs| {
        with_fd_table(|table| SyscallTable::open(fs, table, &path, flags))
    })
}

fn syscall_read(fd: usize, buf_ptr: usize, len: usize) -> SysResult {
    if buf_ptr == 0 {
        return Err(SysError::InvalidArgument);
    }
    let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, len) };
    let fs_guard = KERNEL_FS.lock();
    let fs = fs_guard.as_ref().ok_or(SysError::Unsupported)?;
    with_fd_table(|table| SyscallTable::read(fs, table, fd, buf))
}

fn syscall_write(fd: usize, buf_ptr: usize, len: usize) -> SysResult {
    if buf_ptr == 0 {
        return Err(SysError::InvalidArgument);
    }
    let buf = unsafe { core::slice::from_raw_parts(buf_ptr as *const u8, len) };
    let written = with_fs(|fs| {
        with_fd_table(|table| SyscallTable::write(fs, table, fd, buf))
    })?;
    mirror_console_write(fd, &buf[..written]);
    Ok(written)
}

fn with_fs(f: impl FnOnce(&mut RamFs) -> SysResult) -> SysResult {
    let mut guard = KERNEL_FS.lock();
    let fs = guard.as_mut().ok_or(SysError::Unsupported)?;
    f(fs)
}

fn with_fd_table(f: impl FnOnce(&mut fd::FileDescriptorTable) -> SysResult) -> SysResult {
    let mut guard = KERNEL_FD_TABLE.lock();
    let table = guard.as_mut().ok_or(SysError::Unsupported)?;
    f(table)
}

fn mirror_console_write(fd: usize, buf: &[u8]) {
    if fd != 1 && fd != 2 {
        return;
    }

    // stdout/stderr 的主要语义仍然是写入 fd 表。这里额外支持一个由内核启动代码
    // 注册的 console hook，方便直接在串口上观察用户态 `write(1/2, ...)` 是否
    // 真的经过了 syscall 入口。hook 放在 syscall 模块外部注册，是为了让本文件
    // 同时能被 lib 测试和内核二进制复用。
    if let Some(writer) = *CONSOLE_WRITE.lock() {
        writer(buf);
    }
}

fn decode_open_flags(raw: usize) -> fd::OpenFlags {
    let mut flags = fd::OpenFlags::empty();
    if raw & 1 != 0 {
        flags |= fd::OpenFlags::READ;
    }
    if raw & 2 != 0 {
        flags |= fd::OpenFlags::WRITE;
    }
    if raw & 4 != 0 {
        flags |= fd::OpenFlags::CREATE;
    }
    if raw & 8 != 0 {
        flags |= fd::OpenFlags::TRUNC;
    }
    if raw & 16 != 0 {
        flags |= fd::OpenFlags::APPEND;
    }
    flags
}

unsafe fn copy_user_cstr(ptr: *const u8) -> Result<alloc::string::String, SysError> {
    if ptr.is_null() {
        return Err(SysError::InvalidArgument);
    }

    let mut bytes = Vec::new();
    let mut offset = 0;
    loop {
        if offset >= MAX_USER_STRING {
            return Err(SysError::InvalidArgument);
        }
        let byte = unsafe { *ptr.add(offset) };
        if byte == 0 {
            break;
        }
        bytes.push(byte);
        offset += 1;
    }

    alloc::string::String::from_utf8(bytes).map_err(|_| SysError::InvalidArgument)
}

/// 系统调用分发器。
///
/// 这个文件先定义“内核对外暴露哪些系统调用”的稳定接口。
pub struct SyscallTable;

impl SyscallTable {
    pub fn open(
        fs: &mut RamFs,
        table: &mut fd::FileDescriptorTable,
        path: &str,
        flags: fd::OpenFlags,
    ) -> SysResult {
        table.open(fs, path, flags).map_err(Into::into)
    }

    pub fn read(
        fs: &RamFs,
        table: &mut fd::FileDescriptorTable,
        fd: usize,
        buf: &mut [u8],
    ) -> SysResult {
        table.read(fs, fd, buf).map_err(Into::into)
    }

    pub fn write(
        fs: &mut RamFs,
        table: &mut fd::FileDescriptorTable,
        fd: usize,
        buf: &[u8],
    ) -> SysResult {
        table.write(fs, fd, buf).map_err(Into::into)
    }

    pub fn close(table: &mut fd::FileDescriptorTable, fd: usize) -> SysResult {
        table.close(fd).map_err(Into::into)
    }

    pub fn dup2(table: &mut fd::FileDescriptorTable, old_fd: usize, new_fd: usize) -> SysResult {
        table.dup2(old_fd, new_fd).map_err(Into::into)
    }

    pub fn unlink(fs: &mut RamFs, path: &str) -> SysResult {
        fs.remove(path)?;
        Ok(0)
    }

    pub fn rename(fs: &mut RamFs, old: &str, new: &str) -> SysResult {
        fs.rename(old, new)?;
        Ok(0)
    }

    pub fn mkdir(fs: &mut RamFs, path: &str) -> SysResult {
        fs.mkdir(path)?;
        Ok(0)
    }

    pub fn fork() -> SysResult {
        // TODO: 当前调度器已有独立页表，但还没有复制用户地址空间和文件描述符表。
        // fork 需要复制父进程用户页、复制打开文件表，并让父/子在 TrapFrame 中拿到
        // 不同返回值。接口先固定在这里，后续直接补实现。
        Err(SysError::Unsupported)
    }

    pub fn exec(fs: &mut RamFs, path: &str, _argv: &[&str]) -> SysResult {
        let image = fs.read(path)?;
        let exec_program = *EXEC_PROGRAM.lock();
        exec_program.ok_or(SysError::Unsupported)?(&image)
    }
}

global_asm!(
    r#"
    .global __syscall_entry
    __syscall_entry:
        mov [rip + __SYSCALL_USER_RSP], rsp
        lea rsp, [rip + __SYSCALL_STACK + 16384]

        push r11
        push rcx
        push rbx
        push rbp
        push r12
        push r13
        push r14
        push r15

        mov rbx, rdi
        mov rdi, rax
        mov rax, rsi
        mov rsi, rbx
        mov rbx, rdx
        mov rdx, rax
        mov rcx, rbx
        mov r9, r8
        mov r8, r10

        call __syscall_dispatch

        pop r15
        pop r14
        pop r13
        pop r12
        pop rbp
        pop rbx
        pop rcx
        pop r11
        mov rsp, [rip + __SYSCALL_USER_RSP]
        sysretq
    "#
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syscall::fd::{FileDescriptorTable, OpenFlags};

    #[test]
    fn fs_errors_map_to_sys_errors() {
        assert_eq!(SysError::from(FsError::NotFound), SysError::NoEntry);
        assert_eq!(SysError::from(FsError::AlreadyExists), SysError::Exists);
        assert_eq!(SysError::from(FsError::NotDirectory), SysError::NotDir);
    }

    #[test]
    fn table_syscalls_read_and_write_files() {
        let mut fs = RamFs::new();
        let mut table = FileDescriptorTable::new();

        let fd = SyscallTable::open(
            &mut fs,
            &mut table,
            "/home/msg",
            OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::TRUNC,
        )
        .unwrap();
        assert_eq!(SyscallTable::write(&mut fs, &mut table, fd, b"hello").unwrap(), 5);
        SyscallTable::close(&mut table, fd).unwrap();

        let fd = SyscallTable::open(&mut fs, &mut table, "/home/msg", OpenFlags::READ).unwrap();
        let mut buf = [0u8; 8];
        let n = SyscallTable::read(&fs, &mut table, fd, &mut buf).unwrap();

        assert_eq!(&buf[..n], b"hello");
    }
}
