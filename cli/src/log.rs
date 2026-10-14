//! What `atlas serve` says to the person who started it: that it runs and where, then only what is
//! worth knowing, a line each, with the time and who did it: the page drawn, an export, a note
//! written, and what an agent did with it or where it pointed the page. In the logo's colours in a
//! terminal; plain elsewhere, or with `NO_COLOR`.

use std::io::{IsTerminal, Write};

use clap::builder::styling::{AnsiColor, Effects, Style};

/// Who did what a line says.
#[derive(Clone, Copy)]
pub enum Who {
    /// The reader of the page.
    You,
    /// An agent, through `atlas mcp`.
    Agent,
    /// The server itself.
    Atlas,
}

pub struct Log {
    color: bool,
}

impl Log {
    pub fn new() -> Log {
        Log { color: std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()) }
    }

    fn paint(&self, style: Style, s: &str) -> String {
        if self.color {
            format!("{style}{s}{style:#}")
        } else {
            s.to_string()
        }
    }

    /// The first lines: what is served, where, how an agent joins, how it stops.
    pub fn start(&self, project: &str, url: &str) {
        let (bold, dim, petrol) =
            (Style::new().effects(Effects::BOLD), Style::new().effects(Effects::DIMMED), AnsiColor::Cyan.on_default());
        self.write(&format!("{} {}", self.paint(bold, "atlas serve"), self.paint(dim, &format!("· {project}"))));
        self.write(&format!("  {}  {}", self.paint(dim, "page "), self.paint(petrol.effects(Effects::UNDERLINE), url)));
        self.write(&format!("  {}  atlas mcp: shown · look · notes · answer", self.paint(dim, "agent")));
        self.write(&format!("  {}  ctrl-c", self.paint(dim, "stop ")));
    }

    /// A line, with the time and who did it.
    pub fn say(&self, who: Who, text: &str) {
        let (name, style) = match who {
            Who::You => ("you", AnsiColor::Cyan.on_default()),
            Who::Agent => ("agent", AnsiColor::Red.on_default().effects(Effects::BOLD)),
            Who::Atlas => ("atlas", Style::new().effects(Effects::DIMMED)),
        };
        self.write(&format!(
            "{}  {}  {text}",
            self.paint(Style::new().effects(Effects::DIMMED), &now()),
            self.paint(style, &format!("{name:<5}"))
        ));
    }

    /// A closed terminal is no failure of the server's: the line is let go.
    fn write(&self, line: &str) {
        let mut terminal = std::io::stderr();
        let _ = writeln!(terminal, "{line}");
    }
}

/// The time of day, `16:58:03`, on this machine's clock.
#[cfg(unix)]
fn now() -> String {
    // SAFETY: `time` and `localtime_r` write only the `tm` handed to them
    let t = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

/// The time of day in UTC, where the local zone is not at hand.
#[cfg(not(unix))]
fn now() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    format!("{:02}:{:02}:{:02}", s / 3600 % 24, s / 60 % 60, s % 60)
}
