//! The levels of a signal, gathered into a vector the body makes by its own code and changed after it binds it, in one arm
//! of a choice or in a loop: the step after the change lists what the choice or the loop hands on.

/// A level of a signal.
pub struct Level {
    pub db: f32,
}

fn list(levels: &[&Level]) {
    println!("{}", levels.len());
}

/// The level, listed where it is loud: an empty vector, pushed onto in one arm.
pub fn picked(level: &Level, loud: bool) {
    let mut both: Vec<&Level> = vec![];
    if loud {
        both.push(level);
    }
    list(&both);
}

/// The first level, and the second beside it where it is loud: a vector of the first, pushed onto in one arm.
pub fn gathered(first: &Level, second: &Level, loud: bool) {
    let mut both = vec![first];
    if loud {
        both.push(second);
    }
    list(&both);
}

/// Every level, listed: an empty vector, pushed onto in a loop.
pub fn looped(levels: &[Level]) {
    let mut every: Vec<&Level> = vec![];
    for level in levels {
        every.push(level);
    }
    list(&every);
}
