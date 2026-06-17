/// # 内存分配器
/// 使用链表内存分配器

use crate::list::{list_append, list_empty, ListNode};
use core::alloc::{GlobalAlloc, Layout};

pub struct LinkedListAllocator {
    head: HeapNode
}

unsafe impl GlobalAlloc for LinkedListAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if list_empty(<*mut ListNode>::from(self.head.list_node)) {

        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {

    }
}

#[repr(C, packed)]
pub struct HeapNode{
    pub size: usize,
    pub list_node: ListNode,
}

impl HeapNode {
    pub fn new(size: usize) -> HeapNode {
        let mut node = HeapNode{
            size: size,
            list_node: ListNode::new()
        };
        node
    }

    pub fn append_to_list(&mut self, head: &HeapNode){
        unsafe { list_append(<*mut ListNode>::from(self.list_node), <*mut ListNode>::from(head.list_node)) }
    }

}