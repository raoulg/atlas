use crate::Edge;

/// One thing a body does.
pub enum Item {
    Step(String),
    Choice(Vec<Item>),
}

/// The edges out of each node.
pub struct Ix<'g> {
    out: Vec<&'g Edge>,
}

impl<'g> Ix<'g> {
    pub fn out(&self) -> impl Iterator<Item = &'g Edge> + '_ {
        self.out.iter().copied()
    }
}
