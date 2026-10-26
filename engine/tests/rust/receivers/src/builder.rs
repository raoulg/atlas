pub struct GraphBuilder {
    edges: Vec<(String, String)>,
}

impl GraphBuilder {
    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.edges.push((from.to_string(), to.to_string()));
    }
}
