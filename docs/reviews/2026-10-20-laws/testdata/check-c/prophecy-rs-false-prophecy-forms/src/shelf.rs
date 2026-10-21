pub fn save(n: u32) {
    println!("{n}");
}

pub fn keep() {
    std::fs::write("x", "y").unwrap();
}
