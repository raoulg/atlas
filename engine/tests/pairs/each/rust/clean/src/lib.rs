//! What a graph draws: each node's touches and what it does instead, each drawn where it is.

pub struct Touch {
    pub op: String,
}

pub struct Where {
    pub place: String,
}

pub struct Node {
    pub id: String,
    pub touches: Vec<Touch>,
    pub instead: Vec<Touch>,
}

pub struct Graph {
    pub nodes: Vec<Node>,
}

impl Touch {
    pub fn drawn(&self) -> Where {
        Where { place: self.op.clone() }
    }
}

/// Each node's places, by its id.
pub fn drawn(g: &Graph) -> Vec<(String, Vec<Where>)> {
    g.nodes.iter().map(|n| (n.id.clone(), n.touches.iter().chain(&n.instead).map(Touch::drawn).collect())).collect()
}
