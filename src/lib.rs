#![no_std]

extern crate alloc;

#[cfg(test)]
extern crate std;

pub mod alloc_layout;
pub mod fs;
pub mod list;
pub mod lock;
pub mod syscall;
pub mod user;

#[path = "task/proc_layout.rs"]
pub mod proc_layout;
