//! Training: the run that joins the data and the model. Its arrays are ndarray's, of two ranks.

mod data;
mod model;

fn main() {
    let x = data::batch();
    let y = model::pool(x);
    model::show(y);
}
