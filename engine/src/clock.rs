//! How long a reading spends in each of its stages, per part and per library: what `atlas-code view
//! --timings` reports, for a benchmark to read (`tools/perf.py`). The clock runs on the thread that
//! switches it on (`start`), and only there; off, a stage costs one look at a flag, and nothing it
//! measures reaches the graph.
//!
//! A stage is timed by the guard `stage` or `of` returns, from its making to its drop. A stage within
//! another is not the outer one's: each stage's seconds are its own, so the stages of a reading add up to
//! the time they cover. Guards end in the order opposite to their making, as scoped values do.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

thread_local! {
    static CLOCK: RefCell<Clock> = RefCell::new(Clock::default());
}

/// One stage of a part's reading, for one crate or package or for the whole part: its own seconds, and
/// how many times it ran (files parsed, bodies linked).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Stage {
    /// The part read: its directory in the repository, `.` at its root.
    pub part: String,
    pub stage: &'static str,
    /// The crate or package whose code the stage read; none for a stage of the whole part.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub of: Option<String>,
    /// Whether that crate or package is a library's.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub library: bool,
    pub seconds: f64,
    pub count: u64,
}

/// Where a stage's time goes: the part, the stage, and the crate or package with whether it is a
/// library's.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    part: String,
    stage: &'static str,
    of: Option<(String, bool)>,
}

struct Open {
    key: Key,
    began: Instant,
    /// The time the stages within it took.
    within: Duration,
}

#[derive(Default)]
struct Clock {
    on: bool,
    part: String,
    /// The parts in the order they were first read.
    parts: Vec<String>,
    open: Vec<Open>,
    taken: HashMap<Key, (Duration, u64)>,
}

/// Whether the clock runs on this thread.
pub fn running() -> bool {
    CLOCK.with(|c| c.borrow().on)
}

/// Switch the clock on for this thread, with nothing taken yet.
pub fn start() {
    CLOCK.with(|c| *c.borrow_mut() = Clock { on: true, ..Clock::default() });
}

/// What the clock took on this thread since it was switched on: each stage once per part and crate, the
/// parts in the order they were read, within one the stages, then the program's code before the
/// libraries', each by name.
pub fn taken() -> Vec<Stage> {
    CLOCK.with(|c| {
        let c = c.borrow();
        let mut out: Vec<(usize, Stage)> = c
            .taken
            .iter()
            .map(|(k, (took, count))| {
                let (of, library) = k.of.clone().map_or((None, false), |(o, l)| (Some(o), l));
                let rank = c.parts.iter().position(|p| *p == k.part).unwrap_or(usize::MAX);
                (rank, Stage { part: k.part.clone(), stage: k.stage, of, library, seconds: took.as_secs_f64(), count: *count })
            })
            .collect();
        out.sort_by(|(ra, a), (rb, b)| ra.cmp(rb).then(a.stage.cmp(b.stage)).then(a.library.cmp(&b.library)).then(a.of.cmp(&b.of)));
        out.into_iter().map(|(_, s)| s).collect()
    })
}

/// The part being read while the guard lives: its directory in the repository, `.` at its root.
pub fn part(dir: &str) -> Part {
    let was = CLOCK.with(|c| {
        let mut c = c.borrow_mut();
        if !c.on {
            return None;
        }
        let dir = if dir.is_empty() { ".".to_string() } else { dir.to_string() };
        if !c.parts.contains(&dir) {
            c.parts.push(dir.clone());
        }
        Some(std::mem::replace(&mut c.part, dir))
    });
    Part { was }
}

/// The part being read, while it lives.
#[must_use]
pub struct Part {
    was: Option<String>,
}

impl Drop for Part {
    fn drop(&mut self) {
        if let Some(was) = self.was.take() {
            CLOCK.with(|c| c.borrow_mut().part = was);
        }
    }
}

/// A stage of the whole part, timed while the guard lives.
pub fn stage(stage: &'static str) -> Span {
    open(stage, || None)
}

/// A stage that reads the code of the crate or package `of`, a library's when `library`, timed while
/// the guard lives.
pub fn of(stage: &'static str, of: &str, library: bool) -> Span {
    open(stage, || Some((of.to_string(), library)))
}

fn open(stage: &'static str, of: impl FnOnce() -> Option<(String, bool)>) -> Span {
    let on = CLOCK.with(|c| {
        let mut c = c.borrow_mut();
        if !c.on {
            return false;
        }
        let key = Key { part: c.part.clone(), stage, of: of() };
        c.open.push(Open { key, began: Instant::now(), within: Duration::ZERO });
        true
    });
    Span { on }
}

/// A stage being timed, while it lives.
#[must_use]
pub struct Span {
    on: bool,
}

impl Drop for Span {
    fn drop(&mut self) {
        if !self.on {
            return;
        }
        CLOCK.with(|c| {
            let mut c = c.borrow_mut();
            let Some(open) = c.open.pop() else { return };
            let took = open.began.elapsed();
            if let Some(outer) = c.open.last_mut() {
                outer.within += took;
            }
            let entry = c.taken.entry(open.key).or_default();
            entry.0 += took.saturating_sub(open.within);
            entry.1 += 1;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stage within another is not the outer one's: their seconds add up to no more than the time
    /// both cover, and each is at least its own.
    #[test]
    fn a_stage_within_another_is_its_own() {
        start();
        let began = Instant::now();
        {
            let _part = part("p");
            let _outer = stage("outer");
            std::thread::sleep(Duration::from_millis(20));
            let _inner = of("inner", "lib", true);
            std::thread::sleep(Duration::from_millis(20));
        }
        let covered = began.elapsed().as_secs_f64();
        let took = taken();
        let seconds = |s: &str| took.iter().find(|t| t.stage == s).map(|t| t.seconds).unwrap();
        assert!(seconds("inner") >= 0.02 && seconds("outer") >= 0.02);
        assert!(seconds("outer") + seconds("inner") <= covered);
        let inner = took.iter().find(|t| t.stage == "inner").unwrap();
        assert_eq!((inner.of.as_deref(), inner.library), (Some("lib"), true));
        assert!(took.iter().all(|t| t.part == "p" && t.count == 1));
    }

    /// Off, the clock takes nothing.
    #[test]
    fn off_it_takes_nothing() {
        std::thread::spawn(|| {
            drop(stage("x"));
            drop(of("y", "lib", true));
            assert!(!running() && taken().is_empty());
        })
        .join()
        .unwrap();
    }
}
