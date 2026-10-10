//! A type the crate re-exports from a private module.

pub struct Shape {
    pub sides: u32,
}

impl Shape {
    pub fn square() -> Self {
        Shape { sides: 4 }
    }
}
