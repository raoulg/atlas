//! Exchange rates, as the rates API serves them.

use std::collections::HashMap;

use reqwest::blocking::Client;
use serde::Deserialize;

use crate::error::Error;

/// How many units of each currency one unit of the base buys, on a date.
#[derive(Deserialize)]
pub struct Rates {
    pub base: String,
    pub date: String,
    pub rates: HashMap<String, f64>,
}

impl Rates {
    /// What one unit of `currency` is worth in the base currency.
    pub fn to_base(&self, currency: &str) -> Option<f64> {
        if currency == self.base {
            Some(1.0)
        } else {
            self.rates.get(currency).map(|rate| 1.0 / rate)
        }
    }
}

/// A client of the rates API at one address.
pub struct RatesClient {
    http: Client,
    url: String,
}

impl RatesClient {
    pub fn new(url: &str) -> RatesClient {
        RatesClient { http: Client::new(), url: url.to_string() }
    }

    /// Today's rates against `base`.
    pub fn latest(&self, base: &str) -> Result<Rates, Error> {
        let url = format!("{}/latest", self.url);
        let response = self.http.get(url).query(&[("base", base)]).send()?.error_for_status()?;
        Ok(response.json()?)
    }
}
