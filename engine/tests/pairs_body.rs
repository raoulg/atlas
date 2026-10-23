//! What a body does with values outside its steps, read once for every language (`body.rs`, `Steps`): what its own
//! code uses (`Node::uses`, #101, #126), what it throws away of what a step gives (`Site::dropped`, #134), and the
//! errors it ends with and hands on (`Site::raises`, `Site::fails`, #138). Each language supplies its syntax; the walk is
//! the same. Two pairs, one program per language each:
//!
//! - `uses`: a report of a store of notes. `clean` hands each value to the step that uses it and uses what the last
//!   step gives back; `shortcut` makes the line of the store itself and throws away what the step that saves it gives.
//! - `errors`: the total of the counts some lines hold. `clean` raises an error of the program's own type where a line
//!   holds no count, and each border hands it on up to the route; `shortcut` raises the language's catch-all error and
//!   swallows it (Python, TypeScript), or boxes it into one of no type of the program's (Rust).

// the helpers the prophecy pairs use stand beside the one this file does
#[allow(dead_code)]
mod common;

use atlas_engine::{Graph, Node, Site};

fn node<'g>(g: &'g Graph, id: &str) -> &'g Node {
    g.node(id).unwrap_or_else(|| panic!("no node {id}"))
}

/// The site of `body` that reaches `to`, or runs the operation `to`.
fn site<'g>(g: &'g Graph, body: &str, to: &str) -> &'g Site {
    let n = node(g, body);
    n.sites.iter().find(|s| s.to == to || s.op == to).unwrap_or_else(|| panic!("{body} has no site {to}: {:#?}", n.sites))
}

/// The names the body's own code reads, each once.
fn used(g: &Graph, body: &str) -> Vec<String> {
    let mut names: Vec<String> = node(g, body).uses.iter().map(|u| u.name.clone()).collect();
    names.dedup();
    names
}

/// Each language's route, and the id the program gives its own types, in the `uses` pair.
const USES: [(&str, &str, &str); 3] =
    [("python", "notes.run", "notes."), ("rust", "notes.main", "notes."), ("typescript", "notes.cli.main", "notes.cli.")];

/// A body's own code uses what it reads outside its steps (#101): a value handed to a step is the step's, and a value
/// the body formats itself is the body's. The clean route reads only what its last step gave back (`print(kept)`); the
/// shortcut makes the line of the store itself, so its own arrow takes the store, by its name and of its type.
#[test]
fn uses_the_body_reads_what_it_hands_no_step() {
    for (language, route, own) in USES {
        let p = common::pair(&format!("uses/{language}"));
        assert_eq!(used(&p.clean, route), ["kept"], "{language}: the clean route hands the store to the steps that read it");
        let store = node(&p.shortcut, route).uses.iter().find(|u| u.name == "store");
        let store =
            store.unwrap_or_else(|| panic!("{language}: the shortcut reads the store itself: {:#?}", node(&p.shortcut, route).uses));
        assert_eq!(store.ty, format!("{own}Store"), "{language}: of the program's type, as a step it is handed has it");
        // the step that makes the line in the clean route reads the store itself, its parameter
        let describe = format!("{own}describe");
        assert!(node(&p.clean, &describe).uses.iter().any(|u| u.name == "store"), "{language}: `describe` formats the store");
    }
}

/// A body throws away what a call gives where the call is all a statement is made of (#134): `save(store, line)`,
/// `save(&store, &line);`, `void save(store, line)`. A result the body names is not thrown away.
#[test]
fn dropped_a_call_that_is_a_statement_of_its_own() {
    for (language, route, own) in USES {
        let p = common::pair(&format!("uses/{language}"));
        let save = format!("{own}save");
        assert!(!site(&p.clean, route, &save).dropped, "{language}: the clean route keeps what save gives");
        assert!(site(&p.shortcut, route, &save).dropped, "{language}: the shortcut throws it away");
    }
}

/// An error is a value (#138). Where the code builds an error to raise, the constructor is a step that raises it, of
/// its class whatever library it is of (`NotACount(..)`, `RuntimeError(..)`, `new Error(..)`, `Err(Error::NotACount(..))`).
/// A call hands on what its callee raises uncaught, as Python and TypeScript propagate it and as Rust's `?` converts it
/// to the error type of the function's signature. The clean program's error reaches the route by the program's own
/// type; the shortcut's handler catches what is raised, or `?` converts it to a box of no type of the program's.
#[test]
fn errors_a_raise_is_a_step_and_each_call_hands_it_on() {
    let ids = [
        ("python", "counts.", "counts.run", "counts.NotACount", "RuntimeError()"),
        ("typescript", "counts.cli.", "counts.cli.main", "counts.cli.NotACount", "Error()"),
    ];
    for (language, own, route, error, library) in ids {
        let p = common::pair(&format!("errors/{language}"));
        let (parse, total) = (format!("{own}parse"), format!("{own}total"));
        let raised = site(&p.clean, &parse, error);
        assert!(raised.raises, "{language}: the constructor raises");
        assert_eq!(raised.gives.iter().map(|g| g.ty.as_str()).collect::<Vec<_>>(), [error], "{language}: of its class");
        assert_eq!(site(&p.clean, &total, &parse).fails.as_deref(), Some(error), "{language}: total hands on what parse raises");
        assert_eq!(site(&p.clean, route, &total).fails.as_deref(), Some(error), "{language}: and the route's call of total");
        let builtin = site(&p.shortcut, &parse, library);
        assert!(builtin.raises && builtin.gives.iter().all(|g| g.ty.starts_with("ext:")), "{language}: a library's class: {builtin:#?}");
        assert_eq!(site(&p.shortcut, &total, &parse).fails, None, "{language}: the handler around the call catches it");
        assert_eq!(site(&p.shortcut, route, &total).fails, None, "{language}: so the route never hears of it");
    }
    let p = common::pair("errors/rust");
    let raised = site(&p.clean, "counts.parse", "counts.Error");
    assert!(raised.raises, "the arm's `Err(Error::NotACount(..))`, the last value of parse, raises it");
    assert_eq!(site(&p.clean, "counts.total", "counts.parse").fails.as_deref(), Some("counts.Error"), "`?` hands it on as the program's");
    assert_eq!(site(&p.clean, "counts.main", "counts.total").fails.as_deref(), Some("counts.Error"));
    assert_eq!(site(&p.shortcut, "counts.total", "counts.parse").fails.as_deref(), Some(""), "a `Box<dyn Error>` is no type of the code's");
    assert_eq!(site(&p.shortcut, "counts.main", "counts.total").fails.as_deref(), Some(""));
}
