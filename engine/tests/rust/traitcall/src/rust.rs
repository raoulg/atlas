pub fn parse(text: &str) -> usize {
    text.matches("fn ").count()
}
