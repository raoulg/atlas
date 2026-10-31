//! A server that greets each client by the name it sends, and lets the client read the greeting
//! before the connection closes.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

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
    linger();
}

/// The greeting for a name.
fn greeting(name: &[u8]) -> String {
    format!("hello, {}\n", String::from_utf8_lossy(name).trim())
}

/// Wait a moment, so that the client has read the greeting before the connection is dropped.
fn linger() {
    thread::sleep(Duration::from_secs(1));
}
