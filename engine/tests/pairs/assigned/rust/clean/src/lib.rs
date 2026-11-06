//! A level that is kept under a name and drawn, once with the name bound to it, once with the names
//! of a tuple bound to the elements of a tuple.

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

/// The level, bound once to the name that draws it.
pub fn alone(level: &Level) -> f64 {
    let kept = Some(level);
    draw(kept)
}

/// The level, bound by a tuple pattern to the element at its place.
pub fn paired(level: &Level) -> f64 {
    let (kept, _count) = (Some(level), 0);
    draw(kept)
}

/// The two names a function is given, as parts of one parameter.
pub fn set_holds(n: &mut Node, given: (Vec<Level>, Option<Vec<String>>)) {
    let (params, returns) = given;
    if let Some(r) = returns {
        n.returns = r;
    }
    n.holds.extend(params);
}

/// Names gathered into a list the body changes in place, each bound by its own `let`.
pub fn gather(names: &[String]) -> usize {
    let mut asked = vec![];
    let mut unknown = false;
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
