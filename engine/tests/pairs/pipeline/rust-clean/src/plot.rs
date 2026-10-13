//! Figures of a sunspot series: each kind of plot draws on a drawing area, and every plot saves
//! its figure as a PNG.

use std::fs;
use std::path::{Path, PathBuf};

use plotters::coord::Shift;
use plotters::prelude::*;

use crate::error::Error;
use crate::series::{Month, Smoothed};
use crate::settings::Settings;

const MONTHLY: RGBColor = RGBColor(179, 179, 179);
const SMOOTHED: RGBColor = RGBColor(31, 119, 180);

/// A figure drawn from a series and its smoothed means, and saved as an image.
pub trait Plot {
    /// The choices the figure is drawn with.
    fn settings(&self) -> &Settings;

    /// Draws `series` and `smoothed` on `area`, whatever backend it draws on.
    fn draw<DB: DrawingBackend>(&self, area: &DrawingArea<DB, Shift>, series: &[Month], smoothed: &[Smoothed]) -> Result<(), Error>;

    /// Draws `series` and `smoothed` and writes the figure to `to` as a PNG, creating the
    /// directory it goes in; gives `to`.
    fn save(&self, series: &[Month], smoothed: &[Smoothed], to: &Path) -> Result<PathBuf, Error> {
        if let Some(dir) = to.parent() {
            fs::create_dir_all(dir)?;
        }
        let area = BitMapBackend::new(to, self.settings().size).into_drawing_area();
        self.draw(&area, series, smoothed)?;
        area.present()?;
        Ok(to.to_path_buf())
    }
}

/// The monthly means as a thin grey line, with their smoothed mean over them.
pub struct Trend {
    settings: Settings,
}

impl Trend {
    pub fn new(settings: Settings) -> Trend {
        Trend { settings }
    }
}

impl Plot for Trend {
    fn settings(&self) -> &Settings {
        &self.settings
    }

    fn draw<DB: DrawingBackend>(&self, area: &DrawingArea<DB, Shift>, series: &[Month], smoothed: &[Smoothed]) -> Result<(), Error> {
        let (Some(first), Some(last)) = (series.first(), series.last()) else {
            return Err(Error::Empty);
        };
        let top = series.iter().map(|month| month.mean).fold(0.0, f64::max);
        area.fill(&WHITE)?;
        let mut chart = ChartBuilder::on(area)
            .caption(&self.settings.title, ("sans-serif", 20))
            .margin(10)
            .x_label_area_size(30)
            .y_label_area_size(50)
            .build_cartesian_2d(first.date..last.date, 0.0..top)?;
        chart.configure_mesh().disable_x_mesh().y_desc("sunspot number").draw()?;
        chart
            .draw_series(LineSeries::new(series.iter().map(|month| (month.date, month.mean)), &MONTHLY))?
            .label("monthly mean")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &MONTHLY));
        chart
            .draw_series(LineSeries::new(smoothed.iter().map(|month| (month.date, month.mean)), &SMOOTHED))?
            .label(format!("{}-month mean", self.settings.months))
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &SMOOTHED));
        chart.configure_series_labels().background_style(&WHITE).border_style(&BLACK).draw()?;
        Ok(())
    }
}
