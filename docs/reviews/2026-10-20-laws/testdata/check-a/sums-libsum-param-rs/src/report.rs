use crate::model::{Failed, Fit};

pub fn report(r: Result<Fit, Failed>) -> String {
    match r {
        Ok(f) => format!("fit {}", f.score),
        Err(e) => format!("failed: {}", e.reason),
    }
}

pub fn headline(b: Option<Fit>) -> String {
    match b {
        Some(f) => format!("best {}", f.score),
        None => "none".to_string(),
    }
}
