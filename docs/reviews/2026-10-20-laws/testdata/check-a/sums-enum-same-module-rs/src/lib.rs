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

pub fn fit(xs: &[f64]) -> Outcome {
    if xs.is_empty() {
        Outcome::Failed(Failed { reason: "empty".to_string() })
    } else {
        Outcome::Fit(Fit { score: xs.iter().sum() })
    }
}

pub fn report(r: Outcome) -> String {
    match r {
        Outcome::Fit(f) => format!("fit {}", f.score),
        Outcome::Failed(e) => format!("failed: {}", e.reason),
    }
}
