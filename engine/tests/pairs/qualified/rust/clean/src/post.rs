use crate::model::{Label, Order};

pub fn ship(order: &Order) -> Label {
    Label { text: format!("{} ({} kg)", order.reference, order.weight) }
}
