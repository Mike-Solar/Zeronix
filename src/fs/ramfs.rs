use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    InvalidPath,
    NotFound,
    AlreadyExists,
    NotDirectory,
    IsDirectory,
    DirectoryNotEmpty,
}

pub type FsResult<T> = Result<T, FsError>;

#[derive(Clone)]
enum Node {
    File(Vec<u8>),
    Dir(BTreeMap<String, Node>),
}

/// 内存盘文件系统。
///
/// 这是第一版文件系统：所有数据都保存在内存中的树形结构里，不落盘，
/// 也没有权限、时间戳、inode 编号和硬链接。它的目标不是模拟完整 Unix FS，而是
/// 给系统调用、shell 和用户程序提供一个足够清晰的 POSIX 风格接口。
///
/// 初始化时会创建几个基础目录：
/// - `/bin`：放 shell、ls、cat 等用户程序；
/// - `/lib`：预留给以后放 libc/动态库或运行时；
/// - `/home`：用户工作目录；
/// - `/tmp`：临时文件目录。
pub struct RamFs {
    root: Node,
}

impl RamFs {
    pub fn new() -> Self {
        let mut fs = Self {
            root: Node::Dir(BTreeMap::new()),
        };
        fs.mkdir("/bin").expect("init /bin");
        fs.mkdir("/lib").expect("init /lib");
        fs.mkdir("/home").expect("init /home");
        fs.mkdir("/tmp").expect("init /tmp");
        fs
    }

    pub fn mkdir(&mut self, path: &str) -> FsResult<()> {
        let parts = split_path(path)?;
        if parts.is_empty() {
            return Err(FsError::AlreadyExists);
        }

        let (parent, name) = split_parent(&parts)?;
        let dir = self.dir_mut(parent)?;
        if dir.contains_key(name) {
            return Err(FsError::AlreadyExists);
        }
        dir.insert(name.to_string(), Node::Dir(BTreeMap::new()));
        Ok(())
    }

    pub fn touch(&mut self, path: &str) -> FsResult<()> {
        let parts = split_path(path)?;
        let (parent, name) = split_parent(&parts)?;
        let dir = self.dir_mut(parent)?;
        match dir.get(name) {
            Some(Node::Dir(_)) => Err(FsError::IsDirectory),
            Some(Node::File(_)) => Ok(()),
            None => {
                dir.insert(name.to_string(), Node::File(Vec::new()));
                Ok(())
            }
        }
    }

    pub fn write(&mut self, path: &str, data: &[u8]) -> FsResult<()> {
        let parts = split_path(path)?;
        let (parent, name) = split_parent(&parts)?;
        let dir = self.dir_mut(parent)?;
        match dir.get_mut(name) {
            Some(Node::Dir(_)) => Err(FsError::IsDirectory),
            Some(Node::File(content)) => {
                content.clear();
                content.extend_from_slice(data);
                Ok(())
            }
            None => {
                dir.insert(name.to_string(), Node::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn append(&mut self, path: &str, data: &[u8]) -> FsResult<()> {
        let parts = split_path(path)?;
        let (parent, name) = split_parent(&parts)?;
        let dir = self.dir_mut(parent)?;
        match dir.get_mut(name) {
            Some(Node::Dir(_)) => Err(FsError::IsDirectory),
            Some(Node::File(content)) => {
                content.extend_from_slice(data);
                Ok(())
            }
            None => {
                dir.insert(name.to_string(), Node::File(data.to_vec()));
                Ok(())
            }
        }
    }

    pub fn read(&self, path: &str) -> FsResult<Vec<u8>> {
        match self.node(path)? {
            Node::File(content) => Ok(content.clone()),
            Node::Dir(_) => Err(FsError::IsDirectory),
        }
    }

    pub fn list(&self, path: &str) -> FsResult<Vec<String>> {
        match self.node(path)? {
            Node::File(_) => Err(FsError::NotDirectory),
            Node::Dir(entries) => Ok(entries.keys().cloned().collect()),
        }
    }

    pub fn remove(&mut self, path: &str) -> FsResult<()> {
        let parts = split_path(path)?;
        let (parent, name) = split_parent(&parts)?;
        let dir = self.dir_mut(parent)?;
        match dir.get(name) {
            None => Err(FsError::NotFound),
            Some(Node::Dir(entries)) if !entries.is_empty() => Err(FsError::DirectoryNotEmpty),
            Some(_) => {
                dir.remove(name);
                Ok(())
            }
        }
    }

    pub fn rename(&mut self, old: &str, new: &str) -> FsResult<()> {
        let old_parts = split_path(old)?;
        let new_parts = split_path(new)?;
        let (old_parent, old_name) = split_parent(&old_parts)?;
        let (new_parent, new_name) = split_parent(&new_parts)?;

        let node = {
            let old_dir = self.dir_mut(old_parent)?;
            old_dir.remove(old_name).ok_or(FsError::NotFound)?
        };

        let new_dir = self.dir_mut(new_parent)?;
        if new_dir.contains_key(new_name) {
            return Err(FsError::AlreadyExists);
        }
        new_dir.insert(new_name.to_string(), node);
        Ok(())
    }

    pub fn copy(&mut self, src: &str, dst: &str) -> FsResult<()> {
        let data = self.read(src)?;
        self.write(dst, &data)
    }

    fn node(&self, path: &str) -> FsResult<&Node> {
        let parts = split_path(path)?;
        let mut current = &self.root;
        for part in parts {
            current = match current {
                Node::File(_) => return Err(FsError::NotDirectory),
                Node::Dir(entries) => entries.get(part).ok_or(FsError::NotFound)?,
            };
        }
        Ok(current)
    }

    fn dir_mut(&mut self, parts: &[&str]) -> FsResult<&mut BTreeMap<String, Node>> {
        let mut current = &mut self.root;
        for part in parts {
            current = match current {
                Node::File(_) => return Err(FsError::NotDirectory),
                Node::Dir(entries) => entries.get_mut(*part).ok_or(FsError::NotFound)?,
            };
        }

        match current {
            Node::File(_) => Err(FsError::NotDirectory),
            Node::Dir(entries) => Ok(entries),
        }
    }
}

impl Default for RamFs {
    fn default() -> Self {
        Self::new()
    }
}

fn split_path(path: &str) -> FsResult<Vec<&str>> {
    if !path.starts_with('/') {
        return Err(FsError::InvalidPath);
    }

    Ok(path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect())
}

fn split_parent<'a>(parts: &'a [&'a str]) -> FsResult<(&'a [&'a str], &'a str)> {
    if parts.is_empty() {
        return Err(FsError::InvalidPath);
    }
    Ok((&parts[..parts.len() - 1], parts[parts.len() - 1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_required_directories() {
        let fs = RamFs::new();
        let root = fs.list("/").unwrap();

        assert!(root.contains(&"bin".to_string()));
        assert!(root.contains(&"lib".to_string()));
        assert!(root.contains(&"home".to_string()));
    }

    #[test]
    fn file_lifecycle_works() {
        let mut fs = RamFs::new();

        fs.write("/home/a.txt", b"hello").unwrap();
        assert_eq!(fs.read("/home/a.txt").unwrap(), b"hello");

        fs.copy("/home/a.txt", "/home/b.txt").unwrap();
        assert_eq!(fs.read("/home/b.txt").unwrap(), b"hello");

        fs.rename("/home/b.txt", "/home/c.txt").unwrap();
        assert_eq!(fs.read("/home/c.txt").unwrap(), b"hello");

        fs.remove("/home/c.txt").unwrap();
        assert_eq!(fs.read("/home/c.txt"), Err(FsError::NotFound));
    }

    #[test]
    fn refuses_to_remove_non_empty_directory() {
        let mut fs = RamFs::new();

        fs.mkdir("/home/demo").unwrap();
        fs.touch("/home/demo/file").unwrap();

        assert_eq!(fs.remove("/home/demo"), Err(FsError::DirectoryNotEmpty));
    }
}
