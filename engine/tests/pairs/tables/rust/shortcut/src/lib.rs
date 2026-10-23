//! How long a run may take in its mode: the modes in one list, their plans in another at the same places.

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Serial,
    Parallel,
}

pub struct Plan {
    pub workers: usize,
    pub timeout: usize,
}

fn index(modes: &[Mode], mode: Mode) -> usize {
    modes.iter().position(|m| *m == mode).unwrap_or(0)
}

pub fn timeout(modes: &[Mode], plans: &[Plan], mode: Mode) -> usize {
    plans[index(modes, mode)].timeout
}
