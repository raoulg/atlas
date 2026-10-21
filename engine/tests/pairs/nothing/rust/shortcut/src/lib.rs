//! The same job runner, with what its steps leave out of sight of a reader.

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

pub struct Job {
    on_done: Box<dyn Fn()>,
}

impl Job {
    fn finish(&self) {
        (self.on_done)();
    }
}

pub async fn settle() {}

fn stop() {
    panic!("finished");
}

pub async fn run() -> u32 {
    run_task(&Echo);
    Job { on_done: Box::new(|| println!("done")) }.finish();
    settle().await;
    stop();
    0
}
