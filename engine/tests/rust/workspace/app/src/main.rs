fn show(text: &str) {
    let g = map_core::analyze(text);
    println!("{}", g.lines);
}

fn main() {
    show("a\nb");
}
