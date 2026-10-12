//! The engine knows languages, but only where it reads them. What works on the graph (the flow,
//! the families, the principles, the route as text, the views, the parts, the builder the readers
//! fill) reads the facts a reader recorded: a parameter that is the receiver, a constructor, a
//! private name, what a language leaves to its author. It never reads a language's names for them.

use std::path::PathBuf;

use regex::Regex;

/// The modules that work on the graph, whatever language it was read from.
const GRAPH_SIDE: &[&str] = &["builder", "changes", "families", "flow", "lib", "model", "parts", "principles", "story", "view"];

#[test]
fn only_the_readers_know_a_language() {
    let names = Regex::new(
        r#"__init__|"(?:self|cls|this|Self|None|void|undefined|constructor)"|(?:starts_with|strip_prefix|trim_start_matches)\(['"]_['"]\)|"(?:python|rust|typescript)"|\b(?:Python|Rust|TypeScript)\b"#,
    )
    .unwrap();
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = vec![];
    for m in GRAPH_SIDE {
        let text = std::fs::read_to_string(src.join(format!("{m}.rs"))).unwrap();
        for (i, line) in text.lines().enumerate() {
            let code = line.split("// ").next().unwrap_or("");
            if !code.trim_start().starts_with("//") && names.is_match(code) {
                found.push(format!("{m}.rs:{}: {}", i + 1, line.trim()));
            }
        }
    }
    assert!(found.is_empty(), "a language's own names outside its reader:\n{}", found.join("\n"));
}
