use crate::model;

pub fn ship(order: &model::Order) -> crate::model::Label {
    model::Label { text: format!("{} ({} kg)", order.reference, order.weight) }
}
