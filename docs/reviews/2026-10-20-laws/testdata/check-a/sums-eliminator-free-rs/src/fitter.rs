use crate::model::{Failed, Fit, Outcome};

pub fn fit(xs: &[f64]) -> Outcome {
    if xs.is_empty() {
        Outcome::Failed(Failed { reason: "empty".to_string() })
    } else {
        Outcome::Fit(Fit { score: xs.iter().sum() })
    }
}
