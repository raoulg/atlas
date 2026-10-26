use crate::model::Outcome;

pub fn fit(xs: &[f64]) -> Outcome {
    if xs.is_empty() {
        Outcome::Failed { reason: "empty".to_string() }
    } else {
        Outcome::Fit { score: xs.iter().sum() }
    }
}
