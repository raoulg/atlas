//! spend: keeps a ledger of expenses and reports them in one currency.

mod config;
mod error;
mod expense;
mod rates;
mod report;
mod store;

use std::path::{Path, PathBuf};
use std::process::Command as Program;

use clap::{Parser, Subcommand, ValueEnum};

use config::Config;
use error::Error;
use rates::RatesClient;
use report::{Csv, Markdown, Render};
use store::Store;

/// Where the program keeps its settings, its ledger and its reports, and where it asks for rates.
const CONFIG: &str = "spend.toml";
const LEDGER: &str = "spend.db";
const REPORTS: &str = "reports";
const RATES_API: &str = "https://api.frankfurter.app";
/// The program that shows a report.
const VIEWER: &str = "open";

#[derive(Parser)]
#[command(about = "Keeps a ledger of expenses and reports them in one currency")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Reads expenses from a CSV file into the ledger.
    Import { file: PathBuf },
    /// Fetches today's exchange rates into the ledger.
    Rates,
    /// Writes the ledger as a report in the base currency.
    Report {
        #[arg(value_enum, default_value_t = Format::Markdown)]
        format: Format,
        /// Opens the report once it is written.
        #[arg(long)]
        open: bool,
    },
}

/// The formats a report is written in.
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Markdown,
    Csv,
}

impl Format {
    fn renderer(self) -> Box<dyn Render> {
        match self {
            Format::Markdown => Box::new(Markdown),
            Format::Csv => Box::new(Csv),
        }
    }
}

fn main() -> Result<(), Error> {
    let cli = Cli::parse();
    let config = Config::load(Path::new(CONFIG))?;
    let store = Store::open(Path::new(LEDGER))?;
    match cli.command {
        Command::Import { file } => {
            let expenses = expense::read_csv(&file)?;
            store.save(&expenses)?;
            println!("imported {} expenses", expenses.len());
        }
        Command::Rates => {
            let client = RatesClient::new(RATES_API);
            let rates = client.latest(&config.base)?;
            store.save_rates(&rates)?;
            println!("rates of {} against {}", rates.date, rates.base);
        }
        Command::Report { format, open } => {
            let rates = store.rates(&config.base)?;
            let lines = report::lines(&store.expenses()?, &rates)?;
            let path = report::write(Path::new(REPORTS), format.renderer().as_ref(), &config.title, &lines)?;
            println!("wrote {}", path.display());
            if open {
                Program::new(VIEWER).arg(&path).status()?;
            }
        }
    }
    Ok(())
}
