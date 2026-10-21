pub struct Fit {
    pub score: f64,
}

pub struct Failed {
    pub reason: String,
}

pub enum Outcome {
    Fit(Fit),
    Failed(Failed),
}
