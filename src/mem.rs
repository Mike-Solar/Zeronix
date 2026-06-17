pub mod page;

#[derive(Clone, Copy, Debug)]
pub struct PhysAddr(usize) ;
#[derive(Clone, Copy, Debug)]
pub struct VirtAddr(usize);

pub trait MemoryAddress{
    fn as_usize(&self) -> usize;
    fn as_u64(&self) -> u64;
}


impl MemoryAddress for PhysAddr {
    fn as_u64(&self) -> u64 {
        self.0 as u64
    }

    fn as_usize(&self) -> usize {
        self.0
    }
}
impl From<u64> for PhysAddr {
    fn from(value: u64) -> PhysAddr {
        PhysAddr(value as usize)
    }
}
impl From<usize> for PhysAddr {
    fn from(value: usize) -> PhysAddr {
        PhysAddr(value)
    }
}

impl From<PhysAddr> for u64 {
    fn from(value: PhysAddr) -> u64 {
        value.0 as u64
    }
}
impl From<PhysAddr> for usize {
    fn from(value: PhysAddr) -> usize {
        value.0
    }
}

impl MemoryAddress for VirtAddr {
    fn as_u64(&self) -> u64 {
        self.0 as u64
    }

    fn as_usize(&self) -> usize {
        self.0
    }
}
impl From<u64> for VirtAddr {
    fn from(value: u64) -> VirtAddr {
        VirtAddr(value as usize)
    }
}
impl From<usize> for VirtAddr {
    fn from(value: usize) -> VirtAddr {
        VirtAddr(value)
    }
}

impl From<VirtAddr> for u64 {
    fn from(value: VirtAddr) -> u64 {
        value.0 as u64
    }
}
impl From<VirtAddr> for usize {
    fn from(value: VirtAddr) -> usize {
        value.0
    }
}
