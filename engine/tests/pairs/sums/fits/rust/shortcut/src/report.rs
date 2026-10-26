//! What a fit says, as a line.

use crate::model::Outcome;

pub fn line(o: &Outcome) -> String {
    match o {
        Outcome::Fit { score } => format!("fit {score:.2}"),
        Outcome::Failed { reason } => format!("failed: {reason}"),
    }
}
