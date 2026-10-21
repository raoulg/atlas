/// Exported, called below, does nothing.
pub fn tidy(_n: u32) {}

/// Private, called below, does nothing.
fn sweep(_n: u32) {}

pub fn close(n: u32) -> u32 {
    tidy(n);
    sweep(n);
    n + 1
}
