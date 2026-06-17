use crate::fs::ramfs::{FsError, RamFs};

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
    Unsupported,
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

/// 教学用系统调用分发器。
///
/// 这个文件先定义“内核对外暴露哪些系统调用”的稳定接口。真正的 syscall/sysret
/// 汇编入口、用户态 libc 封装、文件描述符表会在下一层接入；现在先让 VFS 和进程
/// 模型有一个明确的调用边界，避免 shell/命令直接碰内核内部结构。
pub struct SyscallTable;

impl SyscallTable {
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

    pub fn exec(_path: &str, _argv: &[&str]) -> SysResult {
        // TODO: exec 需要 ELF/扁平二进制装载器，把新程序映射进当前进程地址空间，
        // 重建用户栈并替换 TrapFrame.rip/rsp。
        Err(SysError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fs_errors_map_to_sys_errors() {
        assert_eq!(SysError::from(FsError::NotFound), SysError::NoEntry);
        assert_eq!(SysError::from(FsError::AlreadyExists), SysError::Exists);
        assert_eq!(SysError::from(FsError::NotDirectory), SysError::NotDir);
    }
}
