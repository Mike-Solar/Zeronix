use alloc::vec::Vec;

use crate::fs::ramfs::RamFs;

const ELF_HEADER_SIZE: usize = 64;
const PROGRAM_HEADER_SIZE: usize = 56;
const PT_LOAD: u32 = 1;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const EV_CURRENT: u32 = 1;
const ENTRY_BASE: u64 = 0x0000_0000_0040_0000;
const ENTRY_STRIDE: u64 = 0x1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    TooSmall,
    BadMagic,
    Unsupported,
    Truncated,
    MissingLoadSegment,
    UnknownProgram,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramId {
    Shell = 1,
    Ls = 2,
    Rm = 3,
    Mv = 4,
    Cp = 5,
    Touch = 6,
    Cat = 7,
    Echo = 8,
    Sleep = 9,
    Count = 10,
}

impl ProgramId {
    pub const ALL: [Self; 10] = [
        Self::Shell,
        Self::Ls,
        Self::Rm,
        Self::Mv,
        Self::Cp,
        Self::Touch,
        Self::Cat,
        Self::Echo,
        Self::Sleep,
        Self::Count,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Shell => "shell",
            Self::Ls => "ls",
            Self::Rm => "rm",
            Self::Mv => "mv",
            Self::Cp => "cp",
            Self::Touch => "touch",
            Self::Cat => "cat",
            Self::Echo => "echo",
            Self::Sleep => "sleep",
            Self::Count => "count",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|program| program.name() == name)
    }

    pub fn entry(self) -> u64 {
        ENTRY_BASE + self as u64 * ENTRY_STRIDE
    }

    fn from_entry(entry: u64) -> Option<Self> {
        let offset = entry.checked_sub(ENTRY_BASE)?;
        if offset % ENTRY_STRIDE != 0 {
            return None;
        }
        let id = offset / ENTRY_STRIDE;
        Self::ALL.into_iter().find(|program| *program as u64 == id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadSegment {
    pub offset: usize,
    pub virt_addr: u64,
    pub file_size: usize,
    pub mem_size: usize,
    pub flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedElf {
    pub entry: u64,
    pub program: ProgramId,
    pub load: LoadSegment,
}

pub fn parse(image: &[u8]) -> Result<ParsedElf, ElfError> {
    if image.len() < ELF_HEADER_SIZE {
        return Err(ElfError::TooSmall);
    }
    if &image[0..4] != b"\x7fELF" {
        return Err(ElfError::BadMagic);
    }
    if image[4] != 2 || image[5] != 1 || image[6] != 1 {
        return Err(ElfError::Unsupported);
    }

    let e_type = read_u16(image, 16)?;
    let e_machine = read_u16(image, 18)?;
    let e_version = read_u32(image, 20)?;
    if e_type != ET_EXEC || e_machine != EM_X86_64 || e_version != EV_CURRENT {
        return Err(ElfError::Unsupported);
    }

    let entry = read_u64(image, 24)?;
    let phoff = read_u64(image, 32)? as usize;
    let ehsize = read_u16(image, 52)? as usize;
    let phentsize = read_u16(image, 54)? as usize;
    let phnum = read_u16(image, 56)? as usize;
    if ehsize != ELF_HEADER_SIZE || phentsize != PROGRAM_HEADER_SIZE || phnum == 0 {
        return Err(ElfError::Unsupported);
    }

    let program = ProgramId::from_entry(entry).ok_or(ElfError::UnknownProgram)?;
    let mut load = None;
    for index in 0..phnum {
        let offset = phoff + index * phentsize;
        if offset + PROGRAM_HEADER_SIZE > image.len() {
            return Err(ElfError::Truncated);
        }

        let p_type = read_u32(image, offset)?;
        if p_type != PT_LOAD {
            continue;
        }

        let p_flags = read_u32(image, offset + 4)?;
        let p_offset = read_u64(image, offset + 8)? as usize;
        let p_vaddr = read_u64(image, offset + 16)?;
        let p_filesz = read_u64(image, offset + 32)? as usize;
        let p_memsz = read_u64(image, offset + 40)? as usize;
        if p_offset.checked_add(p_filesz).ok_or(ElfError::Truncated)? > image.len() {
            return Err(ElfError::Truncated);
        }
        if p_memsz < p_filesz {
            return Err(ElfError::Unsupported);
        }

        load = Some(LoadSegment {
            offset: p_offset,
            virt_addr: p_vaddr,
            file_size: p_filesz,
            mem_size: p_memsz,
            flags: p_flags,
        });
        break;
    }

    Ok(ParsedElf {
        entry,
        program,
        load: load.ok_or(ElfError::MissingLoadSegment)?,
    })
}

pub fn build_program_image(program: ProgramId) -> Vec<u8> {
    let payload = user_program_code(program);
    let payload_offset = ELF_HEADER_SIZE + PROGRAM_HEADER_SIZE;
    let mut image = alloc::vec![0; payload_offset + payload.len()];

    image[0..4].copy_from_slice(b"\x7fELF");
    image[4] = 2;
    image[5] = 1;
    image[6] = 1;
    image[7] = 0;

    write_u16(&mut image, 16, ET_EXEC);
    write_u16(&mut image, 18, EM_X86_64);
    write_u32(&mut image, 20, EV_CURRENT);
    write_u64(&mut image, 24, program.entry());
    write_u64(&mut image, 32, ELF_HEADER_SIZE as u64);
    write_u64(&mut image, 40, 0);
    write_u64(&mut image, 48, 0);
    write_u16(&mut image, 52, ELF_HEADER_SIZE as u16);
    write_u16(&mut image, 54, PROGRAM_HEADER_SIZE as u16);
    write_u16(&mut image, 56, 1);
    write_u16(&mut image, 58, 0);
    write_u16(&mut image, 60, 0);
    write_u16(&mut image, 62, 0);

    let ph = ELF_HEADER_SIZE;
    write_u32(&mut image, ph, PT_LOAD);
    write_u32(&mut image, ph + 4, 0x5);
    write_u64(&mut image, ph + 8, payload_offset as u64);
    write_u64(&mut image, ph + 16, program.entry());
    write_u64(&mut image, ph + 24, program.entry());
    write_u64(&mut image, ph + 32, payload.len() as u64);
    write_u64(&mut image, ph + 40, payload.len() as u64);
    write_u64(&mut image, ph + 48, 0x1000);

    image[payload_offset..].copy_from_slice(&payload);
    image
}

fn user_program_code(program: ProgramId) -> Vec<u8> {
    const SYS_EXIT: u32 = 3;
    const SYS_WRITE: u32 = 6;
    const STDOUT: u32 = 1;

    let mut message = alloc::string::String::from("exec /bin/");
    message.push_str(program.name());
    message.push_str(" ok\r\n");
    let message = message.into_bytes();

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

    // mov edx, message.len()
    code.push(0xba);
    code.extend_from_slice(&(message.len() as u32).to_le_bytes());

    // syscall
    code.extend_from_slice(&[0x0f, 0x05]);

    // mov eax, SYS_EXIT
    code.push(0xb8);
    code.extend_from_slice(&SYS_EXIT.to_le_bytes());

    // xor edi, edi
    code.extend_from_slice(&[0x31, 0xff]);

    // syscall
    code.extend_from_slice(&[0x0f, 0x05]);

    // jmp $，如果 exit 尚未接通，不继续执行随机内存。
    code.extend_from_slice(&[0xeb, 0xfe]);

    let message_offset = code.len();
    let displacement = message_offset as isize - lea_next as isize;
    code[lea_start + 3..lea_start + 7].copy_from_slice(&(displacement as i32).to_le_bytes());
    code.extend_from_slice(&message);

    code
}

pub fn install_programs(fs: &mut RamFs) {
    for program in ProgramId::ALL {
        let mut path = alloc::string::String::from("/bin/");
        path.push_str(program.name());
        let image = build_program_image(program);
        let _ = fs.write(&path, &image);
    }
}

fn read_u16(image: &[u8], offset: usize) -> Result<u16, ElfError> {
    let bytes = image.get(offset..offset + 2).ok_or(ElfError::Truncated)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(image: &[u8], offset: usize) -> Result<u32, ElfError> {
    let bytes = image.get(offset..offset + 4).ok_or(ElfError::Truncated)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(image: &[u8], offset: usize) -> Result<u64, ElfError> {
    let bytes = image.get(offset..offset + 8).ok_or(ElfError::Truncated)?;
    Ok(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn write_u16(image: &mut [u8], offset: usize, value: u16) {
    image[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(image: &mut [u8], offset: usize, value: u32) {
    image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(image: &mut [u8], offset: usize, value: u64) {
    image[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_generated_program_image() {
        let image = build_program_image(ProgramId::Ls);
        let parsed = parse(&image).unwrap();

        assert_eq!(parsed.program, ProgramId::Ls);
        assert_eq!(parsed.entry, ProgramId::Ls.entry());
        assert_eq!(parsed.load.virt_addr, ProgramId::Ls.entry());
        assert!(parsed.load.file_size > 16);
    }

    #[test]
    fn rejects_non_elf_data() {
        let image = [0u8; ELF_HEADER_SIZE];
        assert_eq!(parse(&image), Err(ElfError::BadMagic));
    }
}
