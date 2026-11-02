//! The shelf on disk: the store makes a Shelf and hands it to the report, and its tests stand beside it.

pub struct Shelf {
    pub titles: Vec<String>,
}

pub fn load(path: &str) -> Shelf {
    let text = std::fs::read_to_string(path).unwrap();
    Shelf { titles: text.lines().map(String::from).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sample {
        path: String,
    }

    fn sample() -> Sample {
        Sample { path: String::from("fixtures/shelf.csv") }
    }

    fn check(sample: Sample) -> Shelf {
        load(&sample.path)
    }

    #[test]
    fn loads() {
        assert!(!check(sample()).titles.is_empty());
    }
}
