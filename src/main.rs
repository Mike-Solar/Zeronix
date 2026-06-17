#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

mod stdio;
mod list;
mod alloc_layout;

// 链接器导出的符号：它们的"地址"就是物理地址值
unsafe extern "C" {
    static _kernel_phys_start: usize;
    static _kernel_phys_end: usize;
    static _bootdata_start: usize;
    static _bootdata_end: usize;
}
mod mem;
mod fs;
mod syscall;
mod trap;
mod lock;
mod task;
mod user;

use core::alloc::Layout;
use core::panic::PanicInfo;
use multiboot2::BootInformation;
use stdio::LogLevel;
use mem::page::pagealloc::*;
use crate::lock::spin_lock::SpinLock;
use crate::mem::page::{init_kernel_page_table, switch_cr3};

pub static BUDDY_ALLOCATOR: SpinLock<Option<BuddyAllocator>> = SpinLock::new(None);
// 假设最大支持 512MB = 131072 页
// PageInfo 8 字节 * 131072 = 1MB，放在 .bss 中
const MAX_SUPPORTED_PAGES: usize = 131072;

#[unsafe(no_mangle)]
pub extern "C" fn kernel_main(mbi_ptr: u64, magic: u32) -> ! {

    printk!(LogLevel::Info, "Welcome to Zeronix!");

    assert_eq!(magic, multiboot2::MAGIC, "Not loaded by Multiboot2!");
    let boot_info = unsafe {
        BootInformation::load(mbi_ptr as *const _)
            .expect("Failed to parse Multiboot2 info")
    };

    let mem_map = boot_info.memory_map_tag().expect("No memory map from GRUB");

    // 链接器符号的值 = 物理地址
    let kernel_start = core::ptr::addr_of!(_kernel_phys_start) as usize;
    let kernel_end = core::ptr::addr_of!(_kernel_phys_end) as usize;
    let bootdata_start = core::ptr::addr_of!(_bootdata_start) as usize;
    let bootdata_end = core::ptr::addr_of!(_bootdata_end) as usize;

    // MBI 结构本身占用的内存（GRUB 放在某处，必须保留）
    let mbi_start = mbi_ptr as usize;
    let mbi_end = mbi_start + boot_info.total_size() as usize;

    // 计算总页数
    let max_phys = mem_map.memory_areas()
        .into_iter().map(|a| a.end_address() as usize)
        .max()
        .unwrap_or(0);
    let total_pages = max_phys.div_ceil(PAGE_SIZE);

    // Metadata 数组：放在 .bss 中，编译时确定大小
    static mut METADATA: [PageInfo; MAX_SUPPORTED_PAGES] = [PageInfo{order: 0, flags: 0,
        next: PageInfo::NONE, prev: PageInfo::NONE}; MAX_SUPPORTED_PAGES];

    let metadata = unsafe {
        let base = core::ptr::addr_of_mut!(METADATA) as *mut PageInfo;
        core::slice::from_raw_parts_mut(base, total_pages.min(MAX_SUPPORTED_PAGES))
    };

    // 保留区域：内核 ELF、bootdata（页表+栈）、MBI 信息结构
    let reserved = [
        (kernel_start, kernel_end),       // 0x100000 ~ .bss 结束
        (bootdata_start, bootdata_end),    // bootdata 页表+栈
        (mbi_start, mbi_end),              // GRUB 传来的 MBI
    ];

    BUDDY_ALLOCATOR.lock().replace(BuddyAllocator::new(metadata, mem_map, &reserved));

    let addr = init_kernel_page_table(
        &mem_map,
        kernel_start,
        kernel_end,
        bootdata_start,
        bootdata_end,
        mbi_start,
        mbi_end,
    );

    unsafe {switch_cr3(addr);}
    trap::gdt::init();
    syscall::init();
    syscall::init_runtime_fs();
    syscall::set_console_write(serial_console_write);
    syscall::set_process_hooks(exec_program, task::proc::exit_current);
    trap::idt::init();
    task::proc::init();
    let syscall_smoke = user::syscall_write_smoke_program();
    task::proc::spawn_user(&syscall_smoke);
    task::proc::spawn_user(&[0xeb, 0xfe]);
    trap::pic::init();
    stdio::init_serial_interrupts();
    trap::pic::unmask_irq(0);
    trap::pic::unmask_irq(1);
    trap::pic::unmask_irq(4);
    trap::pit::init_100hz();
    unsafe { trap::enable_interrupts(); }

    printk!(LogLevel::Info, "Zeronix boot successfully!");
    let mut shell = Shell::new();
    shell.print_prompt();
    loop {
        unsafe { trap::halt(); }
        shell.poll_jobs();
        while let Some(byte) = stdio::read_serial_byte() {
            shell.handle_byte(byte);
        }
    }
}
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    printk!(LogLevel::Error, "{}", _info);
    loop {}
}

#[alloc_error_handler]
fn alloc_error(layout: Layout) -> ! {
    panic!("allocation error: {:?}", layout);
}

fn serial_console_write(buf: &[u8]) {
    stdio::serial_write(buf);
}

fn exec_program(image: &[u8]) -> syscall::SysResult {
    let proc_ref = task::proc::spawn_elf(image).map_err(|_| syscall::SysError::InvalidArgument)?;
    Ok(proc_ref.pid() as usize)
}

struct Shell {
    line: String,
    jobs: Vec<Job>,
    active: Option<Job>,
    next_job_id: u64,
    last_second: u64,
}

impl Shell {
    fn new() -> Self {
        Self {
            line: String::new(),
            jobs: Vec::new(),
            active: None,
            next_job_id: 1,
            last_second: 0,
        }
    }

    fn print_prompt(&self) {
        stdio::serial_write(b"zeronix:/home$ ");
    }

    fn handle_byte(&mut self, byte: u8) {
        if self.active.is_some() {
            if byte == 26 {
                self.suspend_active_job();
            }
            return;
        }

        match byte {
            b'\r' | b'\n' => {
                stdio::serial_write(b"\r\n");
                self.run_line();
                self.line.clear();
                self.print_prompt();
            }
            8 | 127 => {
                if self.line.pop().is_some() {
                    stdio::serial_write(b"\x08 \x08");
                }
            }
            byte if byte.is_ascii_graphic() || byte == b' ' => {
                self.line.push(byte as char);
                stdio::serial_write(&[byte]);
            }
            _ => {}
        }
    }

    fn run_line(&mut self) {
        let mut line = self.line.trim().to_string();
        if line.is_empty() {
            return;
        }

        let background = line.ends_with('&');
        if background {
            line.pop();
            line = line.trim_end().to_string();
            if line.is_empty() {
                return;
            }
        }

        if self.run_shell_command(&line, background) {
            return;
        }

        let Some(result) = syscall::with_runtime_fs(|fs| user::commands::run_command(fs, &line, b"")) else {
            stdio::serial_write(b"shell: filesystem is not initialized\r\n");
            return;
        };

        match result {
            Ok(output) => {
                stdio::serial_write(&output.stdout);
                stdio::serial_write(&output.stderr);
            }
            Err(err) => {
                printk!(LogLevel::Error, "shell command failed: {:?}", err);
            }
        }
    }

    fn run_shell_command(&mut self, line: &str, background: bool) -> bool {
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some(command) = words.first().copied() else {
            return true;
        };

        match command {
            "jobs" => {
                self.print_jobs();
                true
            }
            "fg" => {
                self.foreground_job(words.get(1).copied());
                true
            }
            "run" | "exec" => {
                self.run_elf(words.get(1).copied());
                true
            }
            "sleep" | "count" => {
                self.start_long_job(command, &words, background);
                true
            }
            _ => false,
        }
    }

    fn run_elf(&mut self, program: Option<&str>) {
        let Some(program) = program else {
            stdio::serial_write(b"usage: run <program>\r\n");
            return;
        };

        let image = syscall::with_runtime_fs(|fs| {
            let mut path = String::from("/bin/");
            path.push_str(program.trim_start_matches("/bin/"));
            fs.read(&path)
        });

        let Some(Ok(image)) = image else {
            stdio::serial_write(b"run: executable not found\r\n");
            return;
        };

        match task::proc::spawn_elf(&image) {
            Ok(proc_ref) => {
                stdio::serial_write(format!("started pid {}\r\n", proc_ref.pid()).as_bytes());
            }
            Err(_) => {
                stdio::serial_write(b"run: invalid executable\r\n");
            }
        }
    }

    fn start_long_job(&mut self, command: &str, words: &[&str], background: bool) {
        let Some(program_ok) = syscall::with_runtime_fs(|fs| user::commands::load_program(fs, command)) else {
            stdio::serial_write(b"shell: filesystem is not initialized\r\n");
            return;
        };
        if program_ok.is_err() {
            stdio::serial_write(b"shell: executable not found\r\n");
            return;
        }

        let seconds = words
            .get(1)
            .and_then(|value| parse_u32(value))
            .unwrap_or(5)
            .max(1);
        let id = self.next_job_id;
        self.next_job_id += 1;
        let job = Job {
            id,
            name: command.to_string(),
            kind: if command == "count" {
                JobKind::Count
            } else {
                JobKind::Sleep
            },
            remaining: seconds,
            total: seconds,
            background,
        };

        if background {
            stdio::serial_write(format!("[{}] {}\r\n", job.id, job.name).as_bytes());
            self.jobs.push(job);
        } else {
            stdio::serial_write(format!("{} running; Ctrl-Z sends it to background\r\n", job.name).as_bytes());
            self.active = Some(job);
        }
    }

    fn poll_jobs(&mut self) {
        let seconds = trap::idt::timer_ticks() / 100;
        while self.last_second < seconds {
            self.last_second += 1;
            self.tick_one_second();
        }
    }

    fn tick_one_second(&mut self) {
        if let Some(mut job) = self.active.take() {
            tick_job(&mut job);
            if job.remaining == 0 {
                stdio::serial_write(format!("{} done\r\n", job.name).as_bytes());
                self.print_prompt();
            } else {
                self.active = Some(job);
            }
        }

        let mut index = 0;
        while index < self.jobs.len() {
            tick_job(&mut self.jobs[index]);
            if self.jobs[index].remaining == 0 {
                let job = self.jobs.remove(index);
                stdio::serial_write(format!("\r\n[{}] done {}\r\n", job.id, job.name).as_bytes());
                if self.active.is_none() {
                    self.print_prompt();
                }
            } else {
                index += 1;
            }
        }
    }

    fn suspend_active_job(&mut self) {
        let Some(mut job) = self.active.take() else {
            return;
        };
        job.background = true;
        stdio::serial_write(format!("\r\n[{}] stopped {}\r\n", job.id, job.name).as_bytes());
        self.jobs.push(job);
        self.print_prompt();
    }

    fn print_jobs(&self) {
        if self.jobs.is_empty() {
            stdio::serial_write(b"no background jobs\r\n");
            return;
        }
        for job in &self.jobs {
            stdio::serial_write(
                format!("[{}] {} {}s remaining\r\n", job.id, job.name, job.remaining).as_bytes(),
            );
        }
    }

    fn foreground_job(&mut self, id: Option<&str>) {
        let Some(id) = id.and_then(parse_u64) else {
            stdio::serial_write(b"usage: fg <job-id>\r\n");
            return;
        };

        let Some(index) = self.jobs.iter().position(|job| job.id == id) else {
            stdio::serial_write(b"fg: no such job\r\n");
            return;
        };

        let mut job = self.jobs.remove(index);
        job.background = false;
        stdio::serial_write(format!("{} foreground\r\n", job.name).as_bytes());
        self.active = Some(job);
    }
}

#[derive(Clone)]
struct Job {
    id: u64,
    name: String,
    kind: JobKind,
    remaining: u32,
    total: u32,
    background: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum JobKind {
    Sleep,
    Count,
}

fn tick_job(job: &mut Job) {
    if job.remaining == 0 {
        return;
    }
    job.remaining -= 1;

    if job.kind == JobKind::Count {
        let current = job.total - job.remaining;
        stdio::serial_write(format!("{}: {}\r\n", job.name, current).as_bytes());
    }
}

fn parse_u32(text: &str) -> Option<u32> {
    let mut value = 0u32;
    for byte in text.bytes() {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u32)?;
    }
    Some(value)
}

fn parse_u64(text: &str) -> Option<u64> {
    let mut value = 0u64;
    for byte in text.bytes() {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u64)?;
    }
    Some(value)
}
