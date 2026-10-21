//! What a fit says: a line, and the points a plot of it draws.

use crate::model::Outcome;

pub struct Report {
    pub line: String,
    pub points: Vec<f64>,
}

pub fn describe(o: &Outcome) -> Report {
    match o {
        Outcome::Fit { score } => Report { line: format!("fit {score:.2}"), points: vec![*score] },
        Outcome::Failed { reason } => Report { line: format!("failed: {reason}"), points: vec![] },
    }
}
