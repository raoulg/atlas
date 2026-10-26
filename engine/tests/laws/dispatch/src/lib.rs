//! One trait, three ways to keep it, and a call that does not say which runs.

pub trait Language {
    fn parse(&self, text: &str) -> usize;
}

pub struct Words;
pub struct Lines;
pub struct Bytes;

impl Language for Words {
    fn parse(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }
}

impl Language for Lines {
    fn parse(&self, text: &str) -> usize {
        text.lines().count()
    }
}

impl Language for Bytes {
    fn parse(&self, text: &str) -> usize {
        text.len()
    }
}

pub fn count(language: &dyn Language, text: &str) -> usize {
    language.parse(text)
}

pub fn words(text: &str) -> usize {
    let words = Words;
    words.parse(text)
}
