//! Functions that give nothing back: one waits, one ends the process, one panics.

use std::time::Duration;

fn wait() {
    std::thread::sleep(Duration::from_millis(1));
}

fn stop() {
    std::process::exit(1);
}

fn halt() {
    panic!("stop");
}

pub fn main_like() {
    wait();
    halt();
    stop();
}
