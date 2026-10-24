//! What a graph draws: each node's places, gathered node by node into a list of a type the code names.

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

/// A node's places, by its id.
pub struct Drawn {
    pub id: String,
    pub places: Vec<Where>,
}

impl Touch {
    pub fn drawn(&self) -> Where {
        Where { place: self.op.clone() }
    }
}

/// Each node's places, by its id.
pub fn drawn(g: &Graph) -> Vec<Drawn> {
    let mut out = vec![];
    for n in &g.nodes {
        let places: Vec<Where> = n.touches.iter().chain(&n.instead).map(Touch::drawn).collect();
        out.push(Drawn { id: n.id.clone(), places });
    }
    out
}
