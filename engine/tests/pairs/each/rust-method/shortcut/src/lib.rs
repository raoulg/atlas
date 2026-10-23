//! What a graph draws: every node's touches and what it does instead, pushed one by one onto a list the body changes.

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

/// Every place the graph's nodes draw.
pub fn drawn(g: &Graph) -> Vec<Where> {
    let mut out = vec![];
    for n in &g.nodes {
        for t in n.touches.iter().chain(&n.instead) {
            out.push(t.drawn());
        }
    }
    out
}
