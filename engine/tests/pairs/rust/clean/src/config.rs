//! The user's settings, read from a TOML file.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::error::Error;

/// What the user sets: the title of a report and the currency it is written in.
#[derive(Deserialize)]
pub struct Config {
    pub title: String,
    pub base: String,
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, Error> {
        let text = fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }
}
