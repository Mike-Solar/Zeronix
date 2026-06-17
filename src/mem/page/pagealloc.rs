use multiboot2::MemoryAreaTypeId;

pub const PAGE_SIZE: usize = 4096;
pub const PAGE_SHIFT: usize = 12;
pub const MAX_ORDER: usize = 11; // 0~10，最大 4MB
/// 压缩版元数据：每个页框 8 字节
#[derive(Clone, Copy)]
pub struct PageInfo {
    pub order: u8, // 0~255
    pub flags: u8, // bit0=is_head, bit1=is_free
    pub next: u32, // PFN+1，0 表示 None
    pub prev: u32, // PFN+1，0 表示 None
}

impl PageInfo {
    pub(crate) const NONE: u32 = 0;
    fn next(&self) -> Option<usize> {
        if self.next == Self::NONE {
            None
        } else {
            Some((self.next - 1) as usize)
        }
    }
    fn set_next(&mut self, v: Option<usize>) {
        self.next = v.map(|x| (x + 1) as u32).unwrap_or(Self::NONE);
    }
    fn prev(&self) -> Option<usize> {
        if self.prev == Self::NONE {
            None
        } else {
            Some((self.prev - 1) as usize)
        }
    }
    fn set_prev(&mut self, v: Option<usize>) {
        self.prev = v.map(|x| (x + 1) as u32).unwrap_or(Self::NONE);
    }
    fn is_head(&self) -> bool {
        self.flags & 1 != 0
    }
    fn set_head(&mut self, v: bool) {
        if v { self.flags |= 1 } else { self.flags &= !1 }
    }
    fn is_free(&self) -> bool {
        self.flags & 2 != 0
    }
    fn set_free(&mut self, v: bool) {
        if v { self.flags |= 2 } else { self.flags &= !2 }
    }
}

impl Default for PageInfo {
    fn default() -> Self {
        Self {
            order: 255,
            flags: 0,
            next: PageInfo::NONE,
            prev: PageInfo::NONE,
        }
    }
}

pub struct BuddyAllocator {
    metadata: &'static mut [PageInfo],
    free_heads: [Option<usize>; MAX_ORDER],
    total_pages: usize,
}

impl BuddyAllocator {
    pub fn new(
        metadata: &'static mut [PageInfo],
        mem_map: &multiboot2::MemoryMapTag,
        reserved: &[(usize, usize)],
    ) -> Self {
        let total_pages = metadata.len();
        for info in metadata.iter_mut() {
            *info = PageInfo {
                order: 255,
                flags: 0,
                next: PageInfo::NONE,
                prev: PageInfo::NONE,
            };
        }
        let mut buddy = Self {
            metadata,
            free_heads: [None; MAX_ORDER],
            total_pages,
        };
        for area in mem_map.memory_areas() {
            if area.typ() == MemoryAreaTypeId::from(1) {
                // 可用内存
                let start = area.start_address() as usize;
                let end = area.end_address() as usize;
                buddy.add_region(start, end, reserved);
            }
        }
        buddy
    }

    fn add_region(&mut self, mut start: usize, end: usize, reserved: &[(usize, usize)]) {
        start = align_up(start, PAGE_SIZE);
        let end = align_down(end, PAGE_SIZE);

        while start < end {
            // 如果 start 落在某个 reserved 区域内，直接跳到区域末尾之后
            let mut in_reserved = false;
            for &(res_start, res_end) in reserved {
                if start >= res_start && start < res_end {
                    start = align_up(res_end, PAGE_SIZE);
                    in_reserved = true;
                    break;
                }
            }
            if in_reserved || start >= end {
                continue;
            }

            // 找到下一个 reserved 边界，限制当前块不能跨越它
            let next_boundary = reserved
                .iter()
                .filter(|(s, _)| *s > start)
                .map(|(s, _)| *s)
                .min()
                .unwrap_or(end)
                .min(end);

            let size = next_boundary - start;
            let pages = size >> PAGE_SHIFT;
            if pages == 0 {
                start = start.saturating_add(PAGE_SIZE);
                continue;
            }

            // 计算当前地址能支持的最大 order
            let align_order = start.trailing_zeros() as usize;
            let order_by_align = align_order.saturating_sub(PAGE_SHIFT);
            let order_by_size = (usize::BITS as usize - 1 - pages.leading_zeros() as usize)
                .min(MAX_ORDER - 1);
            let order = order_by_align.min(order_by_size).min(MAX_ORDER - 1);

            let block_pages = 1usize << order;
            let block_size = block_pages * PAGE_SIZE;
            let pfn = start >> PAGE_SHIFT;
            if pfn + block_pages > self.total_pages {
                break;
            }

            // 标记块内所有页
            for i in 0..block_pages {
                let p = &mut self.metadata[pfn + i];
                p.order = order as u8;
                p.set_head(i == 0);
                p.set_free(true);
                p.set_next(None);
                p.set_prev(None);
            }
            self.push_free(pfn, order);

            start += block_size;
        }
    }

    fn push_free(&mut self, pfn: usize, order: usize) {
        let head = self.free_heads[order];
        let p = &mut self.metadata[pfn];
        p.set_free(true);
        p.set_next(head);
        p.set_prev(None);
        if let Some(old) = head {
            self.metadata[old].set_prev(Some(pfn));
        }
        self.free_heads[order] = Some(pfn);
    }

    /// 分配 2^order 个页，返回物理地址
    pub fn allocate(&mut self, order: usize) -> Option<usize> {
        self.allocate_where(order, |_| true)
    }

    /// 分配 2^order 个页，且整块物理地址必须低于 limit。
    pub fn allocate_below(&mut self, order: usize, limit: usize) -> Option<usize> {
        self.allocate_where(order, |pfn| {
            let start = pfn * PAGE_SIZE;
            let size = (1usize << order) * PAGE_SIZE;
            start.checked_add(size).is_some_and(|end| end <= limit)
        })
    }

    fn allocate_where(
        &mut self,
        order: usize,
        mut pred: impl FnMut(usize) -> bool,
    ) -> Option<usize> {
        if order >= MAX_ORDER {
            return None;
        }
        for o in order..MAX_ORDER {
            if let Some(pfn) = self.find_free(o, &mut pred) {
                self.remove_from_free(pfn, o);
                self.metadata[pfn].set_free(false);
                self.metadata[pfn].set_next(None);
                self.metadata[pfn].set_prev(None);
                // 分裂大块
                let mut split = o;
                while split > order {
                    split -= 1;
                    let buddy_pfn = pfn ^ (1 << split);
                    for i in 0..(1 << split) {
                        let p = &mut self.metadata[buddy_pfn + i];
                        p.order = split as u8;
                        p.set_head(i == 0);
                        p.set_free(true);
                    }
                    self.push_free(buddy_pfn, split);
                }
                // 标记最终分配块
                for i in 0..(1 << order) {
                    let p = &mut self.metadata[pfn + i];
                    p.order = order as u8;
                    p.set_free(false);
                    p.set_head(i == 0);
                }
                return Some(pfn * PAGE_SIZE);
            }
        }
        None
    }

    fn find_free(
        &self,
        order: usize,
        pred: &mut impl FnMut(usize) -> bool,
    ) -> Option<usize> {
        let mut current = self.free_heads[order];
        while let Some(pfn) = current {
            if pred(pfn) {
                return Some(pfn);
            }
            current = self.metadata[pfn].next();
        }
        None
    }

    fn pop_free(&mut self, order: usize) -> Option<usize> {
        let pfn = self.free_heads[order]?;
        let next = self.metadata[pfn].next();
        self.free_heads[order] = next;
        if let Some(n) = next {
            self.metadata[n].set_prev(None);
        }
        let p = &mut self.metadata[pfn];
        p.set_free(false);
        p.set_next(None);
        p.set_prev(None);
        Some(pfn)
    }

    pub fn deallocate(&mut self, addr: usize) {
        let pfn = addr / PAGE_SIZE;
        let mut order = self.metadata[pfn].order as usize;
        let mut current_pfn = pfn;

        assert!(self.metadata[pfn].is_head(), "must free block head");
        assert!(!self.metadata[pfn].is_free(), "double free");

        // 向上合并
        loop {
            if order + 1 >= MAX_ORDER {
                break;
            }
            let buddy_pfn = current_pfn ^ (1 << order);
            if buddy_pfn >= self.total_pages {
                break;
            }

            let buddy = &self.metadata[buddy_pfn];
            if !buddy.is_free() || buddy.order as usize != order || !buddy.is_head() {
                break;
            }

            self.remove_from_free(buddy_pfn, order);
            current_pfn = core::cmp::min(current_pfn, buddy_pfn);
            order += 1;
        }

        let block_pages = 1 << order;
        for i in 0..block_pages {
            let p = &mut self.metadata[current_pfn + i];
            p.order = order as u8;
            p.set_free(true);
            p.set_head(i == 0);
        }
        self.push_free(current_pfn, order);
    }

    fn remove_from_free(&mut self, pfn: usize, order: usize) {
        let node = &self.metadata[pfn];
        let prev = node.prev();
        let next = node.next();
        if let Some(p) = prev {
            self.metadata[p].set_next(next);
        } else {
            self.free_heads[order] = next;
        }
        if let Some(n) = next {
            self.metadata[n].set_prev(prev);
        }
    }
}

fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}
fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}
