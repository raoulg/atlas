//! The model: it pools a batch into one row and shows it.

use ndarray::{Array1, Array2, Axis};

pub fn pool(x: Array2<f32>) -> Array1<f32> {
    x.sum_axis(Axis(0))
}

pub fn show(y: Array1<f32>) {
    println!("{y}");
}
