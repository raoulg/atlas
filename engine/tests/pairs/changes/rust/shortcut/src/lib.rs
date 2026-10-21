//! A log a line is noted in: the log is borrowed to read, so the line goes into a copy.

pub struct Log {
    pub lines: Vec<String>,
}

fn note(log: &Log, line: &str) {
    let mut lines = log.lines.clone();
    lines.push(line.to_string());
}

pub fn run() -> usize {
    let log = Log { lines: vec![] };
    note(&log, "a");
    log.lines.len()
}
