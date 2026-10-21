use crate::model::{Failed, Fit};

pub fn fit(xs: &[f64]) -> Result<Fit, Failed> {
    if xs.is_empty() {
        Err(Failed { reason: "empty".to_string() })
    } else {
        Ok(Fit { score: xs.iter().sum() })
    }
}

pub fn best(xs: &[f64]) -> Option<Fit> {
    xs.iter().cloned().reduce(f64::max).map(|score| Fit { score })
}
