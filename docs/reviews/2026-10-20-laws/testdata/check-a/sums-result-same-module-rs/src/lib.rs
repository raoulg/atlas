pub struct Fit {
    pub score: f64,
}

pub struct Failed {
    pub reason: String,
}

pub fn fit(xs: &[f64]) -> Result<Fit, Failed> {
    if xs.is_empty() {
        Err(Failed { reason: "empty".to_string() })
    } else {
        Ok(Fit { score: xs.iter().sum() })
    }
}

pub fn report(r: Result<Fit, Failed>) -> String {
    match r {
        Ok(f) => format!("fit {}", f.score),
        Err(e) => format!("failed: {}", e.reason),
    }
}
