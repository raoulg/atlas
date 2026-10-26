use crate::model::{Label, Order as Parcel};

pub fn ship(order: &Parcel) -> Label {
    Label { text: format!("{} ({} kg)", order.reference, order.weight) }
}
