//! A route as text names each step by one label: alternatives share a number and are lettered by
//! branch, past `z` as a spreadsheet names its columns, and no label stands for two steps.

use atlas_engine::{analyze, Graph, Options};

/// A crate written to a directory of its own, read, and removed.
fn written(name: &str, main: &str) -> Graph {
    let dir = std::env::temp_dir().join(format!("atlas-story-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let manifest = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    for (path, text) in [("Cargo.toml", manifest.as_str()), ("src/main.rs", main)] {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let g = analyze(&dir, &Options::default());
    let _ = std::fs::remove_dir_all(&dir);
    g.expect("the crate parses")
}

/// A `match` of `arms` arms, each calling a function of its own; the first arm also calls a method
/// of a trait two types implement, a call with alternatives of its own.
fn arms(arms: usize) -> String {
    let mut s = String::from(
        "trait Shape {\n    fn draw(&self);\n}\n\nstruct Circle;\nstruct Square;\n\nimpl Shape for Circle {\n    fn draw(&self) {\n        println!(\"o\");\n    }\n}\n\nimpl Shape for Square {\n    fn draw(&self) {\n        println!(\"[]\");\n    }\n}\n\nfn main() {\n    let shape: &dyn Shape = &Circle;\n    match std::env::args().count() {\n",
    );
    for i in 0..arms {
        let also = if i == 0 { " shape.draw();" } else { "" };
        s += &format!("        {i} => {{ arm{i}();{also} }}\n");
    }
    s += "        _ => {}\n    }\n}\n";
    for i in 0..arms {
        s += &format!("\nfn arm{i}() {{\n    println!(\"{i}\");\n}}\n");
    }
    s
}

/// A step line of a route: its indentation, its label, the step it names and what decides it.
struct Line<'a> {
    indent: usize,
    label: &'a str,
    step: &'a str,
    when: &'a str,
    see: Option<&'a str>,
}

fn lines(route: &str) -> Vec<Line<'_>> {
    route
        .lines()
        .skip(2)
        .filter_map(|l| {
            let text = l.trim_start();
            let (label, rest) = text.split_once("  ")?;
            let step = rest.split("   ·   ").next().unwrap_or(rest).split("  → see ").next().unwrap_or(rest);
            let when = rest.split("   ·   ").find_map(|f| f.strip_prefix("when ")).unwrap_or("");
            let see = rest.split("  → see ").nth(1).map(|s| s.split("   ·   ").next().unwrap_or(s));
            Some(Line { indent: l.len() - text.len(), label, step, when, see })
        })
        .collect()
}

/// A label names one branch: the steps that share it are the steps of one branch (one indentation,
/// one condition), steps under different conditions have labels of their own, and every `→ see`
/// names a label the route shows. A choice past 26 arms keeps this, and so does a call with
/// alternatives inside a lettered branch.
#[test]
fn a_label_names_one_branch() {
    let g = written("arms", &arms(30));
    let route = atlas_engine::story::route(&g, "arms.main", 3).unwrap();
    let lines = lines(&route);
    let mut branch: std::collections::HashMap<&str, (usize, &str)> = Default::default();
    let mut label: std::collections::HashMap<(usize, &str), &str> = Default::default();
    for l in &lines {
        let was = *branch.entry(l.label).or_insert((l.indent, l.when));
        assert_eq!(was, (l.indent, l.when), "{} names two branches, at {}:\n{route}", l.label, l.step);
        if !l.when.is_empty() {
            let named = *label.entry((l.indent, l.when)).or_insert(l.label);
            assert_eq!(named, l.label, "one branch, `when {}`, under two labels:\n{route}", l.when);
        }
        if let Some(at) = l.see {
            assert!(at == "the entry" || branch.contains_key(at), "→ see {at} names no step shown before it:\n{route}");
        }
    }
    let whens: std::collections::HashSet<&str> = lines.iter().map(|l| l.when).filter(|w| !w.is_empty()).collect();
    assert_eq!(whens.len(), 30, "every arm is a branch of the route:\n{route}");
    assert!(lines.iter().any(|l| l.indent > 0), "the call with alternatives is opened:\n{route}");
}

/// The branches of one choice in `choices` each have letters of their own.
#[test]
fn a_choice_letters_each_branch_apart() {
    let g = written("listed", &arms(30));
    let choices = atlas_engine::story::choices(&g, "listed.main").unwrap();
    let letters: Vec<&str> =
        choices.lines().filter(|l| l.starts_with("  ") && l.contains("when ")).filter_map(|l| l.split_whitespace().next()).collect();
    let apart: std::collections::HashSet<&str> = letters.iter().copied().collect();
    assert_eq!(letters.len(), 30, "a line per arm:\n{choices}");
    assert_eq!(apart.len(), letters.len(), "two branches share letters:\n{choices}");
}
