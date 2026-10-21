//! A log changed in place, through `&mut` and through a `RefCell`.

use std::cell::RefCell;

pub struct Log {
    pub lines: Vec<String>,
}

fn note_mut(log: &mut Log, s: &str) -> usize {
    log.lines.push(s.to_string());
    log.lines.len()
}

fn note_cell(log: &RefCell<Log>, s: &str) -> usize {
    log.borrow_mut().lines.push(s.to_string());
    log.borrow().lines.len()
}

fn note_cell_unit(log: &RefCell<Log>, s: &str) {
    log.borrow_mut().lines.push(s.to_string());
}

pub fn main_like() -> usize {
    let mut log = Log { lines: vec![] };
    let cell = RefCell::new(Log { lines: vec![] });
    note_cell_unit(&cell, "c");
    note_mut(&mut log, "a") + note_cell(&cell, "b")
}
