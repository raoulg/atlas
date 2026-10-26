use crate::builder::GraphBuilder;

pub fn collect_symbols(names: &[String], store: &mut GraphBuilder) {
    for name in names {
        store.add_edge(name, name);
    }
}
