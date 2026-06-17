use crate::mem::{MemoryAddress, PhysAddr, VirtAddr};
use crate::mem::page::{PageTable, PageTableEntry, EntryFlags};
use crate::mem::page::pagealloc::BuddyAllocator;

pub const KERNEL_VMA: usize = 0xFFFF800000000000;
pub const EARLY_PAGE_TABLE_LIMIT: usize = 1 << 30;

/// 页表映射器
pub struct Mapper<'a> {
    pml4: &'a mut PageTable,
    buddy: &'a mut BuddyAllocator,
}

impl<'a> Mapper<'a> {
    /// 从 PML4 物理地址创建 Mapper
    /// 注意：这里通过 KERNEL_VMA + phys 访问页表，要求该物理页在启动页表的 1GB 覆盖内
    pub unsafe fn new(pml4_phys: PhysAddr, buddy: &'a mut BuddyAllocator) -> Self {
        let pml4_virt = Self::phys_to_virt(pml4_phys);
        let pml4 = unsafe { &mut *(pml4_virt as *mut PageTable) };
        Self { pml4, buddy }
    }

    /// 物理地址 → 内核虚拟地址（Higher Half 偏移）
    fn phys_to_virt(phys: PhysAddr) -> usize {
        (phys.as_u64() as usize) + KERNEL_VMA
    }

    /// 映射一页：virt -> phys
    pub fn map(&mut self, virt: VirtAddr, phys: PhysAddr, flags: EntryFlags) {
        let v = virt.as_u64() as usize;
        let pml4_idx = (v >> 39) & 0x1FF;
        let pdpt_idx = (v >> 30) & 0x1FF;
        let pd_idx = (v >> 21) & 0x1FF;
        let pt_idx = (v >> 12) & 0x1FF;

        let pdpt = Mapper::<'a>::get_or_create_table(self.buddy, 
                                                     self.pml4.entry_mut(pml4_idx));
        let pd = Mapper::<'a>::get_or_create_table(self.buddy,
                                                   pdpt.entry_mut(pdpt_idx));
        let pt = Mapper::<'a>::get_or_create_table(self.buddy, 
                                                   pd.entry_mut(pd_idx));

        pt.entry_mut(pt_idx).set_addr(phys, flags | EntryFlags::PRESENT);
    }

    /// 获取下级页表，如果不存在则分配新页并清零
    fn get_or_create_table(buddy: &mut BuddyAllocator, entry: &mut PageTableEntry) 
        -> &'a mut PageTable {
        if !entry.is_present() {
            // 分配一个物理页作为新页表
            let frame = buddy.allocate_below(0, EARLY_PAGE_TABLE_LIMIT)
                .expect("out of memory for page table");
            let frame_phys = PhysAddr::from(frame as u64);

            // 设置上级 entry：指向新页表，可读写
            entry.set_addr(frame_phys, EntryFlags::PRESENT | EntryFlags::WRITABLE);

            // 清零新页表（必须！否则硬件读到垃圾数据会崩溃）
            let table_virt = Self::phys_to_virt(frame_phys) as *mut PageTable;
            unsafe { (*table_virt).entries.fill(PageTableEntry::new()) };
        }

        let table_phys = entry.addr();
        let table_virt = Self::phys_to_virt(table_phys) as *mut PageTable;
        unsafe { &mut *table_virt }
    }
}
