use crate::rust;

pub trait Language {
    fn parse(&self, text: &str) -> usize;
}

pub struct Rust;

impl Language for Rust {
    fn parse(&self, text: &str) -> usize {
        rust::parse(text)
    }
}
