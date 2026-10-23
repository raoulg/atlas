//! How long a run may take in its mode: one table of plans, a Plan by its Mode.

use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Serial,
    Parallel,
}

pub struct Plan {
    pub workers: usize,
    pub timeout: usize,
}

pub fn timeout(plans: &HashMap<Mode, Plan>, mode: Mode) -> usize {
    plans[&mode].timeout
}
