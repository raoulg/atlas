//! A module linked: each base it names is resolved, then its body is read and checked. One short
//! name, `b`, is bound twice: to each base in the loop, and to the body after it.

pub struct Base {
    pub name: String,
}

pub struct Body {
    pub text: String,
}

pub struct Module {
    pub bases: Vec<Base>,
    pub path: String,
}

fn resolve(base: &Base) -> String {
    format!("crate::{}", base.name)
}

fn read(path: &str) -> Body {
    Body { text: std::fs::read_to_string(path).unwrap() }
}

fn check(body: &Body) -> usize {
    body.text.lines().count()
}

fn link(m: &Module) -> usize {
    let mut names = vec![];
    for b in &m.bases {
        names.push(resolve(b));
    }
    let b = read(&m.path);
    check(&b) + names.len()
}

fn main() {
    let m = Module { bases: vec![Base { name: "a".to_string() }], path: "body.rs".to_string() };
    println!("{}", link(&m));
}
