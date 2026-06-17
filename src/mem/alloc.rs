/// # 内存分配器
/// 使用空闲链表法的内核堆分配器。

use core::alloc::{GlobalAlloc, Layout};
use core::mem::{align_of, size_of};
use core::ptr::null_mut;

use crate::alloc_layout::{self, AllocationGeometry};
use crate::list::{list_empty, list_remove, ListNode};
use crate::lock::spin_lock::SpinLock;
use crate::mem::page::pagealloc::{MAX_ORDER, PAGE_SIZE};
use crate::mem::page::pagemapper::KERNEL_VMA;
use crate::{container_of_mut, BUDDY_ALLOCATOR};

const CHUNK_ORDER: usize = MAX_ORDER - 1;
const CHUNK_SIZE: usize = PAGE_SIZE * (1 << CHUNK_ORDER);
const BACK_PTR_SIZE: usize = size_of::<usize>();
const MIN_FREE_BLOCK_SIZE: usize = size_of::<HeapNode>() + BACK_PTR_SIZE + 1;

#[global_allocator]
pub static KERNEL_ALLOCATOR: LinkedListAllocator = LinkedListAllocator::new();

pub struct LinkedListAllocator {
    head: SpinLock<ListNode>,
}

unsafe impl GlobalAlloc for LinkedListAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let layout = normalize_layout(layout);
        let mut head_guard = self.head.lock();
        let head = &mut *head_guard as *mut ListNode;
        unsafe { ensure_head_initialized(head) };

        loop {
            if let Some(ptr) = unsafe { allocate_from_free_list(head, layout) } {
                return ptr;
            }

            let required = required_block_size(0, layout).unwrap_or(usize::MAX);
            if required > CHUNK_SIZE || unsafe { !grow_heap(head) } {
                return null_mut();
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        if ptr.is_null() {
            return;
        }

        let header = unsafe { header_from_user_ptr(ptr) };
        let mut head_guard = self.head.lock();
        let head = &mut *head_guard as *mut ListNode;
        unsafe {
            ensure_head_initialized(head);
            ListNode::init_at(core::ptr::addr_of_mut!((*header).list_node));
            insert_and_coalesce(head, header);
        }
    }
}

impl LinkedListAllocator {
    pub const fn new() -> Self {
        Self {
            head: SpinLock::new(ListNode::uninit()),
        }
    }
}

#[repr(C)]
pub struct HeapNode {
    pub size: usize,
    pub list_node: ListNode,
}

impl HeapNode {
    pub const fn new(size: usize) -> Self {
        Self {
            size,
            list_node: ListNode::uninit(),
        }
    }

    pub unsafe fn init_at(ptr: *mut Self, size: usize) {
        unsafe {
            ptr.write(Self::new(size));
            ListNode::init_at(core::ptr::addr_of_mut!((*ptr).list_node));
        }
    }
}

unsafe fn ensure_head_initialized(head: *mut ListNode) {
    unsafe {
        if (*head).next.is_null() || (*head).prev.is_null() {
            ListNode::init_at(head);
        }
    }
}

unsafe fn allocate_from_free_list(head: *mut ListNode, layout: Layout) -> Option<*mut u8> {
    unsafe {
        if list_empty(head) {
            return None;
        }

        let mut node = (*head).next;
        while node != head {
            let next = (*node).next;
            let block = container_of_mut!(node, list_node, HeapNode);
            let block_start = block as usize;
            let block_size = (*block).size;

            if let Some((user_ptr, used_size)) = allocation_for_block(block_start, block_size, layout) {
                list_remove(core::ptr::addr_of_mut!((*block).list_node));

                let remaining = block_size - used_size;
                if remaining >= MIN_FREE_BLOCK_SIZE {
                    let free_block = (block_start + used_size) as *mut HeapNode;
                    HeapNode::init_at(free_block, remaining);
                    insert_and_coalesce(head, free_block);
                }

                (*block).size = used_size;
                *((user_ptr - BACK_PTR_SIZE) as *mut usize) = block_start;
                return Some(user_ptr as *mut u8);
            }

            node = next;
        }

        None
    }
}

unsafe fn grow_heap(head: *mut ListNode) -> bool {
    let mut buddy_guard = BUDDY_ALLOCATOR.lock();
    let buddy = buddy_guard
        .as_mut()
        .expect("You must init buddy allocator before allocating heap memory");

    let phys = match buddy.allocate(CHUNK_ORDER) {
        Some(addr) => addr,
        None => return false,
    };

    let virt = phys + KERNEL_VMA;
    let block = virt as *mut HeapNode;
    unsafe {
        HeapNode::init_at(block, CHUNK_SIZE);
        insert_and_coalesce(head, block);
    }
    true
}

unsafe fn insert_and_coalesce(head: *mut ListNode, block: *mut HeapNode) {
    unsafe {
        let mut prev = head;
        let mut current = (*head).next;
        let block_addr = block as usize;

        while current != head {
            let current_block = container_of_mut!(current, list_node, HeapNode);
            if current_block as usize >= block_addr {
                break;
            }
            prev = current;
            current = (*current).next;
        }

        let node = core::ptr::addr_of_mut!((*block).list_node);
        (*node).prev = prev;
        (*node).next = current;
        (*prev).next = node;
        (*current).prev = node;

        let mut merged = block;

        if prev != head {
            let prev_block = container_of_mut!(prev, list_node, HeapNode);
            if blocks_are_adjacent(prev_block, merged) {
                (*prev_block).size += (*merged).size;
                list_remove(core::ptr::addr_of_mut!((*merged).list_node));
                merged = prev_block;
            }
        }

        let next = (*merged).list_node.next;
        if next != head {
            let next_block = container_of_mut!(next, list_node, HeapNode);
            if blocks_are_adjacent(merged, next_block) {
                (*merged).size += (*next_block).size;
                list_remove(core::ptr::addr_of_mut!((*next_block).list_node));
            }
        }
    }
}

fn blocks_are_adjacent(left: *const HeapNode, right: *const HeapNode) -> bool {
    unsafe { (left as usize).saturating_add((*left).size) == right as usize }
}

unsafe fn header_from_user_ptr(ptr: *mut u8) -> *mut HeapNode {
    unsafe {
        let back_ptr = (ptr as usize - BACK_PTR_SIZE) as *const usize;
        (*back_ptr) as *mut HeapNode
    }
}

fn normalize_layout(layout: Layout) -> Layout {
    alloc_layout::normalize_layout(layout, align_of::<usize>())
}

fn allocation_for_block(
    block_start: usize,
    block_size: usize,
    layout: Layout,
) -> Option<(usize, usize)> {
    let placement =
        alloc_layout::allocation_for_block(block_start, block_size, layout, allocation_geometry())?;
    Some((placement.user_ptr, placement.used_size))
}

fn required_block_size(block_start: usize, layout: Layout) -> Option<usize> {
    alloc_layout::required_block_size(block_start, layout, allocation_geometry())
}

fn allocation_geometry() -> AllocationGeometry {
    AllocationGeometry {
        header_size: size_of::<HeapNode>(),
        block_align: align_of::<HeapNode>(),
        back_ptr_size: BACK_PTR_SIZE,
    }
}
