pub fn scale(x: f64, k: f64) -> f64 {
    x * k
}

pub fn metres(text: &str) -> f64 {
    text.trim_end_matches('m').parse().unwrap_or(0.0)
}
