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

fn draw(gap: f32) {
    println!("{gap}");
}

fn list(levels: &[&Level]) {
    println!("{}", levels.len());
}

/// The gap between the level and the quiet level and the two side by side, each named before it is shown.
pub fn named(first: &Level) {
    let gap = first.db - QUIET.db;
    draw(gap);
    let both = [first, &QUIET];
    list(&both);
}

/// The gap between the level and the quiet level and the two side by side, each written where it is shown.
pub fn written(first: &Level) {
    draw(first.db - QUIET.db);
    list(&[first, &QUIET]);
}

fn listed(first: Level) -> Vec<Level> {
    let both: Vec<Level> = vec![first, QUIET];
    both
}

/// The level, listed beside the quiet level and shown.
pub fn shown(first: Level) {
    show(&listed(first));
}

/// The two levels, gathered by pushing the second onto a vector of the first, and listed.
pub fn gathered(first: &Level, second: &Level) {
    let mut both = vec![first];
    both.push(second);
    list(&both);
}

fn add<'a>(levels: &mut Vec<&'a Level>, level: &'a Level) {
    levels.push(level);
}

/// The two levels, gathered by lending a vector of the first to a function that adds the second, and listed.
pub fn lent(first: &Level, second: &Level) {
    let mut both = vec![first];
    add(&mut both, second);
    list(&both);
}
