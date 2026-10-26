//! Each step of a job leaves something a reader sees: a place, a value it is handed, the end of the run.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

static RUNS: AtomicU32 = AtomicU32::new(0);

pub trait Task {
    fn run(&self);
}

pub struct Echo;

impl Task for Echo {
    fn run(&self) {
        println!("echo");
    }
}

fn run_task(task: &dyn Task) {
    task.run();
}

fn finish(on_done: impl Fn()) {
    on_done();
}

fn count() {
    RUNS.fetch_add(1, Ordering::SeqCst);
}

pub async fn settle(log: &Path) {
    let _ = std::fs::write(log, "settled\n");
}

fn stop() {
    std::process::exit(1);
}

pub async fn run(log: &Path) -> u32 {
    run_task(&Echo);
    finish(|| println!("done"));
    count();
    settle(log).await;
    stop();
    RUNS.load(Ordering::SeqCst)
}
