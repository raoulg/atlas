//! Letters sorted onto a pile, the locals named like the module's functions.

pub struct Letter {
    pub to: String,
}

pub struct Pile {
    pub letters: Vec<Letter>,
    pub returned: Vec<Letter>,
    pub counts: Vec<usize>,
}

/// How many letters a pile holds.
fn size(p: &Pile) -> usize {
    p.letters.len()
}

/// A name as it is compared: trimmed, in lower case.
fn tidy(name: &str) -> String {
    name.trim().to_lowercase()
}

/// How many of the letters that came in are for `name`.
pub fn addressed(incoming: &[Letter], name: &str) -> usize {
    incoming.iter().filter(|l| tidy(&l.to) == tidy(name)).count()
}

/// How many letters the piles hold together.
pub fn total(piles: &[Pile]) -> usize {
    piles.iter().map(size).sum()
}

/// Puts the letters for `name` on the pile, sends the others back, and notes how many came.
pub fn sort(pile: &mut Pile, name: &str, incoming: Vec<Letter>) {
    let tidy = |s: &str| s.trim().to_lowercase();
    let (mut addressed, mut returned) = (vec![], vec![]);
    for l in incoming {
        if tidy(&l.to) == tidy(name) {
            addressed.push(l);
        } else {
            returned.push(l);
        }
    }
    let size = addressed.len();
    pile.letters.extend(addressed);
    pile.returned.extend(returned);
    pile.counts.push(size);
}
