//! The same counts kept in a `static` the methods raise: the change lands in a place of its own,
//! never in the struct that raises it.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

static PLAIN: AtomicUsize = AtomicUsize::new(0);
static ATOMIC: AtomicUsize = AtomicUsize::new(0);
static LOCKED: Mutex<Vec<u8>> = Mutex::new(Vec::new());

pub struct Atomic;

impl Atomic {
    pub fn bump(&self) {
        ATOMIC.fetch_add(1, Ordering::SeqCst);
    }
}

pub struct Locked;

impl Locked {
    pub fn bump(&self) {
        LOCKED.lock().unwrap().push(1);
    }
}

pub struct Celled;

impl Celled {
    pub fn bump(&self) {
        PLAIN.store(1, Ordering::SeqCst);
    }
}

pub fn run(a: &Atomic, l: &Locked, c: &Celled) {
    a.bump();
    l.bump();
    c.bump();
}
