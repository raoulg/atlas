//! What the Rust reader reads off one construct of the language, each on a program written for it
//! under `tests/rust`: a workspace's crates and their modules, a trait's implementations and what
//! they call, a re-export of a whole module, the types a receiver has, what `&mut self` changes,
//! the options clap declares for every subcommand, the places a file written is made at, and the
//! values a generic holds.

use std::path::PathBuf;

use atlas_engine::{analyze, EdgeKind, EntryKind, Graph, Held, Options};

/// The program `tests/rust/<name>`, read.
fn read(name: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/rust").join(name), &Options::default())
        .unwrap_or_else(|e| panic!("tests/rust/{name} parses: {e}"))
}

/// The lines of `kind` from `src` to `dst`.
fn edge(g: &Graph, kind: EdgeKind, src: &str, dst: &str) -> bool {
    g.edges.iter().any(|e| e.kind == kind && e.src == src && e.dst == dst)
}

/// A function's touches as (kind, operation, place).
fn places(g: &Graph, id: &str) -> Vec<(String, String, String)> {
    g.node(id)
        .unwrap_or_else(|| panic!("no node {id}"))
        .touches
        .iter()
        .map(|t| (t.kind_name().to_string(), t.op.clone(), t.artifact.clone()))
        .collect()
}

fn place(kind: &str, op: &str, artifact: &str) -> (String, String, String) {
    (kind.to_string(), op.to_string(), artifact.to_string())
}

/// A Cargo workspace is read member by member: each crate is a module named after its package,
/// its `mod`s are modules below it and its structs and traits classes in them, and a crate with
/// only a binary is the module its `main.rs` is, whose `main` is run from the command line and
/// calls the functions beside it.
#[test]
fn a_workspace_is_its_crates() {
    let g = read("workspace");
    for id in ["map-core", "map-core.model", "map-core.view", "app"] {
        assert!(g.node(id).is_some_and(|n| n.kind == atlas_engine::NodeKind::Module), "module {id}");
    }
    assert!(g.node("map-core.model.Graph").is_some() && g.node("map-core.view.View").is_some());
    assert_eq!(g.node("app.main").and_then(|n| n.entry.as_ref()).map(|e| e.kind), Some(EntryKind::Cli));
    assert!(edge(&g, EdgeKind::Calls, "app.main", "app.show"));
}

/// A method of a trait implementation runs what its body calls: `Rust::parse` calls the module
/// function `rust::parse`, the module that shares the type's name, which the impl's `use crate::rust`
/// names.
#[test]
fn a_trait_impl_method_calls_the_module_function() {
    let g = read("traitcall");
    assert!(edge(&g, EdgeKind::Inherits, "traitcall.language.Rust", "traitcall.language.Language"), "impl Language for Rust");
    assert!(edge(&g, EdgeKind::Calls, "traitcall.language.Rust.parse", "traitcall.rust.parse"));
}

/// `pub use model::*` brings every public name of `model` into the crate's root: the root's
/// `Graph` is the one `model` defines.
#[test]
fn a_glob_re_export_resolves_a_type() {
    let g = read("reexports");
    assert!(edge(&g, EdgeKind::Returns, "reexports.analyze", "reexports.model.Graph"));
}

/// A receiver's type is what the code gives it: a local holds what the call it is kept from
/// gives back, through `?` and a `Box<dyn ..>` (`let lang = language::of(..)?; lang.parse(..)`
/// calls the trait's `parse`), and a parameter holds the type it is declared with
/// (`store.add_edge(..)` with `store: &mut GraphBuilder`).
#[test]
fn a_receiver_has_the_type_the_code_gives_it() {
    let g = read("receivers");
    assert!(edge(&g, EdgeKind::Calls, "receivers.graph_of", "receivers.language.Language.parse"));
    assert!(edge(&g, EdgeKind::Calls, "receivers.collect.collect_symbols", "receivers.builder.GraphBuilder.add_edge"));
}

/// A Rust body lists its calls in the order they run, each with what it is handed and the name
/// it is kept by: a method gives what it returns, not its receiver's type, and the repository a
/// function is handed is a value it hands on.
#[test]
fn a_rust_body_lists_its_calls() {
    let g = read("receivers");
    let graph_of = g.node("receivers.graph_of").unwrap();
    let step = |to: &str| graph_of.sites.iter().find(|s| s.to == to).unwrap_or_else(|| panic!("{to} in {:#?}", graph_of.sites));
    assert_eq!(step("receivers.language.Language.parse").gives, [Held::new("receivers.model.Graph", "g")]);
    assert_eq!(
        step("receivers.language.of").hands,
        [Held::new("receivers.model.Repo", "repo"), Held::new("receivers.model.Options", "opts")]
    );
    let order: Vec<&str> = graph_of.sites.iter().map(|s| s.to.as_str()).collect();
    let at = |to: &str| order.iter().position(|t| *t == to).unwrap_or_else(|| panic!("{to} in {order:?}"));
    assert!(at("receivers.read_makefile") < at("receivers.assign"), "{order:?}");
}

/// A method that takes `&mut self` changes the value it runs on, one that takes `&self` reads it.
#[test]
fn mut_self_mutates() {
    let g = read("mutation");
    assert!(edge(&g, EdgeKind::Mutates, "mutation.Graph.recount", "mutation.Graph"));
    assert!(!edge(&g, EdgeKind::Mutates, "mutation.Graph.len", "mutation.Graph"));
}

/// clap's global options are the options of the entry, whichever subcommand runs: `main` takes
/// `--rounds` and `--no-cache`; the subcommand is no option of its own, and `Cli::parse()` is the
/// step that reads them.
#[test]
fn global_options_are_the_entrys_options() {
    let g = read("global");
    let main = g.node("global.main").unwrap();
    let options = ["--rounds", "--no-cache"].map(String::from).to_vec();
    assert_eq!(main.entry.as_ref().map(|e| (e.kind, e.options.clone())), Some((EntryKind::Cli, options.clone())));
    let parse = main.sites.iter().find(|s| s.command_line.is_some()).expect("main parses its command line");
    assert_eq!((parse.to.as_str(), parse.command_line.clone().unwrap()), ("global.Cli", options));
}

/// A file is written where the command line says, else where the code puts it: in `.made/` at the
/// top of the git repository, a folder found on disk that the code cannot name, or for a clone in
/// the system's temporary folder. Each is a place `main` writes; neither starts at a literal in
/// `main`'s code (`out` is a value it computes), and no caller hands it.
#[test]
fn a_path_the_code_computes_is_each_place_it_can_be() {
    let g = read("keeper");
    let written = places(&g, "keeper.main");
    assert!(written.contains(&place("disk_write", "write", "$TMPDIR/made/graph.json")), "{written:?}");
    assert!(
        written.contains(&place("disk_write", "write", "*/.made/graph.json")),
        "the folder found on disk has no name, the file below it has: {written:?}"
    );
    let tmp = g.node("keeper.main").unwrap().touches.iter().find(|t| t.artifact == "$TMPDIR/made/graph.json").unwrap();
    assert!(!tmp.fixed && tmp.by.is_empty(), "{tmp:?}");
}

/// A directory made where a file is written below it is part of writing that file: the root
/// `create_dir_all` makes under `env::temp_dir()` and the directory an `if let Some(d) =
/// dest.parent()` binds are the folders of the file written, under the temporary folder; no
/// directory is a place of its own.
#[test]
fn a_directory_made_for_a_file_is_part_of_writing_it() {
    let g = read("unpack");
    let unpack = places(&g, "unpack.unpack");
    assert!(unpack.contains(&place("disk_write", "write", "$TMPDIR/")), "{unpack:?}");
    assert!(!unpack.iter().any(|(_, op, _)| op == "create_dir_all"), "{unpack:?}");
}

/// A generic's arguments are values where it holds them: `read_all` gives the Graphs of a
/// `Result<Vec<Graph>>`, `merged` takes the Graphs of a `Vec<Graph>`, and `Ix::out` gives the
/// Edges of an `impl Iterator<Item = &Edge>`, whose `Item` names what it gives, not the module's
/// own `Item`. A function a struct keeps (`fn(&str) -> Part`) is handed no value and holds none.
#[test]
fn a_generic_s_arguments_are_its_values() {
    let g = read("generics");
    assert!(edge(&g, EdgeKind::Returns, "generics.read_all", "generics.Graph"));
    assert!(edge(&g, EdgeKind::Takes, "generics.merged", "generics.Graph"));
    assert!(edge(&g, EdgeKind::Returns, "generics.flow.Ix.out", "generics.Edge"));
    assert!(!edge(&g, EdgeKind::Returns, "generics.flow.Ix.out", "generics.flow.Item"));
    assert!(!edge(&g, EdgeKind::Has, "generics.ChoiceSyntax", "generics.Part"));
}

/// A call stands in the value namespace: in a crate named `clash`, `clash()` calls the function
/// `clash` the binary defines, as `other()` calls `other`; the crate's own name is a module, which
/// no call runs.
#[test]
#[ignore = "a call of a function named like its crate resolves to the crate's module"]
fn a_function_named_like_its_crate_is_called() {
    let g = read("clash");
    let main = g.node("clash.main").unwrap();
    let steps: Vec<&str> = main.sites.iter().map(|s| s.to.as_str()).collect();
    assert_eq!(steps, ["clash.clash", "clash.other"]);
    let written = places(&g, "clash.main");
    assert!(written.contains(&place("disk_write", "write", "$TMPDIR/clash.txt")), "{written:?}");
}
