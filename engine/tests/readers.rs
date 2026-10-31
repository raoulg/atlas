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

/// An id is one of the code's or a name past it (`Code ⊔ Beyond`), and the tag of the second
/// injection is reached through one prism, `model::ext`: it is spelled once, where the prism is
/// defined. No reader, and nothing on the graph side, writes the tag, tests for it or takes it off
/// by hand.
#[test]
fn a_name_past_the_code_goes_through_one_prism() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<PathBuf> = vec![];
    for dir in [root.join("src"), root.join("src/python"), root.join("src/typescript"), root.join("src/rust"), root.join("../cli/src")] {
        files.extend(std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().map_or(false, |e| e == "rs")));
    }
    files.sort();
    let mut found = vec![];
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        // the code, without its tests, which write ids as the page's JSON has them
        for (i, line) in text.lines().take_while(|l| l.trim() != "#[cfg(test)]").enumerate() {
            let line = line.split("// ").next().unwrap_or("");
            if !line.trim_start().starts_with("//") && line.contains("\"ext:") {
                found.push(format!("{}:{}: {}", f.file_name().unwrap().to_string_lossy(), i + 1, line.trim()));
            }
        }
    }
    assert!(
        found.len() == 1 && found[0].starts_with("model.rs:") && found[0].contains("const TAG"),
        "the tag of a name past the code spelled outside `model::ext`:\n{}",
        found.join("\n")
    );
}

/// Library knowledge is data: a library's or a framework's name stands in knowledge.rs and
/// nowhere else, and no project's name or idiom stands anywhere in the engine or the viewer.
#[test]
fn library_knowledge_is_data() {
    use atlas_engine::knowledge as k;
    let mut names: Vec<&str> = vec![];
    names.extend(k::PACKAGES.iter().map(|(_, p, _)| *p));
    names.extend(k::OPERATIONS.iter().map(|(p, _, _)| *p));
    names.extend(k::DRIVER_OPS.iter().map(|(p, _)| *p));
    names.extend(k::TOOL_SERVERS.iter().flat_map(|(p, c)| [*p, *c]));
    names.extend(k::PROMISIFIERS.iter().flat_map(|(p, f)| [*p, *f]));
    names.extend(k::PREFIXED_ROUTERS.iter().flat_map(|r| [r.package, r.class]));
    names.extend(k::MOUNTS.iter().map(|m| m.package));
    names.extend(k::CONTROLLERS.iter().flat_map(|(p, c, _)| [*p, *c]));
    names.extend(k::ARG_PARSERS.iter().flat_map(|a| [a.package, a.namespace]));
    names.extend(k::OPTION_PARSERS.iter().map(|o| o.function));
    names.extend(k::DERIVED_PARSERS.iter().flat_map(|d| std::iter::once(d.package).chain(d.derives.iter().copied())));
    names.extend(k::RUST_INTERIOR.iter().flat_map(|i| i.types.iter().copied()));
    names.extend(k::CLI_FRAMEWORKS.iter().map(|c| c.package));
    names.extend(k::WEB_FRAMEWORKS.iter().map(|w| w.package));
    names.extend(
        k::ENTRY_TABLES.iter().flat_map(|(path, _)| path.iter().copied()).filter(|p| !matches!(*p, "project" | "tool" | "scripts")),
    );
    // the build backends, by the tool table each keeps its settings in
    names.extend(k::PACKAGE_SETTINGS.iter().map(|(path, _)| path[1]).chain(k::PACKAGE_ROOTS.iter().map(|path| path[1])));
    names.extend(k::MUTATORS.iter().map(|(_, s, _)| *s).filter(|s| !matches!(*s, "builtins" | "javascript")));
    names.extend(k::HANDS_ON.iter().map(|(c, _)| *c).filter(|c| *c != "std"));
    names.extend(k::RUST_FILE_OPENERS.iter().map(|(t, _, _)| *t).filter(|t| !matches!(*t, "Reader" | "Writer")));
    // `Iterator` and `Option` are Python's words as well: typing's `Iterator`, typer's `Option`
    names.extend(k::RUST_CONTAINERS.iter().copied().filter(|c| !matches!(*c, "Iterator" | "Option")));
    // a path's classes, functions, methods and attributes hold where ty finds their library, never by
    // their names; the language's own words (`str`, `join`, `builtins`) stand in the reader for other
    // reasons, and `parent` is also the name a path's directory goes by where the path is not known
    let paths = k::PYTHON_PATH_CLASSES
        .iter()
        .copied()
        .chain(k::PYTHON_PATH_FUNCTIONS.iter().map(|(l, f, _)| (*l, *f)))
        .chain(k::PYTHON_PATH_METHODS.iter().map(|(l, m, _)| (*l, *m)))
        .chain(k::PYTHON_PATH_ATTRIBUTES.iter().map(|(l, a, _)| (*l, *a)));
    names.extend(paths.flat_map(|(l, f)| [l, f]).filter(|n| !matches!(*n, "builtins" | "str" | "join" | "parent")));
    names.extend(k::PYTHON_ENVIRONMENT.iter().flat_map(|(l, c, _)| [*l, *c]));
    // a file's classes hold where ty finds their library, never by their names
    names.extend(k::PYTHON_FILE_CLASSES.iter().flat_map(|(l, c, _)| [*l, *c]));
    // a runner's test files and a package's client makers by their conventions (`open` is also a
    // word of the page and of the effects); the conditions of `exports` other than words the reader
    // uses for the language's own forms (`import`, `default`)
    names.extend(k::TYPESCRIPT_TEST_FILES.marks.iter().chain(k::TYPESCRIPT_TEST_FILES.dirs).copied());
    names.extend(k::TYPESCRIPT_CLIENT_MAKERS.names.iter().copied().filter(|n| *n != "open"));
    names.extend(k::EXPORT_CONDITIONS.iter().copied().filter(|c| !matches!(*c, "import" | "require" | "default" | "module" | "node")));
    for table in [
        k::ML_PACKAGES,
        k::HUB_PACKAGES,
        k::FILE_PACKAGES,
        k::ROUTERS,
        k::NODE_FS,
        k::FILE_ROUTES,
        k::ROOT_ROUTES,
        k::DRIVER_ACCESS,
        k::HUB_READ,
        k::HUB_WRITE,
    ] {
        names.extend(table.iter().copied());
    }
    // what the engine once tested by name
    names.extend(["cmd_", "repo_id", "plt", "np", "pd", "OpenOptions", "pathlib"]);
    let quoted: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
    // in the viewer as well, where a library's name is also a word of the page (an event, a kind)
    let anywhere = Regex::new(r"\b(rkg|plusmin|vectormesh|goad|huggingface|cmd_)\b").unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("src")).unwrap().flatten().map(|e| e.path()).collect();
    for sub in ["python", "typescript", "rust"] {
        files.extend(std::fs::read_dir(root.join("src").join(sub)).unwrap().flatten().map(|e| e.path()));
    }
    files.retain(|p| p.extension().map_or(false, |e| e == "rs") && !p.ends_with("knowledge.rs"));
    files.push(root.join("../viewer/index.html"));
    let mut found = vec![];
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        let code = text.split("#[cfg(test)]").next().unwrap_or("");
        for (i, line) in code.lines().enumerate() {
            let line = line.split("// ").next().unwrap_or("");
            if line.trim_start().starts_with("//") {
                continue;
            }
            let rust = f.extension().map_or(false, |e| e == "rs");
            let named = quoted.iter().find(|q| rust && line.contains(q.as_str())).map(|q| q.to_string());
            let hit = named.or_else(|| anywhere.find(line).map(|m| m.as_str().to_string()));
            if let Some(h) = hit {
                found.push(format!("{}:{}: {h}: {}", f.file_name().unwrap().to_string_lossy(), i + 1, line.trim()));
            }
        }
    }
    assert!(found.is_empty(), "library knowledge outside knowledge.rs, or a project's name:\n{}", found.join("\n"));
}
