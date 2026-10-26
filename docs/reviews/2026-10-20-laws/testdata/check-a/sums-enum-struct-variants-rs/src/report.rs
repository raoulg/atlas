use crate::model::Outcome;

pub fn report(r: &Outcome) -> String {
    match r {
        Outcome::Fit { score } => format!("fit {score}"),
        Outcome::Failed { reason } => format!("failed: {reason}"),
    }
}
