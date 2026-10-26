//! A graph that counts its nodes again, and says how many it holds.

pub struct Graph {
    nodes: Vec<String>,
    count: usize,
}

impl Graph {
    pub fn recount(&mut self) {
        self.count = self.nodes.len();
    }

    pub fn len(&self) -> usize {
        self.count
    }
}
