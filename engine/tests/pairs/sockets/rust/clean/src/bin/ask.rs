//! A client of the greeter: sends a name and prints the greeting it reads back.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};

/// Where the greeter listens.
const ADDRESS: &str = "127.0.0.1:7878";

fn main() -> io::Result<()> {
    let mut conn = TcpStream::connect(ADDRESS)?;
    conn.write_all(b"ada")?;
    conn.shutdown(Shutdown::Write)?;
    println!("{}", hear(conn)?);
    Ok(())
}

/// The greeting the server writes back, read to its end.
fn hear(mut conn: TcpStream) -> io::Result<String> {
    let mut greeting = String::new();
    conn.read_to_string(&mut greeting)?;
    Ok(greeting)
}
