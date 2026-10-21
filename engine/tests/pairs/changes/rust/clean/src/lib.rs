//! A log a line is noted in: the signature says the log changes, as `&mut` or as a cell.

use std::cell::RefCell;

pub struct Log {
    pub lines: Vec<String>,
}

fn note(log: &mut Log, line: &str) {
    log.lines.push(line.to_string());
}

fn note_shared(log: &RefCell<Log>, line: &str) {
    log.borrow_mut().lines.push(line.to_string());
}

pub fn run() -> usize {
    let mut log = Log { lines: vec![] };
    note(&mut log, "a");
    let shared = RefCell::new(Log { lines: vec![] });
    note_shared(&shared, "b");
    log.lines.len() + shared.borrow().lines.len()
}
