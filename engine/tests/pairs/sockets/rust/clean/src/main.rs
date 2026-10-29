//! A server that greets each client by the name it sends, and lets the client read the greeting
//! before the connection closes.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::time::{Duration, Instant};

/// Where the server listens.
const ADDRESS: &str = "127.0.0.1:7878";

fn main() -> io::Result<()> {
    let listener = TcpListener::bind(ADDRESS)?;
    for conn in listener.incoming() {
        answer(conn?);
    }
    Ok(())
}

/// Read the client's name and write its greeting back, then let the client read it.
fn answer(mut conn: TcpStream) {
    let mut name = [0; 256];
    let n = conn.read(&mut name).unwrap_or(0);
    let _ = conn.write_all(greeting(&name[..n]).as_bytes());
    linger(&conn);
}

/// The greeting for a name.
fn greeting(name: &[u8]) -> String {
    format!("hello, {}\n", String::from_utf8_lossy(name).trim())
}

/// Close the writing half, so that the client reads the end of the greeting, and read what it
/// still sends for a moment before the connection is let go.
fn linger(conn: &TcpStream) {
    let _ = conn.shutdown(Shutdown::Write);
    let until = Instant::now() + Duration::from_secs(1);
    let mut sink = [0; 1024];
    let _ = conn.set_read_timeout(Some(Duration::from_millis(100)));
    while Instant::now() < until {
        match (&*conn).read(&mut sink) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
}
