//! What fitting a curve gives: a fit, or why there is none.

pub enum Outcome {
    Fit { score: f64 },
    Failed { reason: String },
}
