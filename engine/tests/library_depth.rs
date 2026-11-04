//! How deep Atlas reads into a library is an option of the reading, and what the graph keeps of a
//! library is a restriction of the code it read: a kept node's facts are those it has in all of it,
//! whether the program reaches the code they come from or not.

use std::collections::BTreeSet;
use std::path::PathBuf;

use atlas_engine::{analyze, Graph, Options, Touch, World};

/// One round reads the library modules the project's calls go into; a module of the library
/// that only those call is read in the second.
#[test]
fn each_round_reads_one_step_further_into_a_library() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture-lib");
    let has = |libraries: usize, id: &str| {
        let opts = Options { libraries, ..Options::default() };
        analyze(&repo, &opts).expect("fixture parses").nodes.iter().any(|n| n.id == id)
    };
    assert!(!has(0, "plotkit.data.load"), "no round reads no library");
    assert!(has(1, "plotkit.data.lines"), "the module the app calls is read");
    assert!(!has(1, "plotkit.rows.split"), "a module only the library calls waits for the next round");
    assert!(has(2, "plotkit.rows.split"));
}

/// The crate `app`, whose `main` is `main`, read with the library crate `notekit` beside it, whose
/// `lib.rs` is `library`; both are written to a directory of their own and removed.
fn notekit(name: &str, library: &str, main: &str) -> Graph {
    let dir = std::env::temp_dir().join(format!("atlas-depth-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let files = [
        (
            "app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nnotekit = { path = \"../notekit\" }\n",
        ),
        ("app/src/main.rs", main),
        ("notekit/Cargo.toml", "[package]\nname = \"notekit\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
        ("notekit/src/lib.rs", library),
    ];
    for (path, text) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let g = analyze(&dir.join("app"), &Options::default());
    let _ = std::fs::remove_dir_all(&dir);
    g.expect("the crates parse")
}

/// A class the graph keeps is a door where any of its operations reaches a service, whether the
/// program calls that operation or not: `Shell` runs a command in `run`, and is the same door when
/// the program only asks its `name` as when it runs it too.
#[test]
fn a_kept_class_is_a_door_by_every_operation_it_has() {
    let library = "use std::process::Command;\n\npub struct Shell {\n    program: String,\n}\n\nimpl Shell {\n    pub fn new(program: &str) -> Shell {\n        Shell { program: program.to_string() }\n    }\n\n    pub fn name(&self) -> &str {\n        &self.program\n    }\n\n    pub fn run(&self) -> bool {\n        Command::new(&self.program).status().is_ok()\n    }\n}\n";
    let named =
        notekit("named", library, "fn main() {\n    let shell = notekit::Shell::new(\"ls\");\n    println!(\"{}\", shell.name());\n}\n");
    let run = notekit(
        "run",
        library,
        "fn main() {\n    let shell = notekit::Shell::new(\"ls\");\n    println!(\"{}\", shell.name());\n    shell.run();\n}\n",
    );
    assert!(named.node("notekit.Shell.run").is_none() && run.node("notekit.Shell.run").is_some());
    let door = |g: &Graph| g.node("notekit.Shell").expect("the class the program holds").door;
    assert_eq!(door(&named), Some(World::Process));
    assert_eq!(door(&named), door(&run));
}

/// A path a library's function opens is made whole by every call of that function, whether the
/// program reaches the caller or not: `defaults` hands `open` its own file, and `open`'s reading is
/// the same when the program calls `defaults` too.
#[test]
fn a_caller_the_program_does_not_reach_still_makes_a_path_whole() {
    let library = "use std::fs;\n\npub fn open(path: &str) -> String {\n    fs::read_to_string(path).unwrap_or_default()\n}\n\npub fn defaults() -> String {\n    open(\"defaults.toml\")\n}\n";
    let alone = notekit("alone", library, "fn main() {\n    notekit::open(\"settings.toml\");\n}\n");
    let both = notekit("both", library, "fn main() {\n    notekit::open(\"settings.toml\");\n    notekit::defaults();\n}\n");
    assert!(alone.node("notekit.defaults").is_none() && both.node("notekit.defaults").is_some());
    let opened = |g: &Graph| {
        let n = g.node("notekit.open").expect("the function the program calls");
        let touches = |ts: &[Touch]| ts.iter().map(|t| serde_json::to_string(t).unwrap()).collect::<BTreeSet<_>>();
        (touches(&n.touches), touches(&n.instead))
    };
    let (touches, _) = opened(&alone);
    assert!(touches.iter().any(|t| t.contains("defaults.toml")), "{touches:?}");
    assert_eq!(opened(&alone), opened(&both));
}

/// A library function's parameter holds a value where any call of the function fills it with one,
/// whether the program reaches that call or not: `greet` hands `show` a value its own step makes,
/// and `show`, which hands its parameter on, has the same steps when the program calls `greet` too.
#[test]
fn a_caller_the_program_does_not_reach_still_fills_a_parameter() {
    let library = "pub fn width(text: &str) -> usize {\n    text.len()\n}\n\npub fn show(label: &str) -> usize {\n    width(label)\n}\n\npub fn greet() -> usize {\n    let name = String::from(\"notekit\");\n    show(&name)\n}\n";
    let alone = notekit("show", library, "fn main() {\n    notekit::show(\"settings\");\n}\n");
    let both = notekit("greet", library, "fn main() {\n    notekit::show(\"settings\");\n    notekit::greet();\n}\n");
    assert!(alone.node("notekit.greet").is_none() && both.node("notekit.greet").is_some());
    let steps = |g: &Graph| serde_json::to_string(&g.node("notekit.show").expect("the function the program calls").sites).unwrap();
    assert!(steps(&alone).contains("\"label\""), "{}", steps(&alone));
    assert_eq!(steps(&alone), steps(&both));
}
