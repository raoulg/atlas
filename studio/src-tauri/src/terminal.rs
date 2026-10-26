//! Terminals: the user's own shell on a pseudo-terminal the Rust side owns, started in the open
//! workspace. What the shell writes goes to the window over a channel; what the user types, and the
//! terminal's size, come back by command. Closing a terminal, or the app, ends its shell. When the
//! app ends without closing them (a signal, a crash), the system closes its side of each terminal,
//! and the shell on the other side ends: on Unix it is hung up (SIGHUP), on Windows the pseudo
//! console ends the programs attached to it.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};

/// What a terminal tells the window: text the shell wrote, or that the shell ended, with its code.
#[derive(Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Event {
    Data { text: String },
    Exit { code: Option<u32> },
}

struct Terminal {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

/// The open terminals, by the number the window knows each by.
#[derive(Default)]
pub struct Terminals {
    open: Arc<Mutex<HashMap<u32, Terminal>>>,
    next: AtomicU32,
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize { rows: rows.max(2), cols: cols.max(2), pixel_width: 0, pixel_height: 0 }
}

impl Terminals {
    /// Start the user's shell in `workspace`, `cols` by `rows`; what it writes goes to `send`, which
    /// says whether the window still listens.
    pub fn open(
        &self,
        workspace: &Path,
        cols: u16,
        rows: u16,
        send: impl Fn(Event) -> bool + Send + Sync + 'static,
    ) -> Result<u32, String> {
        let pair = native_pty_system().openpty(size(cols, rows)).map_err(|error| error.to_string())?;
        // the user's login shell, as a terminal application starts it
        let mut command = CommandBuilder::new_default_prog();
        command.cwd(workspace);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env("TERM_PROGRAM", "Atlas Studio");
        let mut child = pair.slave.spawn_command(command).map_err(|error| format!("The shell could not start: {error}"))?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|error| error.to_string())?;
        let writer = pair.master.take_writer().map_err(|error| error.to_string())?;
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let killer = child.clone_killer();
        self.open.lock().map_err(|error| error.to_string())?.insert(id, Terminal { master: pair.master, writer, killer });
        let send = Arc::new(send);
        let data = Arc::clone(&send);
        let reading = std::thread::spawn(move || {
            let (mut buffer, mut pending) = ([0u8; 16384], Vec::new());
            while let Ok(n) = reader.read(&mut buffer) {
                if n == 0 {
                    break;
                }
                pending.extend_from_slice(&buffer[..n]);
                let text = complete_text(&mut pending);
                if !text.is_empty() && !data(Event::Data { text }) {
                    break;
                }
            }
        });
        // once the shell ends, the terminal is closed, which ends the reading on every platform (a
        // Windows console keeps its output open until it is closed); the end is said after the last
        // of the output
        let open = Arc::clone(&self.open);
        std::thread::spawn(move || {
            let code = child.wait().ok().map(|status| status.exit_code());
            open.lock().ok().and_then(|mut open| open.remove(&id));
            let _ = reading.join();
            send(Event::Exit { code });
        });
        Ok(id)
    }

    pub fn write(&self, id: u32, data: &str) -> Result<(), String> {
        let mut open = self.open.lock().map_err(|error| error.to_string())?;
        let terminal = open.get_mut(&id).ok_or("This terminal has ended")?;
        terminal.writer.write_all(data.as_bytes()).and_then(|_| terminal.writer.flush()).map_err(|error| error.to_string())
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        let open = self.open.lock().map_err(|error| error.to_string())?;
        let terminal = open.get(&id).ok_or("This terminal has ended")?;
        terminal.master.resize(size(cols, rows)).map_err(|error| error.to_string())
    }

    /// End the terminal's shell, and take its pseudo-terminal away, which ends the shell where the
    /// kill does not (a Windows console ends the programs attached to it when it closes, a Unix shell
    /// hangs up); the terminal then says it has ended. A shell that ended by itself meanwhile cannot
    /// be killed, and is ended all the same.
    pub fn close(&self, id: u32) -> Result<(), String> {
        let closed = self.open.lock().map_err(|error| error.to_string())?.remove(&id);
        if let Some(mut terminal) = closed {
            let _ = terminal.killer.kill();
        }
        Ok(())
    }

    /// End every shell, as the app closes.
    pub fn close_all(&self) {
        if let Ok(mut open) = self.open.lock() {
            for terminal in open.values_mut() {
                let _ = terminal.killer.kill();
            }
        }
    }
}

/// The text in `pending` up to its last whole character; a character the read split stays in
/// `pending` for the next read, and bytes that are no UTF-8 are shown as U+FFFD.
fn complete_text(pending: &mut Vec<u8>) -> String {
    let whole = match std::str::from_utf8(pending) {
        Ok(_) => pending.len(),
        Err(error) if error.error_len().is_none() => error.valid_up_to(),
        Err(_) => pending.len(),
    };
    let text = String::from_utf8_lossy(&pending[..whole]).into_owned();
    pending.drain(..whole);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_character_split_between_reads_is_kept_whole() {
        let euro = "€".as_bytes();
        let mut pending = vec![b'a', euro[0], euro[1]];
        assert_eq!(complete_text(&mut pending), "a");
        pending.push(euro[2]);
        assert_eq!(complete_text(&mut pending), "€");
        assert!(pending.is_empty());
    }

    /// The events of a terminal until its shell ends, or until none came for ten seconds.
    fn until_exit(events: &std::sync::mpsc::Receiver<Event>) -> (String, Option<Option<u32>>) {
        let mut text = String::new();
        while let Ok(event) = events.recv_timeout(std::time::Duration::from_secs(10)) {
            match event {
                Event::Data { text: more } => text.push_str(&more),
                Event::Exit { code } => return (text, Some(code)),
            }
        }
        (text, None)
    }

    fn terminal(terminals: &Terminals) -> (u32, std::sync::mpsc::Receiver<Event>) {
        let (sender, events) = std::sync::mpsc::channel();
        let id = terminals.open(&std::env::temp_dir(), 80, 24, move |event| sender.send(event).is_ok()).unwrap();
        (id, events)
    }

    #[cfg(unix)]
    #[test]
    fn the_shell_runs_what_is_typed_and_says_how_it_ended() {
        let terminals = Terminals::default();
        let (id, events) = terminal(&terminals);
        terminals.resize(id, 100, 30).unwrap();
        terminals.write(id, "echo atlas-$((40+2))\r").unwrap();
        terminals.write(id, "exit 3\r").unwrap();
        let (text, code) = until_exit(&events);
        assert!(text.contains("atlas-42"), "the shell ran the command: {text}");
        assert_eq!(code, Some(Some(3)));
        assert!(terminals.write(id, "echo\r").is_err(), "an ended terminal takes no input");
    }

    #[test]
    fn closing_a_terminal_ends_its_shell() {
        let terminals = Terminals::default();
        let (id, events) = terminal(&terminals);
        terminals.close(id).unwrap();
        assert!(until_exit(&events).1.is_some(), "the shell ended");
        assert!(terminals.open.lock().unwrap().is_empty());
    }

    #[test]
    fn bytes_that_are_no_text_do_not_stop_the_terminal() {
        let mut pending = vec![b'a', 0xff, b'b'];
        assert_eq!(complete_text(&mut pending), "a\u{fffd}b");
    }
}
