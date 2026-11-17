//! The data: a batch of rows, an array ndarray makes.

use ndarray::Array2;

pub fn batch() -> Array2<f32> {
    Array2::zeros((4, 3))
}
