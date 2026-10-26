//! Two functions that do nothing; one says it may fail.

fn settle() -> Result<(), String> {
    Ok(())
}

fn settle_unit() {}

pub fn main_like() -> Result<(), String> {
    settle_unit();
    settle()
}
