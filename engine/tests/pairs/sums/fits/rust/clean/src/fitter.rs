//! Fitting a curve to measurements.

use crate::model::Outcome;

pub fn fit(xs: &[f64]) -> Outcome {
    if xs.is_empty() {
        return Outcome::Failed { reason: "no measurements".to_string() };
    }
    Outcome::Fit { score: xs.iter().sum::<f64>() / xs.len() as f64 }
}
