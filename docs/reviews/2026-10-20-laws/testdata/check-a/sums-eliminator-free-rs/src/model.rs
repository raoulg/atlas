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

pub fn describe(o: &Outcome) -> String {
    match o {
        Outcome::Fit(f) => format!("fit {}", f.score),
        Outcome::Failed(e) => format!("failed: {}", e.reason),
    }
}
