//! A choice whose arms call different functions, and an or-else whose sides are alternatives.

pub struct Config {
    pub name: String,
}

fn load(path: &str) -> Config {
    Config { name: path.to_string() }
}

fn find() -> Option<Config> {
    None
}

fn infer() -> Config {
    Config { name: String::new() }
}

/// What is declared, else what is found, else an inference.
pub fn resolve(declared: Option<&str>) -> Config {
    match declared {
        Some(p) => load(p),
        None => find().unwrap_or_else(|| infer()),
    }
}
