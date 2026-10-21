//! The points a plot of a fit draws.

use crate::model::Outcome;

pub fn points(o: &Outcome) -> Vec<f64> {
    match o {
        Outcome::Fit { score } => vec![*score],
        Outcome::Failed { .. } => vec![],
    }
}
