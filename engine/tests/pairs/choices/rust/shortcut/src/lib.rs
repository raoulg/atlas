//! A job run in one of two modes, and a line that says how it went.

#[derive(Clone, Copy, PartialEq)]
pub enum Level {
    Quiet,
    Loud,
}

pub struct Settings {
    pub mode: String,
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
    mode: String,
}

impl Runner {
    pub fn new(mode: &str) -> Runner {
        Runner { mode: mode.to_string() }
    }

    pub fn batch(&self) -> usize {
        if self.mode == "parallel" {
            64
        } else {
            1
        }
    }

    pub fn workers(&self) -> usize {
        if self.mode == "parallel" {
            8
        } else {
            1
        }
    }
}

fn retries(mode: &str) -> usize {
    if mode == "parallel" {
        3
    } else {
        1
    }
}

fn timeout(mode: &str) -> usize {
    if mode == "parallel" {
        60
    } else {
        600
    }
}

fn verbosity(settings: &Settings) -> usize {
    let m = &settings.mode;
    let shown = if m == "parallel" { 3 } else { 1 };
    shown + if m == "parallel" { 1 } else { 0 }
}

fn prefix(level: Level) -> &'static str {
    if level == Level::Loud {
        "!"
    } else {
        ""
    }
}

fn suffix(level: Level) -> &'static str {
    match level {
        Level::Loud => "!",
        Level::Quiet => ".",
    }
}

fn load(name: &str) -> Job {
    Job { name: name.to_string(), steps: vec![name.to_string()] }
}

fn infer(name: &str) -> Job {
    Job { name: name.to_string(), steps: vec![] }
}

fn finished(job: &Job, mode: &str) -> bool {
    job.steps.len() * Runner::new(mode).batch() <= timeout(mode)
}

fn send(message: &Message) -> usize {
    println!("{}", message.text);
    message.text.len()
}

pub fn run(name: &str, settings: &Settings, cached: bool) -> usize {
    let job = if cached { load(name) } else { infer(name) };
    let done = finished(&job, &settings.mode);
    let text = format!("{}{}{}{}", prefix(settings.level), job.name, if done { " done" } else { " failed" }, suffix(settings.level));
    send(&Message { text }) + retries(&settings.mode) + verbosity(settings)
}
