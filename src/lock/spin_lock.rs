/// 自旋锁

use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for SpinLock<T> {}
unsafe impl<T: Send> Sync for SpinLock<T> {}

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
    interrupts_were_enabled: bool,
}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self{
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        let interrupts_were_enabled = interrupts_enabled();
        unsafe { disable_interrupts(); }

        loop {
            if self
                .locked
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return SpinLockGuard {
                    lock: self,
                    interrupts_were_enabled,
                };
            }
            spin_loop();
        }
    }

    pub fn try_lock(&self) -> Result<SpinLockGuard<'_, T>, ()> {
        let interrupts_were_enabled = interrupts_enabled();
        unsafe { disable_interrupts(); }

        match self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        {
            Ok(_) => Ok(SpinLockGuard {
                lock: self,
                interrupts_were_enabled,
            }),
            Err(_) => {
                if interrupts_were_enabled {
                    unsafe { enable_interrupts(); }
                }
                Err(())
            }
        }
    }

    pub fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.unlock();
        if self.interrupts_were_enabled {
            unsafe { enable_interrupts(); }
        }
    }
}

#[cfg(target_os = "none")]
fn interrupts_enabled() -> bool {
    let flags: usize;
    unsafe {
        core::arch::asm!(
            "pushfq",
            "pop {}",
            out(reg) flags,
            options(nomem, preserves_flags),
        );
    }
    flags & (1 << 9) != 0
}

#[cfg(not(target_os = "none"))]
fn interrupts_enabled() -> bool {
    false
}

#[cfg(target_os = "none")]
unsafe fn disable_interrupts() {
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack, preserves_flags));
    }
}

#[cfg(not(target_os = "none"))]
unsafe fn disable_interrupts() {}

#[cfg(target_os = "none")]
unsafe fn enable_interrupts() {
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
    }
}

#[cfg(not(target_os = "none"))]
unsafe fn enable_interrupts() {}

#[cfg(test)]
mod tests {
    use super::SpinLock;

    #[test]
    fn lock_allows_mutating_inner_value() {
        let lock = SpinLock::new(1usize);
        {
            let mut guard = lock.lock();
            *guard += 41;
        }

        assert_eq!(*lock.lock(), 42);
    }

    #[test]
    fn try_lock_fails_while_locked() {
        let lock = SpinLock::new(());
        let guard = lock.lock();

        assert!(lock.try_lock().is_err());

        drop(guard);
        assert!(lock.try_lock().is_ok());
    }
}
