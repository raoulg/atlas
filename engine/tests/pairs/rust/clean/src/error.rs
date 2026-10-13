//! What can go wrong, by where it goes wrong.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading a file: {0}")]
    Io(#[from] std::io::Error),
    #[error("the settings: {0}")]
    Config(#[from] toml::de::Error),
    #[error("the ledger: {0}")]
    Ledger(#[from] rusqlite::Error),
    #[error("the rates API: {0}")]
    Rates(#[from] reqwest::Error),
    #[error("not an expense: {0}")]
    Line(String),
    #[error("not an amount: {0}")]
    Amount(String),
    #[error("no rates against {0}; run `spend rates` first")]
    NoRates(String),
    #[error("no rate for {0}")]
    NoRate(String),
}
