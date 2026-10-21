//! A job run in one of two modes, and a line that says how it went.

#[derive(Clone, Copy)]
pub enum Mode {
    Serial,
    Parallel,
}

/// What a mode means for a run.
pub struct Plan {
    pub batch: usize,
    pub workers: usize,
    pub retries: usize,
    pub timeout: usize,
}

impl Mode {
    pub fn plan(self) -> Plan {
        match self {
            Mode::Serial => Plan { batch: 1, workers: 1, retries: 1, timeout: 600 },
            Mode::Parallel => Plan { batch: 64, workers: 8, retries: 3, timeout: 60 },
        }
    }
}

#[derive(Clone, Copy)]
pub enum Level {
    Quiet,
    Loud,
}

impl Level {
    pub fn prefix(self) -> &'static str {
        match self {
            Level::Quiet => "",
            Level::Loud => "!",
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Level::Quiet => ".",
            Level::Loud => "!",
        }
    }
}

pub struct Settings {
    pub mode: Mode,
    pub level: Level,
}

pub struct Job {
    pub name: String,
    pub steps: Vec<String>,
}

pub struct Message {
    pub text: String,
}

pub struct Runner {
    plan: Plan,
}

impl Runner {
    pub fn new(mode: Mode) -> Runner {
        Runner { plan: mode.plan() }
    }

    pub fn batch(&self) -> usize {
        self.plan.batch
    }

    pub fn workers(&self) -> usize {
        self.plan.workers
    }
}

fn load(name: &str) -> Job {
    Job { name: name.to_string(), steps: vec![name.to_string()] }
}

fn infer(name: &str) -> Job {
    Job { name: name.to_string(), steps: vec![] }
}

/// Where the job comes from: the cache, or what its name says.
fn source(name: &str, cached: bool) -> Job {
    if cached {
        load(name)
    } else {
        infer(name)
    }
}

fn finished(job: &Job, mode: Mode) -> bool {
    job.steps.len() * Runner::new(mode).batch() <= mode.plan().timeout
}

fn send(message: &Message) -> usize {
    println!("{}", message.text);
    message.text.len()
}

fn report(job: &Job, level: Level, done: bool) -> usize {
    let message = if done {
        Message { text: format!("{}{} done", level.prefix(), job.name) }
    } else {
        Message { text: format!("{} failed{}", job.name, level.suffix()) }
    };
    send(&message)
}

pub fn run(name: &str, settings: &Settings, cached: bool) -> usize {
    let job = source(name, cached);
    report(&job, settings.level, finished(&job, settings.mode)) + settings.mode.plan().retries
}
