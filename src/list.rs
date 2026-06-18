/// 模仿Linux内核中的侵入式链表
use core::ptr::null_mut;

pub struct ListNode{
    pub next: *mut ListNode,
    pub prev: *mut ListNode
}

unsafe impl Send for ListNode {}

impl ListNode{
    pub const fn uninit() -> Self {
        Self {
            next: null_mut(),
            prev: null_mut(),
        }
    }

    pub fn new() -> Self{
        Self::uninit()
    }

    pub unsafe fn init_at(ptr: *mut Self) {
        unsafe {
            (*ptr).next = ptr;
            (*ptr).prev = ptr;
        }
    }

    pub fn init(&mut self){
        self.next = self;
        self.prev = self;
    }
}
#[macro_export]
macro_rules! container_of_mut {
    ($ptr:expr, $name:ident, $container:ty) => {
        {
            let ptr = $ptr as *mut _ as usize;
            let offset = core::mem::offset_of!($container, $name);
            (ptr - offset) as *mut $container
        }
    };
}
#[macro_export]
macro_rules! container_of {
    ($node_ptr:expr, $name:ident, $container:ty) => {
        {
            let ptr = $node_ptr as *mut _ as usize;
            let offset = core::mem::offset_of!($container, $name);
            (ptr - offset) as *const $container
        }
    };
}

///
///
///

pub unsafe fn list_empty(head: *mut ListNode) -> bool{
    if head.is_null() {
        return true;
    }
    unsafe {
        if (*head).prev.is_null() || (*head).next.is_null() {
            return true;
        }
        if (*head).next == head && (*head).prev == head{
            return true;
        }
    }
    return false;
}

/// 在两个已知节点之间插入新节点
///
/// Linux: __list_add(new, prev, next)
pub unsafe fn list_add(new: *mut ListNode, head: *mut ListNode){
    unsafe { _list_insert(new, head, (*head).next) };
}

/// 在尾部之前插入（相当于 push_back）
///
/// Linux: list_add_tail(new, head)
pub unsafe fn list_append(new: *mut ListNode, head: *mut ListNode){
    unsafe { _list_insert(new, (*head).prev, head) };
}

/// 删除节点并清空其指针（防止重复删除）
///
/// Linux: list_del_init(entry)
pub unsafe fn list_remove(old: *mut ListNode){
    unsafe {
        _list_remove((*old).prev, (*old).next);
        (*old).init()
    }
}

/// 移动节点到另一个链表的头部
///
/// Linux: list_move(entry, head)
pub unsafe fn list_move(entry: *mut ListNode, head: *mut ListNode) {
    unsafe {
        list_remove(entry);
        list_add(entry, head);
    }
}

/// 移动节点到另一个链表的尾部
///
/// Linux: list_move_tail(entry, head)
pub unsafe fn list_move_tail(entry: *mut ListNode, head: *mut ListNode) {
    unsafe {
        list_remove(entry);
        list_append(entry, head);
    }
}

/// 遍历链表（正向）
///
/// Linux: list_for_each(pos, head)
///
/// 用法：
/// 参数：($pos, $head, $body)
///
///     $pos：循环变量名，类型是 *mut ListNode
///     $head：链表头指针
///     $body：循环体代码块
///
#[macro_export]
macro_rules! list_for_each {
    ($pos:ident, $head:expr, $body:block) => {
        let head_ptr = $head as *mut $crate::list::ListNode;
        let mut $pos = unsafe { (*head_ptr).next };
        while $pos != head_ptr {
            $body
            $pos = unsafe { (*$pos).next };
        }
    };
}

/// 安全遍历（允许删除当前节点）
///
/// Linux: list_for_each_safe(pos, n, head)
///
/// 用法：
/// 参数：($pos, $n, $head, $body)
///
/// - $pos：当前节点指针
/// - $n：下一个节点指针（预存，防止删除后断链）
/// - $head：链表头
/// - $body：循环体
#[macro_export]
macro_rules! list_for_each_safe {
    ($pos:ident, $n:ident, $head:expr, $body:block) => {
        let head_ptr = $head as *mut $crate::list::ListNode;
        let mut $pos = unsafe { (*head_ptr).next };
        while $pos != head_ptr {
            let $n = unsafe { (*$pos).next };
            $body
            $pos = $n;
        }
    };
}

/// 遍历链表，直接拿到容器类型指针
///
/// Linux: list_for_each_entry(pos, head, member)
///
/// 用法：
/// 参数：($pos, $head, $type, $member, $body)
/// 
/// - $pos：循环变量名，类型是 *mut $type（比如 *mut Task）
/// - $head：链表头
/// - $type：容器结构体类型（如 Task）
/// - $member：该结构体里的 ListNode 字段名（如 node）
/// - $body：循环体
#[macro_export]
macro_rules! list_for_each_entry {
    ($pos:ident, $head:expr, $type:ty, $member:ident, $body:block) => {
        let head_ptr = $head as *mut $crate::list::ListNode;
        let mut $pos = container_of_mut!((*head_ptr).next, $member, $type);
        while (&(*$pos).$member as *const $crate::list::ListNode) != head_ptr {
            $body
            $pos = container_of_mut!((*$pos).$member.next, $member, $type);
        }
    };
}

/// 安全遍历容器（允许删除）
///
/// 比上面多了一个 $n 保存下一个容器指针
#[macro_export]
macro_rules! list_for_each_entry_safe {
    ($pos:ident, $n:ident, $head:expr, $type:ty, $member:ident, $body:block) => {
        let head_ptr = $head as *mut $crate::list::ListNode;
        let mut $pos = container_of_mut!((*head_ptr).next, $member, $type);
        while (&(*$pos).$member as *const $crate::list::ListNode) != head_ptr {
            let $n = container_of_mut!((*$pos).$member.next, $member, $type);
            $body
            $pos = $n;
        }
    };
}

unsafe fn _list_insert(new: *mut ListNode, prev: *mut ListNode, next: *mut ListNode){
    unsafe {
        (*prev).next = new;
        (*next).prev = new;
        (*new).prev = prev;
        (*new).next = next;
    }
}

unsafe fn _list_remove(prev: *mut ListNode, next: *mut ListNode){
    unsafe {
        (*prev).next = next;
        (*next).prev = prev;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Entry {
        value: u32,
        node: ListNode,
    }

    impl Entry {
        fn new(value: u32) -> Self {
            Self {
                value,
                node: ListNode::uninit(),
            }
        }
    }

    #[test]
    fn empty_list_head_is_empty() {
        let mut head = ListNode::uninit();
        unsafe { ListNode::init_at(&mut head) };

        assert!(unsafe { list_empty(&mut head) });
    }

    #[test]
    fn single_insert_makes_list_non_empty_then_remove_restores_empty() {
        let mut head = ListNode::uninit();
        let mut node = ListNode::uninit();
        unsafe {
            ListNode::init_at(&mut head);
            ListNode::init_at(&mut node);
            list_append(&mut node, &mut head);

            assert!(!list_empty(&mut head));
            assert_eq!(head.next, &mut node as *mut _);
            assert_eq!(head.prev, &mut node as *mut _);

            list_remove(&mut node);
            let node_ptr = &raw mut node;
            assert!(list_empty(&mut head));
            assert_eq!(node.next, node_ptr);
            assert_eq!(node.prev, node_ptr);
        }
    }

    #[test]
    fn appends_preserve_order() {
        let mut head = ListNode::uninit();
        let mut a = ListNode::uninit();
        let mut b = ListNode::uninit();
        unsafe {
            ListNode::init_at(&mut head);
            ListNode::init_at(&mut a);
            ListNode::init_at(&mut b);
            list_append(&mut a, &mut head);
            list_append(&mut b, &mut head);

            assert_eq!(head.next, &mut a as *mut _);
            assert_eq!(a.next, &mut b as *mut _);
            assert_eq!(b.next, &mut head as *mut _);
            assert_eq!(head.prev, &mut b as *mut _);
        }
    }

    #[test]
    fn entry_iteration_returns_containing_structs() {
        let mut head = ListNode::uninit();
        let mut a = Entry::new(1);
        let mut b = Entry::new(2);
        let mut values = alloc::vec::Vec::new();

        unsafe {
            ListNode::init_at(&mut head);
            ListNode::init_at(&mut a.node);
            ListNode::init_at(&mut b.node);
            list_append(&mut a.node, &mut head);
            list_append(&mut b.node, &mut head);

            crate::list_for_each_entry!(entry, &mut head, Entry, node, {
                values.push((*entry).value);
            });
        }

        assert_eq!(values, alloc::vec![1, 2]);
    }

    #[test]
    fn safe_entry_iteration_allows_removing_current() {
        let mut head = ListNode::uninit();
        let mut a = Entry::new(1);
        let mut b = Entry::new(2);
        let mut values = alloc::vec::Vec::new();

        unsafe {
            ListNode::init_at(&mut head);
            ListNode::init_at(&mut a.node);
            ListNode::init_at(&mut b.node);
            list_append(&mut a.node, &mut head);
            list_append(&mut b.node, &mut head);

            crate::list_for_each_entry_safe!(entry, next, &mut head, Entry, node, {
                values.push((*entry).value);
                list_remove(&mut (*entry).node);
                let _ = next;
            });
        }

        assert_eq!(values, alloc::vec![1, 2]);
        assert!(unsafe { list_empty(&mut head) });
    }
}
