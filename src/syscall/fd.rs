use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::ops::{BitOr, BitOrAssign};

use crate::fs::ramfs::{FsError, RamFs};
use crate::syscall::SysError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenFlags(u8);

impl OpenFlags {
    pub const READ: Self = Self(1 << 0);
    pub const WRITE: Self = Self(1 << 1);
    pub const CREATE: Self = Self(1 << 2);
    pub const TRUNC: Self = Self(1 << 3);
    pub const APPEND: Self = Self(1 << 4);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for OpenFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for OpenFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FdError {
    BadFd,
    Permission,
    Fs(FsError),
}

impl From<FsError> for FdError {
    fn from(value: FsError) -> Self {
        Self::Fs(value)
    }
}

impl From<FdError> for SysError {
    fn from(value: FdError) -> Self {
        match value {
            FdError::BadFd => SysError::BadFd,
            FdError::Permission => SysError::Permission,
            FdError::Fs(err) => SysError::from(err),
        }
    }
}

#[derive(Clone)]
enum OpenFile {
    Stdin {
        data: Vec<u8>,
        offset: usize,
    },
    Stdout {
        data: Vec<u8>,
    },
    Stderr {
        data: Vec<u8>,
    },
    File {
        path: String,
        offset: usize,
        flags: OpenFlags,
    },
}

/// 每进程文件描述符表。
///
/// Unix/POSIX 的重定向本质不是 shell 自己“特殊写文件”，而是 shell 在 fork 后、
/// exec 前改子进程的 fd 表。例如 `echo hi > /tmp/x` 可以理解为：
///
/// 1. shell fork 出子进程；
/// 2. 子进程 open("/tmp/x", WRITE|CREATE|TRUNC)，得到某个 fd；
/// 3. 子进程 dup2(fd, 1)，把 stdout 指向该文件；
/// 4. 子进程 exec("/bin/echo", ["echo", "hi"])。
///
/// 这里先实现内核侧 fd 表模型。它还没有接到真实进程结构里，但接口已经按每进程
/// 独立 fd 表设计，后续只需要把 `FileDescriptorTable` 放进 `Proc`。
#[derive(Clone)]
pub struct FileDescriptorTable {
    entries: Vec<Option<OpenFile>>,
}

impl FileDescriptorTable {
    pub fn new() -> Self {
        Self {
            entries: alloc::vec![
                Some(OpenFile::Stdin {
                    data: Vec::new(),
                    offset: 0,
                }),
                Some(OpenFile::Stdout { data: Vec::new() }),
                Some(OpenFile::Stderr { data: Vec::new() }),
            ],
        }
    }

    pub fn set_stdin(&mut self, data: &[u8]) {
        self.entries[0] = Some(OpenFile::Stdin {
            data: data.to_vec(),
            offset: 0,
        });
    }

    pub fn stdout(&self) -> &[u8] {
        match self.entries.get(1).and_then(Option::as_ref) {
            Some(OpenFile::Stdout { data }) => data,
            _ => &[],
        }
    }

    pub fn stderr(&self) -> &[u8] {
        match self.entries.get(2).and_then(Option::as_ref) {
            Some(OpenFile::Stderr { data }) => data,
            _ => &[],
        }
    }

    pub fn open(&mut self, fs: &mut RamFs, path: &str, flags: OpenFlags) -> Result<usize, FdError> {
        if flags.contains(OpenFlags::CREATE) {
            fs.touch(path)?;
        }
        if flags.contains(OpenFlags::TRUNC) {
            fs.write(path, &[])?;
        } else if !flags.contains(OpenFlags::CREATE) {
            // 读一次只为了确认路径存在且不是目录。这样 open 成功后，后续 read/write
            // 的错误更接近普通 Unix 文件语义。
            let _ = fs.read(path)?;
        }

        let file = OpenFile::File {
            path: path.to_string(),
            offset: 0,
            flags,
        };
        Ok(self.insert(file))
    }

    pub fn close(&mut self, fd: usize) -> Result<usize, FdError> {
        if fd >= self.entries.len() || self.entries[fd].is_none() {
            return Err(FdError::BadFd);
        }
        self.entries[fd] = None;
        Ok(0)
    }

    pub fn dup2(&mut self, old_fd: usize, new_fd: usize) -> Result<usize, FdError> {
        let old = self
            .entries
            .get(old_fd)
            .and_then(Option::as_ref)
            .ok_or(FdError::BadFd)?
            .clone();

        while self.entries.len() <= new_fd {
            self.entries.push(None);
        }
        self.entries[new_fd] = Some(old);
        Ok(new_fd)
    }

    pub fn read(&mut self, fs: &RamFs, fd: usize, buf: &mut [u8]) -> Result<usize, FdError> {
        let file = self.entry_mut(fd)?;
        match file {
            OpenFile::Stdin { data, offset } => {
                let n = copy_from_offset(data, *offset, buf);
                *offset += n;
                Ok(n)
            }
            OpenFile::File { path, offset, flags } => {
                if !flags.contains(OpenFlags::READ) {
                    return Err(FdError::Permission);
                }
                let data = fs.read(path)?;
                let n = copy_from_offset(&data, *offset, buf);
                *offset += n;
                Ok(n)
            }
            OpenFile::Stdout { .. } | OpenFile::Stderr { .. } => Err(FdError::Permission),
        }
    }

    pub fn write(&mut self, fs: &mut RamFs, fd: usize, buf: &[u8]) -> Result<usize, FdError> {
        let file = self.entry_mut(fd)?;
        match file {
            OpenFile::Stdout { data } | OpenFile::Stderr { data } => {
                data.extend_from_slice(buf);
                Ok(buf.len())
            }
            OpenFile::File { path, offset, flags } => {
                if !flags.contains(OpenFlags::WRITE) {
                    return Err(FdError::Permission);
                }

                if flags.contains(OpenFlags::APPEND) {
                    fs.append(path, buf)?;
                } else {
                    let mut data = fs.read(path).unwrap_or_default();
                    if *offset > data.len() {
                        data.resize(*offset, 0);
                    }
                    if *offset + buf.len() > data.len() {
                        data.resize(*offset + buf.len(), 0);
                    }
                    data[*offset..*offset + buf.len()].copy_from_slice(buf);
                    fs.write(path, &data)?;
                    *offset += buf.len();
                }
                Ok(buf.len())
            }
            OpenFile::Stdin { .. } => Err(FdError::Permission),
        }
    }

    fn insert(&mut self, file: OpenFile) -> usize {
        for (fd, slot) in self.entries.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(file);
                return fd;
            }
        }
        self.entries.push(Some(file));
        self.entries.len() - 1
    }

    fn entry_mut(&mut self, fd: usize) -> Result<&mut OpenFile, FdError> {
        self.entries
            .get_mut(fd)
            .and_then(Option::as_mut)
            .ok_or(FdError::BadFd)
    }
}

impl Default for FileDescriptorTable {
    fn default() -> Self {
        Self::new()
    }
}

fn copy_from_offset(src: &[u8], offset: usize, dst: &mut [u8]) -> usize {
    if offset >= src.len() {
        return 0;
    }
    let n = core::cmp::min(dst.len(), src.len() - offset);
    dst[..n].copy_from_slice(&src[offset..offset + n]);
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdin_stdout_work() {
        let mut fs = RamFs::new();
        let mut table = FileDescriptorTable::new();
        table.set_stdin(b"abc");

        let mut buf = [0u8; 2];
        assert_eq!(table.read(&fs, 0, &mut buf).unwrap(), 2);
        assert_eq!(&buf, b"ab");

        assert_eq!(table.write(&mut fs, 1, b"ok").unwrap(), 2);
        assert_eq!(table.stdout(), b"ok");
    }

    #[test]
    fn dup2_redirects_stdout_to_file() {
        let mut fs = RamFs::new();
        let mut table = FileDescriptorTable::new();

        let fd = table
            .open(
                &mut fs,
                "/home/out",
                OpenFlags::CREATE | OpenFlags::WRITE | OpenFlags::TRUNC,
            )
            .unwrap();
        table.dup2(fd, 1).unwrap();
        table.write(&mut fs, 1, b"redirected").unwrap();

        assert_eq!(fs.read("/home/out").unwrap(), b"redirected");
    }

    #[test]
    fn file_offsets_advance() {
        let mut fs = RamFs::new();
        fs.write("/home/data", b"abcdef").unwrap();
        let mut table = FileDescriptorTable::new();
        let fd = table.open(&mut fs, "/home/data", OpenFlags::READ).unwrap();

        let mut buf = [0u8; 3];
        assert_eq!(table.read(&fs, fd, &mut buf).unwrap(), 3);
        assert_eq!(&buf, b"abc");
        assert_eq!(table.read(&fs, fd, &mut buf).unwrap(), 3);
        assert_eq!(&buf, b"def");
    }
}
