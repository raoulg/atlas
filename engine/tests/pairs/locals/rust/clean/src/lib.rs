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

fn draw(gap: f32) {
    println!("{gap}");
}

fn list(levels: &[&Level]) {
    println!("{}", levels.len());
}

/// The gap between the two levels and the two levels side by side, each named before it is shown.
pub fn named(first: &Level, second: &Level) {
    let gap = first.db - second.db;
    draw(gap);
    let both = [first, second];
    list(&both);
}

/// The gap between the two levels and the two levels side by side, each written where it is shown.
pub fn written(first: &Level, second: &Level) {
    draw(first.db - second.db);
    list(&[first, second]);
}

fn listed(first: Level, second: Level) -> Vec<Level> {
    let both: Vec<Level> = vec![first, second];
    both
}

/// The two levels, listed and shown.
pub fn shown(first: Level, second: Level) {
    show(&listed(first, second));
}

/// The two levels, gathered in a vector written out once and listed.
pub fn gathered(first: &Level, second: &Level) {
    let both = vec![first, second];
    list(&both);
}

/// The first level, and the second beside it where it is loud: the vector the choice gives, written out in each arm, and
/// listed.
pub fn chosen(first: &Level, second: &Level, loud: bool) {
    let both = if loud { vec![first, second] } else { vec![first] };
    list(&both);
}
