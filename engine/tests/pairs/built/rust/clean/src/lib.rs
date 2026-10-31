//! A heading read from a file, shown on a block of whichever kind the caller hands, under the block's name.

use std::path::Path;

/// What a block is called.
pub trait Named {
    fn name(&self) -> &str;
}

pub trait Block: Named {
    /// The heading, as most blocks show it.
    fn show(&self, heading: &str) {
        println!("{heading}");
    }
}

/// A block that shows the heading as it is.
pub struct Plain;

pub struct Boxed {
    pub width: usize,
}

impl Named for Plain {
    fn name(&self) -> &str {
        "plain"
    }
}

impl Named for Boxed {
    fn name(&self) -> &str {
        "boxed"
    }
}

impl Block for Plain {}

impl Block for Boxed {
    fn show(&self, heading: &str) {
        println!("[ {:^width$} ]", heading.trim(), width = self.width);
    }
}

/// The heading in `source`, shown on the block handed, under its name.
pub fn report(block: &dyn Block, source: &Path) {
    let heading = std::fs::read_to_string(source).unwrap_or_default();
    println!("{}:", block.name());
    block.show(&heading);
}
