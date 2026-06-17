pub const USER_CODE_BASE: usize = 0x0000_0000_0040_0000;
pub const USER_SLOT_SIZE: usize = 0x20_0000;
pub const USER_STACK_TOP: usize = 0x0000_7fff_ffff_f000;
pub const DEFAULT_TIME_SLICE_TICKS: u32 = 5;

pub fn user_code_base(pid: u64) -> usize {
    USER_CODE_BASE + pid as usize * USER_SLOT_SIZE
}

pub fn user_stack_top(pid: u64) -> usize {
    USER_STACK_TOP - pid as usize * USER_SLOT_SIZE
}

pub fn consume_tick(ticks_left: &mut u32, default_slice: u32) -> bool {
    if *ticks_left > 0 {
        *ticks_left -= 1;
    }

    if *ticks_left == 0 {
        *ticks_left = default_slice;
        true
    } else {
        false
    }
}

pub fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_slots_do_not_overlap() {
        let first_code = user_code_base(1);
        let second_code = user_code_base(2);
        let first_stack_top = user_stack_top(1);
        let second_stack_top = user_stack_top(2);

        assert_eq!(second_code - first_code, USER_SLOT_SIZE);
        assert_eq!(first_stack_top - second_stack_top, USER_SLOT_SIZE);
    }

    #[test]
    fn time_slice_expires_and_reloads() {
        let mut ticks = 2;

        assert!(!consume_tick(&mut ticks, DEFAULT_TIME_SLICE_TICKS));
        assert_eq!(ticks, 1);

        assert!(consume_tick(&mut ticks, DEFAULT_TIME_SLICE_TICKS));
        assert_eq!(ticks, DEFAULT_TIME_SLICE_TICKS);
    }

    #[test]
    fn align_down_rounds_to_lower_boundary() {
        assert_eq!(align_down(0x12345, 0x1000), 0x12000);
        assert_eq!(align_down(0x12000, 0x1000), 0x12000);
    }
}
