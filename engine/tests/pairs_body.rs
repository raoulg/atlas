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

//!
//! Beside them, what a call says of each value it hands, the parameter it fills (`Held::fills`, `Site::args`, #157),
//! what an arm binds by the body's own code (`Node::binds`, #158), and a function a call runs on each element
//! (`Steps::handed`), read on the programs of the other pairs where the code writes them.

// the helpers the prophecy pairs use stand beside the one this file does
#[allow(dead_code)]
mod common;

use std::path::PathBuf;

use atlas_engine::{analyze, Graph, Node, Options, Site, Slot};

/// The program at `dir` below `engine/tests/pairs`.
fn program(dir: &str) -> Graph {
    analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs").join(dir), &Options::default()).unwrap()
}

/// What a site hands, by the name that holds each value (its type where no name holds it), with the parameters it fills.
fn fills(s: &Site) -> Vec<(String, Vec<Slot>)> {
    let named = |h: &atlas_engine::Held| if h.name.is_empty() || h.name.starts_with('#') { h.ty.clone() } else { h.name.clone() };
    s.hands.iter().map(|h| (named(h), h.fills.clone())).collect()
}

fn at(n: u32) -> Slot {
    Slot::At(n)
}

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
        assert_eq!(site(&p.clean, &total, &parse).fails, [error], "{language}: total hands on what parse raises");
        assert_eq!(site(&p.clean, route, &total).fails, [error], "{language}: and the route's call of total");
        let builtin = site(&p.shortcut, &parse, library);
        assert!(builtin.raises && builtin.gives.iter().all(|g| g.ty.starts_with("ext:")), "{language}: a library's class: {builtin:#?}");
        assert_eq!(site(&p.shortcut, &total, &parse).fails, Vec::<String>::new(), "{language}: the handler around the call catches it");
        assert_eq!(site(&p.shortcut, route, &total).fails, Vec::<String>::new(), "{language}: so the route never hears of it");
    }
    let p = common::pair("errors/rust");
    let raised = site(&p.clean, "counts.parse", "counts.Error");
    assert!(raised.raises, "the arm's `Err(Error::NotACount(..))`, the last value of parse, raises it");
    assert_eq!(site(&p.clean, "counts.total", "counts.parse").fails, ["counts.Error"], "`?` hands it on as the program's");
    assert_eq!(site(&p.clean, "counts.main", "counts.total").fails, ["counts.Error"]);
    assert_eq!(site(&p.shortcut, "counts.total", "counts.parse").fails, [""], "a `Box<dyn Error>` is no type of the code's");
    assert_eq!(site(&p.shortcut, "counts.main", "counts.total").fails, [""]);
}

/// A call into a family runs the method or one of its overrides, decided at run time, so it fails with each error any of
/// them can end with: its codomain is the coproduct of theirs (#213). The clean sources fail as their base says a source
/// fails, with the program's one error; the shortcut's fail each their own way, and the call through the base, and the
/// route's call of it, hand on both. A reading is a function of the code: two readings hand on the same errors.
#[test]
fn errors_a_call_into_a_family_fails_with_what_each_override_can() {
    let ids = [
        ("python", "settings.", "settings.run", ["settings.SourceError"], ["ext:builtins.RuntimeError", "ext:builtins.ValueError"]),
        (
            "typescript",
            "settings.cli.",
            "settings.cli.main",
            ["settings.cli.SourceError"],
            ["ext:javascript.Error", "ext:javascript.TypeError"],
        ),
    ];
    for (language, own, route, clean, shortcut) in ids {
        let (p, again) = (common::pair(&format!("failures/{language}")), common::pair(&format!("failures/{language}")));
        let (load, settings) = (format!("{own}Source.load"), format!("{own}settings"));
        for (g, want) in [(&p.clean, &clean[..]), (&p.shortcut, &shortcut[..])] {
            assert_eq!(site(g, &settings, &load).fails, want, "{language}: the call through the base");
            assert_eq!(site(g, route, &settings).fails, want, "{language}: and the route's call of it");
        }
        for (one, other) in [(&p.clean, &again.clean), (&p.shortcut, &again.shortcut)] {
            assert_eq!(site(one, &settings, &load).fails, site(other, &settings, &load).fails, "{language}: two readings alike");
        }
    }
}

/// A call hands each value to the parameter its argument fills (#157), by where the call writes it, never by its type:
/// the value it runs on, a place after it, a name, a spread whose parameters the call does not say. A value written in
/// an argument fills that argument's parameter, also through a projection, and what a step written inside gives fills
/// the place the step stands in. The parameters the call writes an argument for are `args`: one with no value handed
/// (a literal) holds none, one not written holds its default.
#[test]
fn a_call_hands_each_value_to_the_parameter_its_argument_fills() {
    // Rust: `report::write(Path::new(REPORTS), format.renderer().as_ref(), &config.title, &lines)`
    let g = program("rust/clean");
    let write = site(&g, "spend.main", "spend.report.write");
    assert_eq!(write.args, [at(0), at(1), at(2), at(3)], "the directory is written, by a literal");
    let want = [("config", vec![at(2)]), ("lines", vec![at(3)]), ("spend.report.Render", vec![at(1)])];
    assert_eq!(fills(write), want.map(|(n, s)| (n.to_string(), s)), "the renderer a nested step gives, in its own place");
    // `Trend::new(settings).save(&series, &smoothed, &cli.figure)` and `download(SILSO, &cli.raw)`
    let g = program("pipeline/rust-clean");
    let save = site(&g, "sunspots.main.main", "sunspots.plot.Plot.save");
    assert_eq!(save.args, [Slot::Receiver, at(0), at(1), at(2)]);
    assert!(
        fills(save).contains(&("sunspots.plot.Trend".to_string(), vec![Slot::Receiver])),
        "the nested step's result runs it: {save:#?}"
    );
    assert!(fills(save).contains(&("cli".to_string(), vec![at(2)])), "`&cli.figure` is the third: {save:#?}");
    let download = site(&g, "sunspots.main.main", "sunspots.download.download");
    assert_eq!((download.args.clone(), fills(download)), (vec![at(0), at(1)], vec![("cli".to_string(), vec![at(1)])]));
    // TypeScript: `download(SILSO, values.raw)`, `{ ...DEFAULT_SETTINGS, months: monthsOf(values.months) }`
    let g = program("pipeline/typescript");
    let download = site(&g, "sunspots.cli.main", "sunspots.download.download");
    assert_eq!((download.args.clone(), fills(download)), (vec![at(0), at(1)], vec![("values".to_string(), vec![at(1)])]));
    let settings = site(&g, "sunspots.cli.main", "sunspots.settings.Settings");
    assert_eq!(settings.args, [Slot::Keywords, Slot::Name("months".into())], "an object literal writes by name");
    assert_eq!(
        fills(settings),
        [("DEFAULT_SETTINGS".to_string(), vec![Slot::Keywords]), (String::new(), vec![Slot::Name("months".into())])],
        "what the spread hands fills the names the literal does not write"
    );
    // Python: `Harvester(client=.., store=.., exporter=.., unit=settings.unit)`, `Summary(day.station, day.date, *row)`
    let g = program("python/clean");
    let built = site(&g, "harvest.harvester.build_harvester", "harvest.harvester.Harvester");
    assert!(fills(built).contains(&("settings".to_string(), vec![Slot::Name("unit".into())])), "{built:#?}");
    let summary = site(&g, "harvest.store.ReadingStore.kept_summary", "harvest.model.Summary");
    assert_eq!(summary.args, [at(0), at(1), Slot::Spread(2)]);
    assert_eq!(fills(summary), [("day".to_string(), vec![at(0), at(1)]), ("row".to_string(), vec![Slot::Spread(2)])]);
    let g = program("pipeline/python-clean");
    let save = site(&g, "sunspots.cli.run", "sunspots.plot.Plot.save");
    assert!(fills(save).contains(&("sunspots.plot.Trend".to_string(), vec![Slot::Receiver])), "{save:#?}");
    // a function or a library's call runs on no value it is not handed: `pd.read_csv(..)` is written in its module
    let read = site(&g, "sunspots.series.load", "read_csv");
    assert!(!read.args.contains(&Slot::Receiver), "{read:#?}");
}

/// A name an arm of a choice binds by the body's own code is made by that arm (#158), as one a step in the arm gives:
/// `let mut out = match format { "csv" => String::from(..), _ => format!(..) }` makes `out` in both arms, the first by
/// its step (`Site::gives`) and the second by `format!`, no step (`Node::binds`). The clean report renders through a
/// trait whose method makes its line with `format!` outside any choice, so it binds nothing in an arm.
#[test]
fn an_arm_that_binds_a_name_by_its_own_code_makes_it() {
    let p = common::pair("rust");
    let write = node(&p.shortcut, "spend.report.write");
    let from = write.sites.iter().find(|s| s.op == "from" && s.gives.iter().any(|g| g.name == "out")).expect("`String::from` gives out");
    assert_eq!(from.arms, [(0, 0)]);
    let out: Vec<&Vec<(u32, u32)>> = write.binds.iter().filter(|b| b.name == "out").map(|b| &b.arms).collect();
    assert!(out.contains(&&vec![(0, 1)]), "the `_` arm binds out by `format!`: {:#?}", write.binds);
    assert!(!out.contains(&&vec![(0, 0)]), "the csv arm's step gives it");
    assert!(node(&p.clean, "spend.report.Markdown.render").binds.iter().all(|b| b.name != "out"), "bound outside every choice");
    // the same in Python and TypeScript: what follows a guard is its last arm
    let g = program("python/shortcut");
    let names: Vec<&str> = node(&g, "harvest.store.ReadingStore.summary").binds.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["temperatures", "low", "high", "mean"], "`if row is None:` binds them by its own code");
    let g = program("pipeline/typescript");
    let trend = node(&g, "sunspots.plot.trend");
    assert!(trend.binds.iter().any(|b| b.name == "bottom" && b.arms == [(0, 1)]), "{:#?}", trend.binds);
}

/// What a step gives that the body's own code reads by no name is a use (#101), named by the step that makes it:
/// `mode.plan().timeout <= ..`, `ledger.merged(other).rows.length`, `this.rows().map((row) => row.reading)`. A result a
/// step around it is handed, bound to a name, or given back as it is, is none.
#[test]
fn what_a_step_gives_and_the_body_reads_itself_is_a_use() {
    for (dir, body, to, ty) in [
        ("choices/rust/clean", "runs.finished", "runs.Mode.plan", "runs.Plan"),
        ("changes/typescript/clean", "ledger.ledger.main", "ledger.ledger.Ledger.merged", "ledger.ledger.Ledger"),
        ("collections/typescript/clean", "gathering.gathering.Store.get", "gathering.gathering.Store.rows", "gathering.gathering.Row"),
        ("collections/rust/clean", "gathering.Store.get", "gathering.Store.rows", "gathering.Row"),
        ("python/clean", "harvest.store.ReadingStore.history", "harvest.model.Reading", "harvest.model.Reading"),
    ] {
        let g = program(dir);
        let s = site(&g, body, to);
        let given = s.gives.iter().find(|h| h.ty == ty).unwrap_or_else(|| panic!("{dir}: {to} gives a {ty}: {s:#?}"));
        assert!(given.name.starts_with(&format!("#{body}@")), "{dir}: named by its step: {given:?}");
        assert!(
            node(&g, body).uses.iter().any(|u| u.name == given.name && u.ty == ty),
            "{dir}: the body reads it: {:#?}",
            node(&g, body).uses
        );
    }
    // handed to the step around it, it is that step's
    let g = program("choices/rust/clean");
    let batch = node(&g, "runs.finished").sites.iter().find(|s| s.to == "runs.Runner.new").unwrap();
    assert!(batch.gives.iter().all(|h| !h.name.starts_with("#runs.finished@")), "{batch:#?}");
}

/// A function a call runs on each element of what it goes over is one form in every language (`Steps::handed`): a step
/// where it is named, handed what the element is a part of, which fills its first parameter. Rust's
/// `piles.iter().map(size)`, Python's `sorted(readings, key=score)`, TypeScript's `.map(parseLine)`.
#[test]
fn a_function_run_on_each_element_is_handed_the_element() {
    let g = program("shadows/rust/shortcut");
    let size = node(&g, "inbox.total").sites.iter().find(|s| s.to == "inbox.size").expect("`map(size)` runs size");
    assert_eq!((size.args.clone(), fills(size)), (vec![at(0)], vec![("piles".to_string(), vec![at(0)])]));
    let g = program("gives/python/clean");
    assert_eq!(site(&g, "readings.store.ranked", "readings.store.score").args, [at(0)], "`key=score` is handed each reading");
    let g = program("pipeline/typescript");
    assert_eq!(site(&g, "sunspots.series.parse", "sunspots.series.parseLine").args, [at(0)], "`.map(parseLine)` hands it each line");
}
