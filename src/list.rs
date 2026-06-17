use core::ptr::null_mut;

pub struct ListNode{
    pub next: *mut ListNode,
    pub prev: *mut ListNode
}

impl ListNode{
    pub fn new() -> Self{
        let mut node = Self{next: null_mut(), prev: null_mut()};
        node.prev = &mut node as *mut ListNode;
        node.next = &mut node as *mut ListNode;
        node
    }

    pub fn init(&mut self){
        self.next = self;
        self.prev = self;
    }
}
#[macro_export]
macro_rules! container_of_mut {
    ($ptr:expr, $name:ident, $container:ty) => {
        unsafe {
            use core::mem::offset_of;
            let ptr = $ptr as *mut _ as usize;
            let offset = offset_of!($container, $name);
            (ptr - offset) as *mut $container
        }
    };
}
#[macro_export]
macro_rules! container_of {
    ($node_ptr:expr, $name:ident, $container:ty) => {
        unsafe {
            use core::mem::offset_of;
            let ptr = $node_ptr as *mut _ as usize;
            let offset = offset_of!($container, $name);
            (ptr - offset) as *const $container
        }
    };
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
        let head_ptr = $head as *mut ListNode;
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
        let head_ptr = $head as *mut ListNode;
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
        let head_ptr = $head as *mut ListNode;
        let mut $pos = container_of_mut!(unsafe { (*head_ptr).next }, $type, $member);
        while (&(*$pos).$member as *const ListNode) != head_ptr {
            $body
            $pos = container_of_mut!(unsafe { (*(*$pos).$member.next) }, $type, $member);
        }
    };
}

/// 安全遍历容器（允许删除）
///
/// 比上面多了一个 $n 保存下一个容器指针
#[macro_export]
macro_rules! list_for_each_entry_safe {
    ($pos:ident, $n:ident, $head:expr, $type:ty, $member:ident, $body:block) => {
        let head_ptr = $head as *mut ListNode;
        let mut $pos = container_of_mut!(unsafe { (*head_ptr).next }, $type, $member);
        while (&(*$pos).$member as *const ListNode) != head_ptr {
            let $n = container_of_mut!(unsafe { (*(*$pos).$member.next) }, $type, $member);
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
