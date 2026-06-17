#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
extern crate alloc;

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
pub mod trap;
pub mod lock;

use alloc::vec;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::panic::PanicInfo;
use multiboot2::BootInformation;
use stdio::LogLevel;
use mem::page::pagealloc::*;
use crate::lock::spin_lock::SpinLock;
use crate::mem::page::{init_kernel_page_table, switch_cr3};
use crate::trap::pic::pic_remap;

pub static BUDDY_ALLOCATOR: SpinLock<Option<BuddyAllocator>> = SpinLock::new(None);
// 课程设计：假设最大支持 512MB = 131072 页
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
    trap::idt::init();
    pic_remap(32, 40);

    printk!(LogLevel::Info, "Zeronix boot successfully!");
    loop {
        let mut vec = vec![0; 256];
        printk!(LogLevel::Info, "Vector allocated successfully!");
        for i in 0..256 {
            vec[i]=i;
        }
        for i in 0..255 {
            printk!(LogLevel::Info, "vec[{}]={}", i, vec[i]);
        }
        drop(vec);
        printk!(LogLevel::Info, "Vector drop successfully!");
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
