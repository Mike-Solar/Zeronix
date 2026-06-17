pub mod pagealloc;
pub mod pagemapper;

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
use crate::mem::page::pagealloc::{BuddyAllocator, PAGE_SIZE};
use crate::mem::page::pagemapper::{Mapper, EARLY_PAGE_TABLE_LIMIT, KERNEL_VMA};

/// 建立新的内核页表，返回 PML4 物理地址
pub fn init_kernel_page_table(
    buddy: &mut BuddyAllocator,
    mem_map: &multiboot2::MemoryMapTag,
    kernel_start: usize,
    kernel_end: usize,
    bootdata_start: usize,
    bootdata_end: usize,
    mbi_start: usize,
    mbi_end: usize,
) -> PhysAddr {
    // 分配新 PML4（必须在 1GB 以内，否则启动页表访问不到）
    let pml4_phys = PhysAddr::from(
        buddy.allocate_below(0, EARLY_PAGE_TABLE_LIMIT)
            .expect("no memory for PML4") as u64
    );

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

    set_range(&mut mapper, symbol_addr(core::ptr::addr_of!(_text_start)), symbol_addr(core::ptr::addr_of!(_text_end)),
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
