//! The levels of a signal, each looked up in a collection the module keeps: by a subscript, or by the method that looks
//! it up. `pushed` and `pushed_looked_up` look up in a list they change in place first, which starts at the module's quiet
//! level; `added` and `added_looked_up` change the row they look up, and so the module's rows.
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// A level of a signal.
pub struct Level {
    pub db: f32,
}

static QUIET: Level = Level { db: 0.0 };
static LEVELS: [Level; 2] = [Level { db: -3.0 }, Level { db: -12.0 }];
static BY_NAME: LazyLock<HashMap<String, Level>> =
    LazyLock::new(|| HashMap::from([("loud".to_string(), Level { db: -3.0 }), ("soft".to_string(), Level { db: -12.0 })]));
static ROWS: Mutex<Vec<Vec<Level>>> = Mutex::new(Vec::new());

fn draw(level: &Level) {
    println!("{}", level.db);
}

fn show(rows: &[Vec<Level>]) {
    println!("{}", rows.len());
}

/// The first level, by its place.
pub fn indexed() -> Option<()> {
    let first = &LEVELS[0];
    draw(first);
    Some(())
}

/// The first level, the slice's first.
pub fn looked_up() -> Option<()> {
    let first = LEVELS.first()?;
    draw(first);
    Some(())
}

/// The first level, got by its place.
pub fn got_at() -> Option<()> {
    let first = LEVELS.get(0)?;
    draw(first);
    Some(())
}

/// The first two levels, by their places.
pub fn paired() -> Option<()> {
    let (first, second) = (&LEVELS[0], &LEVELS[1]);
    draw(first);
    draw(second);
    Some(())
}

/// The first two levels, got by their places.
pub fn paired_looked_up() -> Option<()> {
    let (first, second) = (LEVELS.first()?, LEVELS.get(1)?);
    draw(first);
    draw(second);
    Some(())
}

/// The level of a name, by its key.
pub fn keyed(name: &str) -> Option<()> {
    let level = &BY_NAME[name];
    draw(level);
    Some(())
}

/// The level of a name, got by its key.
pub fn got(name: &str) -> Option<()> {
    let level = BY_NAME.get(name)?;
    draw(level);
    Some(())
}

/// The quiet level and a second gathered in a list, the first by its place.
pub fn pushed(second: &Level) -> Option<()> {
    let mut both = vec![&QUIET];
    both.push(second);
    draw(both[0]);
    Some(())
}

/// The quiet level and a second gathered in a list, the list's first.
pub fn pushed_looked_up(second: &Level) -> Option<()> {
    let mut both = vec![&QUIET];
    both.push(second);
    draw(both.first()?);
    Some(())
}

/// A level added to the module's first row, by its place.
pub fn added(level: Level) -> Option<()> {
    let mut rows = ROWS.lock().ok()?;
    let row = &mut rows[0];
    row.push(level);
    show(&rows);
    Some(())
}

/// A level added to the module's first row, the rows' first.
pub fn added_looked_up(level: Level) -> Option<()> {
    let mut rows = ROWS.lock().ok()?;
    let row = rows.first_mut()?;
    row.push(level);
    show(&rows);
    Some(())
}
