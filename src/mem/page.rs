pub mod pagealloc;
pub mod pagemapper;

use core::sync::atomic::{AtomicU64, Ordering};

use crate::mem::{MemoryAddress, PhysAddr, VirtAddr};

unsafe extern "C"{
    static _text_start: usize;
    static _text_end: usize;
    static _rodata_start: usize;
    static _rodata_end: usize;
    static _data_start: usize;
    static _data_end: usize;
    static _bss_start: usize;
    static _bss_end: usize;
}

#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct PageTableEntry(u64);
impl PageTableEntry {
    pub const fn new() -> Self { Self(0) }

    /// 设置该页的物理地址
    /// 按位与0x000F_FFFF_FFFF_F000，清空前12位和52-63位，因为页表项低12位是标志位，63位是flag_bit，剩下的
    /// 不是地址的一部分
    pub fn set_addr(&mut self, phys: PhysAddr, flags: EntryFlags) {
        self.0 = (phys.as_u64() & 0x000F_FFFF_FFFF_F000) | flags.bits();
    }

    /// 获取物理地址
    /// 物理页框必须4K对齐，所以低12位一定是0
    pub fn addr(&self) -> PhysAddr {
        PhysAddr::from(self.0 & 0x000F_FFFF_FFFF_F000)
    }

    pub fn flags(&self) -> EntryFlags {
        EntryFlags::from_bits_truncate(self.0)
    }

    pub fn add_flags(&mut self, flags: EntryFlags) {
        self.0 |= flags.bits();
    }

    pub fn is_present(&self) -> bool { self.0 & 1 != 0 }
    pub fn is_huge(&self) -> bool { self.0 & (1 << 7) != 0 }
}

bitflags::bitflags! {
    #[derive(Copy, Clone)]
    pub struct EntryFlags: u64 {
        const PRESENT = 1 << 0;
        const WRITABLE = 1 << 1;
        const USER = 1 << 2;
        const HUGE = 1 << 7;
        const GLOBAL = 1 << 8;
        const NX = 1 << 63;  // No Execute
    }
}
/// 一页页表 = 512 个 8-byte entries = 4096 bytes，正好一个物理页
#[repr(align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; 512],
}

impl PageTable {
    pub const fn new() -> Self {
        Self { entries: [PageTableEntry::new(); 512] }
    }

    pub fn set_entry(&mut self, index: usize, entry: PageTableEntry) {
        self.entries[index] = entry;
    }

    pub fn entry(&self, index: usize) -> &PageTableEntry {
        &self.entries[index]
    }

    pub fn entry_mut(&mut self, index: usize) -> &mut PageTableEntry {
        &mut self.entries[index]
    }
}

use multiboot2::MemoryAreaTypeId;
use crate::BUDDY_ALLOCATOR;
use crate::mem::page::pagealloc::PAGE_SIZE;
use crate::mem::page::pagemapper::{Mapper, EARLY_PAGE_TABLE_LIMIT, KERNEL_VMA};

static KERNEL_PML4: AtomicU64 = AtomicU64::new(0);

/// 建立新的内核页表，返回 PML4 物理地址
pub fn init_kernel_page_table(
    mem_map: &multiboot2::MemoryMapTag,
    kernel_start: usize,
    kernel_end: usize,
    bootdata_start: usize,
    bootdata_end: usize,
    mbi_start: usize,
    mbi_end: usize,
) -> PhysAddr {
    // 这里BUDDY_ALLOCATIR一定已经初始化，否则直接panic也应该
    let mut buddy_guard = BUDDY_ALLOCATOR.lock();
    let buddy = buddy_guard
        .as_mut()
        .expect("Incorrect order of init page table and buddy allocator.");
    // 分配新 PML4（必须在 1GB 以内，否则启动页表访问不到）
    let pml4_phys = PhysAddr::from(
        buddy.allocate_below(0, EARLY_PAGE_TABLE_LIMIT)
            .expect("no memory for PML4") as u64
    );
    KERNEL_PML4.store(pml4_phys.as_u64(), Ordering::Relaxed);

    unsafe {
        let pml4_virt = (pml4_phys.as_usize() + KERNEL_VMA) as *mut PageTable;
        (*pml4_virt).entries.fill(PageTableEntry::new());
    }

    let mut mapper = unsafe { Mapper::new(pml4_phys, buddy) };
    // 1. 映射所有可用物理内存到 Higher Half（0xFFFF800000000000 + phys）
    for area in mem_map.memory_areas() {
        if area.typ() == MemoryAreaTypeId::from(1) { // 可用内存
            let start = area.start_address() as usize;
            let end = area.end_address() as usize;

            for phys in (start..end).step_by(PAGE_SIZE) {
                let virt = VirtAddr::from((phys + KERNEL_VMA) as u64);
                let flags = EntryFlags::PRESENT | EntryFlags::WRITABLE | EntryFlags::NX;
                mapper.map(virt, PhysAddr::from(phys as u64), flags);
            }
        }
    }

    // 2. 精确映射内核区域
    set_range(&mut mapper, kernel_start + KERNEL_VMA, kernel_end + KERNEL_VMA,
              EntryFlags::PRESENT | EntryFlags::WRITABLE | EntryFlags::NX | EntryFlags::GLOBAL);

    set_identity_range(&mut mapper, bootdata_start, bootdata_end,
                       EntryFlags::PRESENT | EntryFlags::WRITABLE | EntryFlags::NX);
    set_identity_range(&mut mapper, mbi_start, mbi_end,
                       EntryFlags::PRESENT | EntryFlags::WRITABLE | EntryFlags::NX);

    set_range(&mut mapper, symbol_addr(core::ptr::addr_of!(_text_start )), symbol_addr(core::ptr::addr_of!(_text_end)),
              EntryFlags::PRESENT | EntryFlags::GLOBAL);

    set_range(&mut mapper, symbol_addr(core::ptr::addr_of!(_rodata_start)), symbol_addr(core::ptr::addr_of!(_rodata_end)),
          EntryFlags::PRESENT | EntryFlags::NX | EntryFlags::GLOBAL);

    set_range(&mut mapper, symbol_addr(core::ptr::addr_of!(_data_start)), symbol_addr(core::ptr::addr_of!(_data_end)),
          EntryFlags::PRESENT | EntryFlags::WRITABLE | EntryFlags::NX | EntryFlags::GLOBAL);

    set_range(&mut mapper, symbol_addr(core::ptr::addr_of!(_bss_start)), symbol_addr(core::ptr::addr_of!(_bss_end)),
              EntryFlags::PRESENT | EntryFlags::WRITABLE | EntryFlags::NX | EntryFlags::GLOBAL);
    // TODO: 映射帧缓冲区（通过 multiboot2 framebuffer_tag 获取）
    pml4_phys
}

/// 返回当前内核页表的 PML4 物理地址。
///
/// 调度器创建内核线程时直接复用这个 CR3；创建用户进程时，则用它作为模板复制
/// PML4 的高半区。这样用户进程拥有独立低半区地址空间，同时仍然能在陷入内核后
/// 访问同一份高半区内核映射。
pub fn kernel_pml4() -> PhysAddr {
    let pml4 = KERNEL_PML4.load(Ordering::Relaxed);
    assert!(pml4 != 0, "kernel page table is not initialized");
    PhysAddr::from(pml4)
}

/// 为用户进程创建一个新的 PML4。
///
/// x86_64 常见布局是：
/// - PML4[0..256)：用户低半区，每个进程独立；
/// - PML4[256..512)：内核高半区，所有进程共享。
///
/// 这里复制内核 PML4 的高 256 项，低 256 项清零。后续 `map_into_page_table`
/// 会只向这个新 PML4 的用户低半区添加用户代码、用户栈等映射。
pub fn create_user_page_table() -> PhysAddr {
    let kernel_pml4 = kernel_pml4();
    let mut buddy_guard = BUDDY_ALLOCATOR.lock();
    let buddy = buddy_guard
        .as_mut()
        .expect("Incorrect order of create_user_page_table and buddy allocator.");

    let user_pml4 = PhysAddr::from(
        buddy
            .allocate_below(0, EARLY_PAGE_TABLE_LIMIT)
            .expect("no memory for user PML4") as u64,
    );

    unsafe {
        let kernel = (kernel_pml4.as_usize() + KERNEL_VMA) as *const PageTable;
        let user = (user_pml4.as_usize() + KERNEL_VMA) as *mut PageTable;
        (*user).entries.fill(PageTableEntry::new());

        let mut index = 256;
        while index < 512 {
            (*user).entries[index] = (*kernel).entries[index];
            index += 1;
        }
    }

    user_pml4
}

/// 向指定 PML4 映射一页。
///
/// 这个函数是进程地址空间的基础接口。调用方显式传入要修改的 PML4，而不是隐式
/// 修改当前 CR3，所以它可以在创建进程时为“尚未运行”的用户进程布置地址空间。
pub fn map_into_page_table(pml4: PhysAddr, virt: VirtAddr, phys: PhysAddr, flags: EntryFlags) {
    let mut buddy_guard = BUDDY_ALLOCATOR.lock();
    let buddy = buddy_guard
        .as_mut()
        .expect("Incorrect order of map_into_page_table and buddy allocator.");
    let mut mapper = unsafe { Mapper::new(pml4, buddy) };
    mapper.map(virt, phys, flags);
}

/// 切换 CR3 到新页表
pub unsafe fn switch_cr3(pml4_phys: PhysAddr) {
    let cr3_val = pml4_phys.as_u64() & 0x000F_FFFF_FFFF_F000;
    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) cr3_val, options(nostack, preserves_flags));
    }
}

/// 刷新单个 TLB 条目
pub unsafe fn flush_tlb(virt: VirtAddr) {
    unsafe {
        core::arch::asm!("invlpg [{}]", in(reg) virt.as_u64(), options(nostack, preserves_flags));
    }
}
fn set_range(mapper: &mut Mapper, start: usize, end: usize, flags: EntryFlags) {
    let start = align_down(start, PAGE_SIZE);
    let end = align_up(end, PAGE_SIZE);
    for addr in (start..end).step_by(PAGE_SIZE) {
        let virt = VirtAddr::from(addr as u64);
        // 注意：这里需要知道虚拟地址对应的物理地址
        // 对于 Identity Map 或 Higher Half 恒等映射，phys = addr - KERNEL_VMA
        let phys = PhysAddr::from((addr - KERNEL_VMA) as u64);
        mapper.map(virt, phys, flags);
    }
}

fn set_identity_range(mapper: &mut Mapper, start: usize, end: usize, flags: EntryFlags) {
    let start = align_down(start, PAGE_SIZE);
    let end = align_up(end, PAGE_SIZE);
    for addr in (start..end).step_by(PAGE_SIZE) {
        mapper.map(VirtAddr::from(addr), PhysAddr::from(addr), flags);
    }
}

fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}

fn symbol_addr(symbol: *const usize) -> usize {
    symbol as usize
}
