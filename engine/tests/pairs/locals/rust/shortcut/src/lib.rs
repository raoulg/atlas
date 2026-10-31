//! A level of a signal, shown beside the quiet level: `pair` makes the array that holds them by its own code, `listed`
//! the vector, from the level it is handed and the module's quiet level.

/// A level of a signal.
pub struct Level {
    pub db: f32,
}

/// The level of silence.
const QUIET: Level = Level { db: 0.0 };

fn show(levels: &[Level]) {
    println!("{}", levels.len());
}

fn pair(first: Level) -> [Level; 2] {
    let both: [Level; 2] = [first, QUIET];
    both
}

/// The level, shown beside the quiet level.
pub fn paired(first: Level) {
    show(&pair(first));
}

fn listed(first: Level) -> Vec<Level> {
    let both: Vec<Level> = vec![first, QUIET];
    both
}

/// The level, listed beside the quiet level and shown.
pub fn shown(first: Level) {
    show(&listed(first));
}
