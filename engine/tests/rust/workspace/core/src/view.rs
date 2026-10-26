use crate::model::Graph;

pub trait View {
    fn shown(&self, g: &Graph) -> usize;
}
