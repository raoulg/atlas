//! A shop that ships orders: the order it is handed, the label it gives back.

pub mod model;
pub mod post;

pub use model::{Label, Order};
pub use post::ship;
