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
}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self{
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        // TODO: 关闭中断
        while self.try_lock().is_err() {
            spin_loop();
        }

        SpinLockGuard { lock: self }
    }

    pub fn try_lock(&self) -> Result<SpinLockGuard<'_, T>, ()> {
        // TODO: 关闭中断
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map(|_| SpinLockGuard { lock: self })
            .map_err(|_| ())
    }

    pub fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
        // TODO：恢复中断状态
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
    }
}
