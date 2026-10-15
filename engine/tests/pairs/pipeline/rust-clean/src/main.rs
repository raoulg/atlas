//! The command line: a run names the source and the files, and hands them down.

use std::num::NonZeroUsize;
use std::path::PathBuf;

use clap::Parser;

use sunspots::{download, load, smooth, Error, Plot, Settings, Trend};

const SILSO: &str = "https://www.sidc.be/SILSO/DATA/SN_m_tot_V2.0.csv";
const RAW: &str = "data/raw/sunspots.csv";
const FIGURE: &str = "figures/sunspots.png";

/// Downloads SILSO's monthly sunspot numbers where none are kept, smooths them and saves their trend.
#[derive(Parser)]
struct Cli {
    /// Where SILSO's file is kept.
    #[arg(long, default_value = RAW)]
    raw: PathBuf,
    /// Where the figure is saved, as a PNG.
    #[arg(long, default_value = FIGURE)]
    figure: PathBuf,
    /// How many months the smoothed mean is taken over.
    #[arg(long, default_value_t = Settings::default().months)]
    months: NonZeroUsize,
}

fn main() -> Result<(), Error> {
    let cli = Cli::parse();
    let settings = Settings { months: cli.months, ..Settings::default() };
    if !cli.raw.exists() {
        download(SILSO, &cli.raw)?;
    }
    let series = load(&cli.raw)?;
    let smoothed = smooth(&series, settings.months);
    Trend::new(settings).save(&series, &smoothed, &cli.figure)?;
    Ok(())
}
