//! A heading read from a file, shown on a plain block built where it is shown.

use std::path::Path;

pub trait Block {
    /// The heading, as most blocks show it.
    fn show(&self, heading: &str) {
        println!("{heading}");
    }
}

pub struct Plain {
    pub width: usize,
}

pub struct Boxed {
    pub width: usize,
}

impl Block for Plain {}

impl Block for Boxed {
    fn show(&self, heading: &str) {
        println!("[ {:^width$} ]", heading.trim(), width = self.width);
    }
}

/// The heading in `source`, shown on a plain block.
pub fn report(source: &Path) {
    let heading = std::fs::read_to_string(source).unwrap_or_default();
    let plain = Plain { width: 40 };
    plain.show(&heading);
}
