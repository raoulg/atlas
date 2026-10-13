//! Exchange rates, as the rates API serves them.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;

/// The rates the conversions of this run use, by currency.
static LATEST: Mutex<Option<HashMap<String, f64>>> = Mutex::new(None);

/// Today's rates against `base`, from the rates API the settings name.
pub fn fetch(config: &HashMap<String, String>, base: &str) -> Result<Value, reqwest::Error> {
    let url = format!("{}/latest?base={}", config["rates_url"], base);
    reqwest::blocking::get(url)?.json()
}

/// Keeps the rates for the conversions that follow.
pub fn remember(rates: HashMap<String, f64>) {
    *LATEST.lock().unwrap() = Some(rates);
}

/// What one unit of `currency` is worth in the base currency.
pub fn to_base(currency: &str) -> f64 {
    LATEST.lock().unwrap().as_ref().and_then(|rates| rates.get(currency).copied()).map_or(1.0, |rate| 1.0 / rate)
}
