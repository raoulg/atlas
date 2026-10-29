//! Blocks that show a heading under a name: every block is named, a unit struct is a block by its
//! name alone, and a block handed as a trait object is asked its name.

pub trait Named {
    fn name(&self) -> String {
        String::from("block")
    }
}

pub trait Block: Named {
    fn show(&self, heading: &str);
}

pub struct Plain;

pub struct Fancy {
    pub stars: usize,
}

impl Named for Plain {}

impl Block for Plain {
    fn show(&self, heading: &str) {
        println!("{}: {heading}", self.name());
    }
}

impl Named for Fancy {
    fn name(&self) -> String {
        "*".repeat(self.stars)
    }
}

impl Block for Fancy {
    fn show(&self, heading: &str) {
        println!("{} {heading} {}", self.name(), self.name());
    }
}

pub fn plain(heading: &str) {
    Plain.show(heading);
}

pub fn bound(heading: &str) {
    let plain = Plain;
    plain.show(heading);
}

pub fn named(block: &dyn Block) -> String {
    block.name()
}
