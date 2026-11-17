//! A level that is kept under a name and drawn, once with the name assigned after a `let` of
//! nothing, once with the names of a tuple bound to the elements of a tuple.

pub struct Level {
    pub db: f64,
}

pub struct Node {
    pub holds: Vec<Level>,
    pub returns: Vec<String>,
}

fn draw(level: Option<&Level>) -> f64 {
    level.map_or(0.0, |l| l.db)
}

/// The level, kept in a name that was nothing first.
pub fn alone(level: &Level) -> f64 {
    let mut kept = None;
    kept = Some(level);
    draw(kept)
}

/// The same, in a tuple pattern.
pub fn paired(level: &Level) -> f64 {
    let (mut kept, mut _count) = (None, 0);
    kept = Some(level);
    draw(kept)
}

/// The two names a function is given, one parameter written as a tuple pattern.
pub fn set_holds(n: &mut Node, (params, returns): (Vec<Level>, Option<Vec<String>>)) {
    if let Some(r) = returns {
        n.returns = r;
    }
    n.holds.extend(params);
}

/// Names gathered into a list the body changes in place, bound by one tuple pattern.
pub fn gather(names: &[String]) -> usize {
    let (mut asked, mut unknown) = (vec![], false);
    for s in names {
        asked.push(s.clone());
    }
    if asked.is_empty() {
        unknown = true;
    }
    let first = asked.first();
    first.map_or(unknown as usize, |f| f.len())
}

/// The caller that fills the parameter.
pub fn run(n: &mut Node, levels: Vec<Level>) {
    set_holds(n, (levels, None));
}
