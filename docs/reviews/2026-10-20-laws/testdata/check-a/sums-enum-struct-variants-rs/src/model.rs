pub enum Outcome {
    Fit { score: f64 },
    Failed { reason: String },
}
