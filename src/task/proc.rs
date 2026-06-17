use alloc::boxed::Box;
use alloc::collections::VecDeque;
use core::arch::global_asm;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::lock::spin_lock::SpinLock;
use crate::mem::{PhysAddr, VirtAddr};
use crate::mem::page::{self, EntryFlags};
use crate::mem::page::pagemapper::KERNEL_VMA;
use crate::mem::page::pagealloc::PAGE_SIZE;
use crate::task::proc_layout::{
    align_down, consume_tick, user_code_base, user_stack_top, DEFAULT_TIME_SLICE_TICKS,
};
use crate::trap;
use crate::trap::gdt::{self, USER_CODE_SELECTOR, USER_DATA_SELECTOR};
use crate::user::elf;
use crate::BUDDY_ALLOCATOR;

unsafe extern "C" {
    static _bootdata_start: usize;
    static _bootdata_end: usize;
}

const KERNEL_STACK_ORDER: usize = 2;
const KERNEL_STACK_SIZE: usize = (1 << KERNEL_STACK_ORDER) * PAGE_SIZE;
const USER_STACK_ORDER: usize = 1;
const USER_STACK_SIZE: usize = (1 << USER_STACK_ORDER) * PAGE_SIZE;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProcState {
    Ready,
    Running,
    Blocked,
    Exited,
}

#[derive(Clone, Copy)]
pub struct ProcRef(*mut Proc);

unsafe impl Send for ProcRef {}

impl ProcRef {
    fn as_ptr(self) -> *mut Proc {
        self.0
    }

    pub fn pid(self) -> u64 {
        unsafe { (*self.as_ptr()).pid }
    }
}

pub struct Proc {
    pub pid: u64,
    pub state: ProcState,
    pub context: Context,
    pub pagetable: PhysAddr,
    pub kstack_phys: usize,
    pub kstack_top: usize,
    pub kind: ProcKind,
    pub ticks_left: u32,
    entry: Option<KernelEntry>,
    trap_frame: Option<*mut TrapFrame>,
    user_code_phys: Option<usize>,
    user_stack_phys: Option<usize>,
}

pub type KernelEntry = fn() -> !;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProcKind {
    Kernel,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnError {
    BadElf,
    ProgramTooLarge,
    NoMemory,
}

static READY_QUEUE: SpinLock<VecDeque<ProcRef>> = SpinLock::new(VecDeque::new());
static NEXT_PID: AtomicU64 = AtomicU64::new(1);
static mut CURRENT: Option<ProcRef> = None;

unsafe extern "C" {
    fn __switch_context(old: *mut Context, new: *const Context);
    fn __enter_user(tf: *const TrapFrame) -> !;
}

pub fn init() {
    unsafe {
        if read_current().is_some() {
            return;
        }

        let boot_proc = Box::leak(Box::new(Proc {
            pid: 0,
            state: ProcState::Running,
            context: Context::new(),
            pagetable: page::kernel_pml4(),
            kstack_phys: 0,
            kstack_top: gdt::ring0_stack_top(),
            kind: ProcKind::Kernel,
            ticks_left: DEFAULT_TIME_SLICE_TICKS,
            entry: None,
            trap_frame: None,
            user_code_phys: None,
            user_stack_phys: None,
        }));

        write_current(Some(ProcRef(boot_proc as *mut Proc)));
    }
}

pub fn spawn_kernel(entry: KernelEntry) -> ProcRef {
    init();
    let pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);

    let mut buddy_guard = BUDDY_ALLOCATOR.lock();
    let buddy = buddy_guard
        .as_mut()
        .expect("You must initialize buddy allocator before spawning a kernel task.");

    let kstack_phys = buddy
        .allocate(KERNEL_STACK_ORDER)
        .expect("no memory for kernel task stack");
    drop(buddy_guard);

    let kstack_top = kstack_phys + KERNEL_VMA + KERNEL_STACK_SIZE;
    let proc = Box::leak(Box::new(Proc {
        pid,
        state: ProcState::Ready,
        context: Context {
            rsp: align_down(kstack_top, 16) as u64,
            rip: kernel_task_bootstrap as *const () as u64,
            ..Context::new()
        },
        pagetable: page::kernel_pml4(),
        kstack_phys,
        kstack_top,
        kind: ProcKind::Kernel,
        ticks_left: DEFAULT_TIME_SLICE_TICKS,
        entry: Some(entry),
        trap_frame: None,
        user_code_phys: None,
        user_stack_phys: None,
    }));

    let proc_ref = ProcRef(proc as *mut Proc);
    push_ready(proc_ref);
    proc_ref
}

/// 用一段机器码创建一个很小的 Ring 3 用户进程。
///
/// - 每个用户进程都有独立 PML4，低半区用户地址空间互相隔离；
/// - PML4 高半区复制内核映射，陷入内核后仍能访问内核代码、数据、堆和栈；
/// - 每个用户进程分配一页代码页和若干用户栈页；
/// - 用户页必须带 `USER` 标志，否则 Ring 3 访问会触发 #PF；
/// - 第一次被调度时，内核线程入口会根据内核栈上的 `TrapFrame` 执行 `iretq`，
///   由 CPU 完成 CPL=0 到 CPL=3 的权限级切换。
///
/// 最小烟测程序是 `[0xeb, 0xfe]`，即 `jmp $` 无限循环。它不会主动系统调用，
/// 但 PIT 定时器仍应能从 Ring 3 抢占它，并通过 TSS.rsp0 切回对应进程的内核栈。
pub fn spawn_user(code: &[u8]) -> ProcRef {
    init();
    assert!(code.len() <= PAGE_SIZE, "user loader supports one code page");
    let pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);
    let user_code_base = user_code_base(pid);
    let user_stack_top = user_stack_top(pid);
    let pagetable = page::create_user_page_table();
    map_bootdata_identity(pagetable);

    let (kstack_phys, code_phys, ustack_phys) = {
        let mut buddy_guard = BUDDY_ALLOCATOR.lock();
        let buddy = buddy_guard
            .as_mut()
            .expect("You must initialize buddy allocator before spawning a user task.");

        let kstack_phys = buddy
            .allocate(KERNEL_STACK_ORDER)
            .expect("no memory for user task kernel stack");
        let code_phys = buddy
            .allocate(0)
            .expect("no memory for user code page");
        let ustack_phys = buddy
            .allocate(USER_STACK_ORDER)
            .expect("no memory for user stack");

        (kstack_phys, code_phys, ustack_phys)
    };

    unsafe {
        let dst = (code_phys + KERNEL_VMA) as *mut u8;
        core::ptr::write_bytes(dst, 0, PAGE_SIZE);
        if code.is_empty() {
            // 空程序直接执行零字节没有意义。这里放一个 `jmp $`，让进程
            // 稳定停在用户态，方便验证 Ring 3 入口和时钟抢占。
            *dst.add(0) = 0xeb;
            *dst.add(1) = 0xfe;
        } else {
            core::ptr::copy_nonoverlapping(code.as_ptr(), dst, code.len());
        }
    }

    page::map_into_page_table(
        pagetable,
        VirtAddr::from(user_code_base),
        PhysAddr::from(code_phys as u64),
        EntryFlags::PRESENT | EntryFlags::USER,
    );

    let user_stack_bottom = user_stack_top - USER_STACK_SIZE;
    for i in 0..(1 << USER_STACK_ORDER) {
        map_user_stack_page(
            pagetable,
            user_stack_bottom + i * PAGE_SIZE,
            ustack_phys + i * PAGE_SIZE,
        );
    }

    let kstack_top = kstack_phys + KERNEL_VMA + KERNEL_STACK_SIZE;
    let trap_frame = (kstack_top - core::mem::size_of::<TrapFrame>()) as *mut TrapFrame;
    unsafe {
        trap_frame.write(TrapFrame::new_user(
            user_code_base as u64,
            user_stack_top as u64,
        ));
    }

    let proc = Box::leak(Box::new(Proc {
        pid,
        state: ProcState::Ready,
        context: Context {
            rsp: align_down(trap_frame as usize, 16) as u64,
            rip: user_task_bootstrap as *const () as u64,
            ..Context::new()
        },
        pagetable,
        kstack_phys,
        kstack_top,
        kind: ProcKind::User,
        ticks_left: DEFAULT_TIME_SLICE_TICKS,
        entry: None,
        trap_frame: Some(trap_frame),
        user_code_phys: Some(code_phys),
        user_stack_phys: Some(ustack_phys),
    }));

    let proc_ref = ProcRef(proc as *mut Proc);
    push_ready(proc_ref);
    proc_ref
}

pub fn spawn_elf(image: &[u8]) -> Result<ProcRef, SpawnError> {
    init();
    let parsed = elf::parse(image).map_err(|_| SpawnError::BadElf)?;
    if parsed.load.file_size > PAGE_SIZE || parsed.load.mem_size > PAGE_SIZE {
        return Err(SpawnError::ProgramTooLarge);
    }

    let pid = NEXT_PID.fetch_add(1, Ordering::Relaxed);
    let user_stack_top = user_stack_top(pid);
    let pagetable = page::create_user_page_table();
    map_bootdata_identity(pagetable);

    let (kstack_phys, code_phys, ustack_phys) = {
        let mut buddy_guard = BUDDY_ALLOCATOR.lock();
        let buddy = buddy_guard
            .as_mut()
            .expect("You must initialize buddy allocator before spawning a user task.");

        let kstack_phys = buddy.allocate(KERNEL_STACK_ORDER).ok_or(SpawnError::NoMemory)?;
        let code_phys = buddy.allocate(0).ok_or(SpawnError::NoMemory)?;
        let ustack_phys = buddy.allocate(USER_STACK_ORDER).ok_or(SpawnError::NoMemory)?;

        (kstack_phys, code_phys, ustack_phys)
    };

    unsafe {
        let dst = (code_phys + KERNEL_VMA) as *mut u8;
        core::ptr::write_bytes(dst, 0, PAGE_SIZE);
        core::ptr::copy_nonoverlapping(
            image[parsed.load.offset..].as_ptr(),
            dst,
            parsed.load.file_size,
        );
    }

    page::map_into_page_table(
        pagetable,
        VirtAddr::from(parsed.load.virt_addr),
        PhysAddr::from(code_phys as u64),
        EntryFlags::PRESENT | EntryFlags::USER,
    );

    map_user_stack(pagetable, user_stack_top, ustack_phys);

    let kstack_top = kstack_phys + KERNEL_VMA + KERNEL_STACK_SIZE;
    let trap_frame = (kstack_top - core::mem::size_of::<TrapFrame>()) as *mut TrapFrame;
    unsafe {
        trap_frame.write(TrapFrame::new_user(parsed.entry, user_stack_top as u64));
    }

    let proc = Box::leak(Box::new(Proc {
        pid,
        state: ProcState::Ready,
        context: Context {
            rsp: align_down(trap_frame as usize, 16) as u64,
            rip: user_task_bootstrap as *const () as u64,
            ..Context::new()
        },
        pagetable,
        kstack_phys,
        kstack_top,
        kind: ProcKind::User,
        ticks_left: DEFAULT_TIME_SLICE_TICKS,
        entry: None,
        trap_frame: Some(trap_frame),
        user_code_phys: Some(code_phys),
        user_stack_phys: Some(ustack_phys),
    }));

    let proc_ref = ProcRef(proc as *mut Proc);
    push_ready(proc_ref);
    Ok(proc_ref)
}

pub fn current_proc() -> ProcRef {
    unsafe { read_current().expect("no current process; call task::proc::init first") }
}

pub fn yield_now() {
    unsafe { schedule() }
}

pub fn exit_current(_status: usize) -> ! {
    unsafe {
        trap::disable_interrupts();
        if let Some(current) = read_current() {
            (*current.as_ptr()).state = ProcState::Exited;
        }
        schedule();
        trap::enable_interrupts();
        loop {
            trap::halt();
        }
    }
}

pub fn on_timer_tick() {
    unsafe {
        let Some(current) = read_current() else {
            return;
        };

        let proc = &mut *current.as_ptr();
        if consume_tick(&mut proc.ticks_left, DEFAULT_TIME_SLICE_TICKS) {
            schedule();
        }
    }
}

fn push_ready(proc_ref: ProcRef) {
    unsafe {
        (*proc_ref.as_ptr()).state = ProcState::Ready;
    }
    READY_QUEUE.lock().push_back(proc_ref);
}

fn map_user_stack(pagetable: PhysAddr, stack_top: usize, stack_phys: usize) {
    let stack_bottom = stack_top - USER_STACK_SIZE;
    for i in 0..(1 << USER_STACK_ORDER) {
        map_user_stack_page(
            pagetable,
            stack_bottom + i * PAGE_SIZE,
            stack_phys + i * PAGE_SIZE,
        );
    }
}

fn map_user_stack_page(pagetable: PhysAddr, virt: usize, phys: usize) {
    page::map_into_page_table(
        pagetable,
        VirtAddr::from(virt as u64),
        PhysAddr::from(phys as u64),
        EntryFlags::PRESENT | EntryFlags::WRITABLE | EntryFlags::USER | EntryFlags::NX,
    );
}

fn map_bootdata_identity(pagetable: PhysAddr) {
    let bootdata_start = core::ptr::addr_of!(_bootdata_start) as usize;
    let bootdata_end = core::ptr::addr_of!(_bootdata_end) as usize;
    let start = align_down(bootdata_start, PAGE_SIZE);
    let end = (bootdata_end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

    // 当前内核从 32-bit/long-mode 启动栈进入 Rust，早期还没有主动把 RSP
    // 切到高半区内核栈。第一次时钟中断触发调度时，`schedule()` 会先在
    // 这段低地址 bootdata 栈上执行 `switch_cr3()`，然后才切到目标进程的
    // 内核栈。如果用户页表没有保留这段 supervisor identity mapping，
    // `mov cr3` 后紧跟着的 `ret` 会因为读低地址栈而 #PF，随后升级成 #DF。
    //
    // 这里映射的是 supervisor 页：最终 PTE 不带 USER 位。即使同一个低半区
    // PML4 分支稍后因为用户代码/栈映射而把中间页表项提升为 USER，CPU 仍会
    // 在最终 PTE 拒绝 Ring 3 访问这段 bootdata。
    for addr in (start..end).step_by(PAGE_SIZE) {
        page::map_into_page_table(
            pagetable,
            VirtAddr::from(addr as u64),
            PhysAddr::from(addr as u64),
            EntryFlags::PRESENT | EntryFlags::WRITABLE | EntryFlags::NX,
        );
    }
}

unsafe fn schedule() {
    let interrupts_were_enabled = trap::interrupts_enabled();
    unsafe { trap::disable_interrupts(); }

    let next = {
        let mut queue = READY_QUEUE.lock();
        queue.pop_front()
    };

    let Some(next) = next else {
        if interrupts_were_enabled {
            unsafe { trap::enable_interrupts(); }
        }
        return;
    };

    let old = current_proc();
    if old.as_ptr() == next.as_ptr() {
        if interrupts_were_enabled {
            unsafe { trap::enable_interrupts(); }
        }
        return;
    }

    unsafe {
        if (*old.as_ptr()).state == ProcState::Running {
            (*old.as_ptr()).state = ProcState::Ready;
            READY_QUEUE.lock().push_back(old);
        }

        (*next.as_ptr()).state = ProcState::Running;
        (*next.as_ptr()).ticks_left = DEFAULT_TIME_SLICE_TICKS;
        write_current(Some(next));
        gdt::set_kernel_stack((*next.as_ptr()).kstack_top);
        page::switch_cr3((*next.as_ptr()).pagetable);

        __switch_context(
            core::ptr::addr_of_mut!((*old.as_ptr()).context),
            core::ptr::addr_of!((*next.as_ptr()).context),
        );
    }

    if interrupts_were_enabled {
        unsafe { trap::enable_interrupts(); }
    }
}

extern "C" fn kernel_task_bootstrap() -> ! {
    let proc_ref = current_proc();
    let entry = unsafe {
        (*proc_ref.as_ptr())
            .entry
            .expect("kernel task missing entry point")
    };

    unsafe { trap::enable_interrupts(); }
    entry()
}

extern "C" fn user_task_bootstrap() -> ! {
    let proc_ref = current_proc();
    let tf = unsafe {
        (*proc_ref.as_ptr())
            .trap_frame
            .expect("user task missing trap frame")
    };

    unsafe { __enter_user(tf) }
}

#[repr(C)]
pub struct Context {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub rip: u64,
}

/// `__enter_user` 消费的软件栈帧。
///
/// 前 15 个字段是手动恢复的通用寄存器。最后 5 个字段必须严格符合 `iretq`
/// 从 Ring 0 返回到 Ring 3 时弹栈的顺序：RIP、CS、RFLAGS、RSP、SS。
/// 如果顺序错了，通常会立刻 #GP 或 triple fault。
#[repr(C)]
pub struct TrapFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl TrapFrame {
    pub const fn new_user(rip: u64, rsp: u64) -> Self {
        Self {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            r11: 0,
            r10: 0,
            r9: 0,
            r8: 0,
            rdi: 0,
            rsi: 0,
            rbp: 0,
            rdx: 0,
            rcx: 0,
            rbx: 0,
            rax: 0,
            rip,
            cs: USER_CODE_SELECTOR as u64,
            rflags: 0x202,
            rsp,
            ss: USER_DATA_SELECTOR as u64,
        }
    }
}

impl Context {
    pub const fn new() -> Self {
        Self {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            rbx: 0,
            rbp: 0,
            rsp: 0,
            rip: 0,
        }
    }
}

unsafe fn read_current() -> Option<ProcRef> {
    unsafe { core::ptr::addr_of!(CURRENT).read() }
}

unsafe fn write_current(value: Option<ProcRef>) {
    unsafe { core::ptr::addr_of_mut!(CURRENT).write(value); }
}

global_asm!(
    r#"
    .global __switch_context
    __switch_context:
        mov [rdi + 0], r15
        mov [rdi + 8], r14
        mov [rdi + 16], r13
        mov [rdi + 24], r12
        mov [rdi + 32], rbx
        mov [rdi + 40], rbp
        mov [rdi + 48], rsp
        lea rax, [rip + 1f]
        mov [rdi + 56], rax

        mov r15, [rsi + 0]
        mov r14, [rsi + 8]
        mov r13, [rsi + 16]
        mov r12, [rsi + 24]
        mov rbx, [rsi + 32]
        mov rbp, [rsi + 40]
        mov rsp, [rsi + 48]
        jmp [rsi + 56]
    1:
        ret

    .global __enter_user
    __enter_user:
        mov rsp, rdi
        pop r15
        pop r14
        pop r13
        pop r12
        pop r11
        pop r10
        pop r9
        pop r8
        pop rdi
        pop rsi
        pop rbp
        pop rdx
        pop rcx
        pop rbx
        pop rax
        iretq
    "#
);
