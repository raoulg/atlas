//! The calls a reader could not follow, by what the name each starts from is bound to. Only the
//! unknown ones count against the engine; the rest are the language's own (`Some`, `fetch`), a
//! library's (`std::fs::read`, `path.join`), or a value the body was handed or made itself.

use std::path::PathBuf;

use atlas_engine::{analyze, Graph, Options, Unresolved};

fn read(dir: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(dir), &Options::default()).unwrap()
}

/// The buckets of a graph's calls not followed, each such call in exactly one.
fn buckets(g: &Graph) -> Unresolved {
    let u = g.stats.unresolved_calls;
    assert_eq!(u.builtin + u.library + u.local + u.unknown, g.stats.unresolved_calls_total, "{}: {u:?}", g.root);
    u
}

#[test]
fn a_call_not_followed_is_bound_somewhere() {
    // where ty finds each name defined: `fn(..)`, `cls(..)`, `x.mean`, `note.model_dump_json` and
    // `con.execute` start from a parameter or a local; `READERS[kind](..)` from the module's table
    // and `Note.model_validate_json` from the program's own class, which the engine does not follow
    assert_eq!(buckets(&read("tests/fixture")), Unresolved { builtin: 0, library: 0, local: 9, unknown: 2 });
    // `Some`, `Ok` and `String::new` are the prelude's; `std::fs`, `std::io`, `reqwest::blocking`
    // and `rusqlite::Connection` crates the program depends on
    assert_eq!(buckets(&read("tests/fixture-rs")), Unresolved { builtin: 4, library: 19, local: 0, unknown: 0 });
    // `fetch`, `console`, `process` and `localStorage` are globals, and `uit.push` the language's
    // own on an array the body declares; `path.join` and `useState` imports, `pool.query` and
    // `app.get` calls on what a package made, as are the calls on what a package's call gives
    // (`expect(..).toBe`, `createServerFn(..).handler`, `createFileRoute("/kaart")(..)`); `c.json`
    // a handler's parameter
    assert_eq!(buckets(&read("tests/fixture-ts")), Unresolved { builtin: 11, library: 34, local: 5, unknown: 0 });
}
