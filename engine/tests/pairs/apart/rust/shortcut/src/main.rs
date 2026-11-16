//! The program sets its ledger up on the folder it keeps its data in, and flushes it as the latest.

use std::path::PathBuf;

use ledger::Ledger;

struct Pipeline {
    ledger: Ledger,
}

impl Pipeline {
    fn new() -> Pipeline {
        Pipeline { ledger: Ledger::new(PathBuf::from("data/blocked")) }
    }

    fn run(&self) {
        self.ledger.flush("latest");
    }
}

fn main() {
    Pipeline::new().run();
}
