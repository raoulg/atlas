//! The levels of a signal, each looked up in the collection it is handed: by a subscript, or by the method that looks it
//! up. `pushed` and `pushed_looked_up` look up in a list they change in place first; `added` and `added_looked_up` change
//! the row they look up, and so the rows.
use std::collections::HashMap;

/// A level of a signal.
pub struct Level {
    pub db: f32,
}

fn draw(level: &Level) {
    println!("{}", level.db);
}

fn show(rows: &[Vec<Level>]) {
    println!("{}", rows.len());
}

/// The first level, by its place.
pub fn indexed(levels: &[Level]) -> Option<()> {
    let first = &levels[0];
    draw(first);
    Some(())
}

/// The first level, the slice's first.
pub fn looked_up(levels: &[Level]) -> Option<()> {
    let first = levels.first()?;
    draw(first);
    Some(())
}

/// The first level, got by its place.
pub fn got_at(levels: &[Level]) -> Option<()> {
    let first = levels.get(0)?;
    draw(first);
    Some(())
}

/// The first two levels, by their places.
pub fn paired(levels: &[Level]) -> Option<()> {
    let (first, second) = (&levels[0], &levels[1]);
    draw(first);
    draw(second);
    Some(())
}

/// The first two levels, got by their places.
pub fn paired_looked_up(levels: &[Level]) -> Option<()> {
    let (first, second) = (levels.first()?, levels.get(1)?);
    draw(first);
    draw(second);
    Some(())
}

/// The level of a name, by its key.
pub fn keyed(levels: &HashMap<String, Level>, name: &str) -> Option<()> {
    let level = &levels[name];
    draw(level);
    Some(())
}

/// The level of a name, got by its key.
pub fn got(levels: &HashMap<String, Level>, name: &str) -> Option<()> {
    let level = levels.get(name)?;
    draw(level);
    Some(())
}

/// The first of two levels gathered in a list, by its place.
pub fn pushed(first: &Level, second: &Level) -> Option<()> {
    let mut both = vec![first];
    both.push(second);
    draw(both[0]);
    Some(())
}

/// The first of two levels gathered in a list, the list's first.
pub fn pushed_looked_up(first: &Level, second: &Level) -> Option<()> {
    let mut both = vec![first];
    both.push(second);
    draw(both.first()?);
    Some(())
}

/// A level added to the first row, by its place.
pub fn added(rows: &mut [Vec<Level>], level: Level) -> Option<()> {
    let row = &mut rows[0];
    row.push(level);
    show(rows);
    Some(())
}

/// A level added to the first row, the slice's first.
pub fn added_looked_up(rows: &mut [Vec<Level>], level: Level) -> Option<()> {
    let row = rows.first_mut()?;
    row.push(level);
    show(rows);
    Some(())
}
