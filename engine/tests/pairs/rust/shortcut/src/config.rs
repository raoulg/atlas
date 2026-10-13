//! The user's settings, read from a TOML file.

use std::collections::HashMap;
use std::env;
use std::fs;

/// The settings by name: `title`, `base` and `rates_url`.
pub fn load() -> HashMap<String, String> {
    let path = env::var("SPEND_CONFIG").unwrap_or_else(|_| "spend.toml".to_string());
    let text = fs::read_to_string(&path).expect("cannot read the settings");
    toml::from_str(&text).expect("the settings are not `key = \"value\"` lines")
}
