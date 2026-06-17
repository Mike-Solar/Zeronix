use alloc::vec::Vec;

pub mod commands;

/// 生成一个最小的用户态 syscall 烟测程序。
///
/// 当前教学内核还没有 ELF 装载器，也没有用户态 libc，所以这里直接生成一小段
/// x86_64 机器码交给 `spawn_user` 映射到用户代码页。程序逻辑等价于：
///
/// ```text
/// write(1, "user syscall write ok\n", 22);
/// loop {}
/// ```
///
/// x86_64 的 `syscall` 调用约定和普通 C ABI 不完全一样：
/// - `rax` 放系统调用号；
/// - `rdi/rsi/rdx/r10/r8/r9` 放参数；
/// - CPU 会把返回 RIP 存到 `rcx`，把 RFLAGS 存到 `r11`。
///
/// 这段程序使用 RIP 相对寻址拿到字符串地址，因此不需要知道自己最终被映射到
/// 哪个用户虚拟地址；只要代码和字符串在同一页内即可。
pub fn syscall_write_smoke_program() -> Vec<u8> {
    const SYS_WRITE: u32 = 6;
    const STDOUT: u32 = 1;
    const MESSAGE: &[u8] = b"user syscall write ok\n";

    let mut code = Vec::new();

    // mov eax, SYS_WRITE
    code.push(0xb8);
    code.extend_from_slice(&SYS_WRITE.to_le_bytes());

    // mov edi, STDOUT
    code.push(0xbf);
    code.extend_from_slice(&STDOUT.to_le_bytes());

    // lea rsi, [rip + message]
    let lea_start = code.len();
    code.extend_from_slice(&[0x48, 0x8d, 0x35, 0, 0, 0, 0]);
    let lea_next = code.len();

    // mov edx, MESSAGE.len()
    code.push(0xba);
    code.extend_from_slice(&(MESSAGE.len() as u32).to_le_bytes());

    // syscall
    code.extend_from_slice(&[0x0f, 0x05]);

    // jmp $，避免用户程序返回到未知地址。
    code.extend_from_slice(&[0xeb, 0xfe]);

    let message_offset = code.len();
    let displacement = message_offset as isize - lea_next as isize;
    let displacement = displacement as i32;
    code[lea_start + 3..lea_start + 7].copy_from_slice(&displacement.to_le_bytes());
    code.extend_from_slice(MESSAGE);

    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_program_uses_rip_relative_message() {
        let code = syscall_write_smoke_program();
        let lea_start = 10;
        let disp = i32::from_le_bytes([
            code[lea_start + 3],
            code[lea_start + 4],
            code[lea_start + 5],
            code[lea_start + 6],
        ]) as isize;
        let message = b"user syscall write ok\n";
        let message_offset = code.len() - message.len();
        let target = lea_start as isize + 7 + disp;

        assert_eq!(target as usize, message_offset);
        assert_eq!(&code[message_offset..], message);
        assert!(code.len() < 4096);
    }
}
