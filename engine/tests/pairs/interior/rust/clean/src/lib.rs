//! Counters that change through a shared reference: the count lives in a field of the struct, in a
//! type that changes in place. The same count kept by `&mut self` is the reference.

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// A count raised by `&mut self`.
pub struct Plain {
    n: usize,
}

impl Plain {
    pub fn bump(&mut self) {
        self.n += 1;
    }
}

/// A count in an atomic.
pub struct Atomic {
    n: AtomicUsize,
}

impl Atomic {
    pub fn bump(&self) {
        self.n.fetch_add(1, Ordering::SeqCst);
    }
}

/// A count in an atomic that its clones share.
pub struct Shared {
    n: Arc<AtomicUsize>,
}

impl Shared {
    pub fn bump(&self) {
        self.n.fetch_add(1, Ordering::SeqCst);
    }
}

/// Bytes in a lock that its clones share.
pub struct Locked {
    n: Arc<Mutex<Vec<u8>>>,
}

impl Locked {
    pub fn bump(&self) {
        self.n.lock().unwrap().push(1);
    }
}

/// A count in a cell.
pub struct Celled {
    n: Cell<usize>,
}

impl Celled {
    pub fn bump(&self) {
        self.n.set(1);
    }
}

/// A count a method only reads.
pub struct Reader {
    n: AtomicUsize,
}

impl Reader {
    pub fn seen(&self) -> usize {
        self.n.load(Ordering::SeqCst)
    }
}

pub fn run(p: &mut Plain, a: &Atomic, s: &Shared, l: &Locked, c: &Celled) {
    p.bump();
    a.bump();
    s.bump();
    l.bump();
    c.bump();
}
