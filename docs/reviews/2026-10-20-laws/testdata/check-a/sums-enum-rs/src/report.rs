use crate::model::Outcome;

pub fn report(r: &Outcome) -> String {
    match r {
        Outcome::Fit(f) => format!("fit {}", f.score),
        Outcome::Failed(e) => format!("failed: {}", e.reason),
    }
}
