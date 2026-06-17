use alloc::string::String;
use alloc::vec::Vec;

use crate::fs::ramfs::{FsError, RamFs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    Empty,
    Unknown,
    MissingArgument,
    Fs(FsError),
}

impl From<FsError> for CommandError {
    fn from(value: FsError) -> Self {
        Self::Fs(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl CommandOutput {
    fn stdout(text: impl AsRef<[u8]>) -> Self {
        Self {
            stdout: text.as_ref().to_vec(),
            stderr: Vec::new(),
        }
    }

    fn empty() -> Self {
        Self {
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }
}

/// 运行一条教学 shell 命令。
///
/// 这里的“用户程序”暂时还是内核仓库里的纯 Rust 函数，但它们只依赖 `RamFs`
/// 暴露的文件系统接口。这样做的目的，是先把 shell 语义、路径操作和重定向规则
/// 测清楚；等 ELF/exec 装载器完成后，可以把每个命令拆成 `/bin/ls`、`/bin/cat`
/// 这样的独立用户程序。
///
/// 支持的重定向：
/// - `cmd > /path`：把 stdout 覆盖写入文件；
/// - `cmd >> /path`：把 stdout 追加写入文件；
/// - `cmd < /path`：把文件内容作为 stdin，目前主要给 `cat` 使用。
pub fn run_command(fs: &mut RamFs, line: &str, stdin: &[u8]) -> Result<CommandOutput, CommandError> {
    let mut words: Vec<&str> = line.split_whitespace().collect();
    if words.is_empty() {
        return Err(CommandError::Empty);
    }

    let mut input = stdin.to_vec();
    let mut redirect_out: Option<(&str, bool)> = None;
    let mut index = 0;
    while index < words.len() {
        match words[index] {
            ">" | ">>" | "<" => {
                if index + 1 >= words.len() {
                    return Err(CommandError::MissingArgument);
                }
                let op = words[index];
                let path = words[index + 1];
                if op == "<" {
                    input = fs.read(path)?;
                } else {
                    redirect_out = Some((path, op == ">>"));
                }
                words.drain(index..=index + 1);
            }
            _ => index += 1,
        }
    }

    let output = dispatch(fs, &words, &input)?;
    if let Some((path, append)) = redirect_out {
        if append {
            fs.append(path, &output.stdout)?;
        } else {
            fs.write(path, &output.stdout)?;
        }
        Ok(CommandOutput::empty())
    } else {
        Ok(output)
    }
}

fn dispatch(fs: &mut RamFs, words: &[&str], stdin: &[u8]) -> Result<CommandOutput, CommandError> {
    match words.first().copied().ok_or(CommandError::Empty)? {
        "ls" => cmd_ls(fs, words),
        "rm" => cmd_rm(fs, words),
        "mv" => cmd_mv(fs, words),
        "cp" => cmd_cp(fs, words),
        "touch" => cmd_touch(fs, words),
        "cat" => cmd_cat(fs, words, stdin),
        "echo" => cmd_echo(words),
        _ => Err(CommandError::Unknown),
    }
}

fn cmd_ls(fs: &RamFs, words: &[&str]) -> Result<CommandOutput, CommandError> {
    let path = words.get(1).copied().unwrap_or("/");
    let mut names = fs.list(path)?;
    names.sort();

    let mut out = String::new();
    for name in names {
        out.push_str(&name);
        out.push('\n');
    }
    Ok(CommandOutput::stdout(out))
}

fn cmd_rm(fs: &mut RamFs, words: &[&str]) -> Result<CommandOutput, CommandError> {
    let path = words.get(1).copied().ok_or(CommandError::MissingArgument)?;
    fs.remove(path)?;
    Ok(CommandOutput::empty())
}

fn cmd_mv(fs: &mut RamFs, words: &[&str]) -> Result<CommandOutput, CommandError> {
    let old = words.get(1).copied().ok_or(CommandError::MissingArgument)?;
    let new = words.get(2).copied().ok_or(CommandError::MissingArgument)?;
    fs.rename(old, new)?;
    Ok(CommandOutput::empty())
}

fn cmd_cp(fs: &mut RamFs, words: &[&str]) -> Result<CommandOutput, CommandError> {
    let src = words.get(1).copied().ok_or(CommandError::MissingArgument)?;
    let dst = words.get(2).copied().ok_or(CommandError::MissingArgument)?;
    fs.copy(src, dst)?;
    Ok(CommandOutput::empty())
}

fn cmd_touch(fs: &mut RamFs, words: &[&str]) -> Result<CommandOutput, CommandError> {
    let path = words.get(1).copied().ok_or(CommandError::MissingArgument)?;
    fs.touch(path)?;
    Ok(CommandOutput::empty())
}

fn cmd_cat(fs: &RamFs, words: &[&str], stdin: &[u8]) -> Result<CommandOutput, CommandError> {
    if words.len() == 1 {
        return Ok(CommandOutput::stdout(stdin));
    }

    let mut out = Vec::new();
    for path in &words[1..] {
        out.extend_from_slice(&fs.read(path)?);
    }
    Ok(CommandOutput::stdout(out))
}

fn cmd_echo(words: &[&str]) -> Result<CommandOutput, CommandError> {
    let mut out = String::new();
    let mut first = true;
    for word in &words[1..] {
        if !first {
            out.push(' ');
        }
        first = false;
        out.push_str(word);
    }
    out.push('\n');
    Ok(CommandOutput::stdout(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implements_basic_file_commands() {
        let mut fs = RamFs::new();

        run_command(&mut fs, "touch /home/a", b"").unwrap();
        run_command(&mut fs, "echo hello > /home/a", b"").unwrap();
        assert_eq!(
            run_command(&mut fs, "cat /home/a", b"").unwrap().stdout,
            b"hello\n"
        );

        run_command(&mut fs, "cp /home/a /home/b", b"").unwrap();
        run_command(&mut fs, "mv /home/b /home/c", b"").unwrap();
        assert_eq!(
            run_command(&mut fs, "cat /home/c", b"").unwrap().stdout,
            b"hello\n"
        );

        run_command(&mut fs, "rm /home/c", b"").unwrap();
        assert!(matches!(
            run_command(&mut fs, "cat /home/c", b""),
            Err(CommandError::Fs(FsError::NotFound))
        ));
    }

    #[test]
    fn supports_stdout_append_and_stdin_redirect() {
        let mut fs = RamFs::new();

        run_command(&mut fs, "echo first > /home/log", b"").unwrap();
        run_command(&mut fs, "echo second >> /home/log", b"").unwrap();
        assert_eq!(
            run_command(&mut fs, "cat /home/log", b"").unwrap().stdout,
            b"first\nsecond\n"
        );

        assert_eq!(
            run_command(&mut fs, "cat < /home/log", b"ignored").unwrap().stdout,
            b"first\nsecond\n"
        );
    }

    #[test]
    fn ls_lists_directory_entries() {
        let mut fs = RamFs::new();
        run_command(&mut fs, "touch /home/z", b"").unwrap();
        run_command(&mut fs, "touch /home/a", b"").unwrap();

        assert_eq!(
            run_command(&mut fs, "ls /home", b"").unwrap().stdout,
            b"a\nz\n"
        );
    }
}
