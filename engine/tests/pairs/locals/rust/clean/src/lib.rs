//! Two levels of a signal, shown side by side: `pair` makes the array that holds them by its own code, `listed` the vector.

/// A level of a signal.
pub struct Level {
    pub db: f32,
}

fn show(levels: &[Level]) {
    println!("{}", levels.len());
}

fn pair(first: Level, second: Level) -> [Level; 2] {
    let both: [Level; 2] = [first, second];
    both
}

/// The two levels, shown side by side.
pub fn paired(first: Level, second: Level) {
    show(&pair(first, second));
}

fn listed(first: Level, second: Level) -> Vec<Level> {
    let both: Vec<Level> = vec![first, second];
    both
}

/// The two levels, listed and shown.
pub fn shown(first: Level, second: Level) {
    show(&listed(first, second));
}
