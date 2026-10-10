fn main() {
    let page = handed::world::fetch("https://example.org");
    println!("{}", page.body);
}
