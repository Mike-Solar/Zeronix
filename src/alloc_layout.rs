use core::alloc::Layout;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationGeometry {
    pub header_size: usize,
    pub block_align: usize,
    pub back_ptr_size: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationPlacement {
    pub user_ptr: usize,
    pub used_size: usize,
}

pub fn normalize_layout(layout: Layout, min_align: usize) -> Layout {
    let size = layout.size().max(1);
    let align = layout.align().max(min_align);
    Layout::from_size_align(size, align).expect("normalized layout must be valid")
}

pub fn allocation_for_block(
    block_start: usize,
    block_size: usize,
    layout: Layout,
    geometry: AllocationGeometry,
) -> Option<AllocationPlacement> {
    let user_ptr = user_ptr_for_block(block_start, layout, geometry)?;
    let allocation_end = user_ptr.checked_add(layout.size())?;
    let used_end = align_up(allocation_end, geometry.block_align)?;
    let used_size = used_end.checked_sub(block_start)?;

    if used_size <= block_size {
        Some(AllocationPlacement {
            user_ptr,
            used_size,
        })
    } else {
        None
    }
}

pub fn required_block_size(
    block_start: usize,
    layout: Layout,
    geometry: AllocationGeometry,
) -> Option<usize> {
    let user_ptr = user_ptr_for_block(block_start, layout, geometry)?;
    let allocation_end = user_ptr.checked_add(layout.size())?;
    align_up(allocation_end, geometry.block_align)?.checked_sub(block_start)
}

pub fn user_ptr_for_block(
    block_start: usize,
    layout: Layout,
    geometry: AllocationGeometry,
) -> Option<usize> {
    let after_header = block_start.checked_add(geometry.header_size)?;
    let after_back_ptr = after_header.checked_add(geometry.back_ptr_size)?;
    align_up(after_back_ptr, layout.align())
}

pub fn align_up(addr: usize, align: usize) -> Option<usize> {
    debug_assert!(align.is_power_of_two());
    let mask = align.checked_sub(1)?;
    addr.checked_add(mask).map(|v| v & !mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEOMETRY: AllocationGeometry = AllocationGeometry {
        header_size: 24,
        block_align: 8,
        back_ptr_size: 8,
    };

    #[test]
    fn aligns_user_pointer_to_requested_layout() {
        let layout = Layout::from_size_align(13, 64).unwrap();
        let placement = allocation_for_block(0x1003, 512, layout, GEOMETRY).unwrap();

        assert_eq!(placement.user_ptr % 64, 0);
        assert!(placement.used_size >= GEOMETRY.header_size + GEOMETRY.back_ptr_size + 13);
        assert_eq!((0x1003 + placement.used_size) % GEOMETRY.block_align, 0);
    }

    #[test]
    fn reserves_space_for_back_pointer_before_user_memory() {
        let layout = Layout::from_size_align(1, 8).unwrap();
        let placement = allocation_for_block(0x1000, 128, layout, GEOMETRY).unwrap();

        assert!(placement.user_ptr >= 0x1000 + GEOMETRY.header_size + GEOMETRY.back_ptr_size);
        assert_eq!((placement.user_ptr - GEOMETRY.back_ptr_size) % GEOMETRY.back_ptr_size, 0);
    }

    #[test]
    fn rejects_blocks_that_are_too_small() {
        let layout = Layout::from_size_align(128, 16).unwrap();
        assert!(allocation_for_block(0x1000, 32, layout, GEOMETRY).is_none());
    }

    #[test]
    fn detects_overflow() {
        let layout = Layout::from_size_align(16, 8).unwrap();
        assert!(allocation_for_block(usize::MAX - 4, 128, layout, GEOMETRY).is_none());
    }
}
