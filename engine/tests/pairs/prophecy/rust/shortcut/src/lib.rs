//! Where finished reports are kept.

/// Writes reports and gives nothing back.
pub struct Sink;

impl Sink {
    pub fn new() -> Sink {
        Sink
    }

    pub fn write(&self, report: &str) {
        println!("{report}");
    }
}

/// What goes in comes out.
#[doc = "prophecy: supersedes=archive::Sink"]
pub struct Store;

impl Store {
    pub fn new() -> Store {
        todo!()
    }

    pub fn write(&self, _report: &str) {
        todo!()
    }

    pub fn read(&self) -> String {
        todo!()
    }
}

pub fn run(report: &str) {
    Store::new().write(report);
}

