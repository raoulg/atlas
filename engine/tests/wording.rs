//! What Atlas says, kept once. A finding's sentences are how Atlas coaches: they are a contract of
//! `atlas check`, never a law. Every other test asks for what a finding is about (its principle,
//! its node, the facts in the graph that show it), so that a sentence reworded fails here and only
//! here, where it is read as wording. For each principle one finding of its shortcut, said as Atlas
//! says it today: when a sentence changes on purpose, change it here.

use std::path::PathBuf;

use atlas_engine::principles::{check, Principle};
use atlas_engine::{analyze, Options};

struct Said {
    principle: &'static str,
    pair: &'static str,
    node: &'static str,
    what: &'static str,
    question: &'static str,
}

const SAID: &[Said] = &[
    Said {
        principle: "doors",
        pair: "doors/python/shortcut",
        node: "units.convert.parse",
        what: "parse does not say what `text` is, nor what it gives",
        question: "Until its signature says so, a reader and Atlas both lose the route at this door. What does parse take and give?",
    },
    Said {
        principle: "sums",
        pair: "python/shortcut",
        node: "harvest.model.Fetched",
        what: "Fetched (Reading | Outage) is handed on unopened: show and unreached take it",
        question: "Each of them chooses again between Reading and Outage. Does each need all of them, or could the code that makes a Fetched hand on only what each one uses?",
    },
    Said {
        principle: "overrides",
        pair: "overrides/python/shortcut",
        node: "reading.counts.Shelter.count",
        what: "Shelter.count promises () -> int; 1 override breaks it",
        question: "Can every caller of Shelter.count take Kennel.count in its place? If not, is Kennel a Shelter, or does it use one?",
    },
    Said {
        principle: "places",
        pair: "places/python/shortcut",
        node: "stock.rows.default_rows",
        what: "default_rows decides itself where data lives: data/rows.csv",
        question: "Taken as a parameter, a script, a test and a notebook can each choose. Who should decide where this lives: default_rows, or whoever calls it?",
    },
    Said {
        principle: "changes",
        pair: "changes/python/shortcut",
        node: "board.core.Board.take_from",
        what: "Board.take_from changes the Board it is handed as `other`",
        question: "Nothing in the signature shows that `other` changes. Should the Board change itself through a method of its own, or should Board.take_from hand back a new Board?",
    },
    Said {
        principle: "gives",
        pair: "gives/python/shortcut",
        node: "readings.store.get_store",
        what: "get_store gives back a value its callers use, and does not say what it is",
        question: "Without a return annotation, what its callers get has no type, and the route stops there. What does get_store give back?",
    },
    Said {
        principle: "choices",
        pair: "choices/python/shortcut",
        node: "runs.core.Runner",
        what: "Runner.batch and Runner.workers test `self.mode` in 2 places; \"parallel\" (2×) come back",
        question: "What each case means is spelled out again in each place, so a new case is a change in all of them. Could the choice be made once, in a table or a type that says what each `self.mode` stands for?",
    },
    Said {
        principle: "nothing",
        pair: "nothing/python/shortcut",
        node: "jobs.runner.Job.finish",
        what: "Job.finish gives nothing back, and Atlas cannot see what `self.on_done(..)` does",
        question: "Atlas sees no change and no place in what Job.finish runs, but it cannot see what `self.on_done(..)` does. Does that leave something behind which the picture lacks (a log, a change in a library's or a class's value, a wait), or is Job.finish dead?",
    },
    Said {
        principle: "layers",
        pair: "layers/python/shortcut",
        node: "kitchen",
        what: "kitchen and shop depend on one another",
        question: "Packages in a cycle have no order: none can be read, tested or replaced without the others. Which of these links runs against the flow, and where does what it uses belong?",
    },
    Said {
        principle: "lens",
        pair: "lens/python/club/shortcut",
        node: "club.desk.Desk",
        what: "Desk puts Visit into SQLite(visits) and gets no Visit back",
        question: "A lens gives back what it is given: put then get. Desk puts Visit, and none of its gets gives that back. What does SQLite(visits) hold, and could Desk get it back as it went in?",
    },
    Said {
        principle: "prophecy",
        pair: "prophecy/python/shortcut",
        node: "archive.app.run",
        what: "run leans on `Store`, which is not built",
        question: "Production would run on a stub here. Build `Store` first, or is run meant to change with it, as a prophecy that supersedes it?",
    },
];

#[test]
fn each_principle_says_what_it_said() {
    let mut wrong = vec![];
    for said in SAID {
        let g = analyze(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pairs").join(said.pair), &Options::default()).unwrap();
        let found = check(&g).into_iter().find(|f| f.principle == said.principle && f.node == said.node);
        let Some(f) = found else {
            wrong.push(format!("{}: no finding on {} in {}", said.principle, said.node, said.pair));
            continue;
        };
        if f.what != said.what || f.question != said.question {
            wrong.push(format!("{} on {}\n  what:     {}\n  question: {}", said.principle, said.node, f.what, f.question));
        }
    }
    assert!(wrong.is_empty(), "the wording changed; read each and, if it is meant, say it here:\n{}", wrong.join("\n"));
}

#[test]
fn every_principle_has_its_wording_kept() {
    for p in Principle::ALL {
        assert!(SAID.iter().any(|s| p == s.principle), "{p} has no sentence in SAID");
    }
}
