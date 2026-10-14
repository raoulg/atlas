//! HTTP/1.1 for the pages Atlas serves on this machine (`atlas history`, `atlas serve`): one request
//! per connection, read whole, and one answer written back. A server answers only at the addresses it
//! serves, so a page of another site that renamed itself to this machine is refused, and a request
//! that changes something must carry `X-Atlas`, a header a page of another site cannot send without
//! asking first.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;

use anyhow::Result;

/// A request as a server reads it: what it asks, where, with what in its query and body, the address
/// the browser sent it to, and whether it came from a page of Atlas's own.
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    pub host: String,
    pub own: bool,
    pub body: Vec<u8>,
}

/// An answer: its status line, the type of its body, and the body.
pub struct Answer {
    pub status: &'static str,
    pub kind: &'static str,
    pub body: Vec<u8>,
}

/// The largest body a request may carry.
const MOST: usize = 4 << 20;

impl Answer {
    pub fn page(html: String) -> Answer {
        Answer { status: "200 OK", kind: "text/html; charset=utf-8", body: html.into_bytes() }
    }

    pub fn json(json: String) -> Answer {
        Answer { status: "200 OK", kind: "application/json", body: json.into_bytes() }
    }

    pub fn text(status: &'static str, text: String) -> Answer {
        Answer { status, kind: "text/plain; charset=utf-8", body: text.into_bytes() }
    }

    /// What went wrong, in words, where the page would have been.
    pub fn failed(e: anyhow::Error) -> Answer {
        Answer::text("500 Internal Server Error", format!("{e:#}"))
    }

    /// Nothing to say.
    pub fn none() -> Answer {
        Answer { status: "204 No Content", kind: "text/plain", body: vec![] }
    }
}

/// One request: its request line, its headers and the body its `Content-Length` names.
pub fn request(conn: &TcpStream) -> Result<Request> {
    let mut reader = BufReader::new(conn);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut words = line.split_whitespace();
    let (method, target) = (words.next().unwrap_or("").to_string(), words.next().unwrap_or("/").to_string());
    let (mut host, mut own, mut length) = (String::new(), false, 0);
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else { continue };
        match name.trim().to_ascii_lowercase().as_str() {
            "host" => host = value.trim().to_string(),
            "x-atlas" => own = true,
            "content-length" => length = value.trim().parse().unwrap_or(0),
            _ => {}
        }
    }
    let mut body = vec![0; length.min(MOST)];
    reader.read_exact(&mut body)?;
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let query = query
        .split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| kv.split_once('=').unwrap_or((kv, "")))
        .map(|(k, v)| (decoded(k), decoded(v)))
        .collect();
    Ok(Request { method, path: decoded(path), query, host, own, body })
}

/// A part of an address with its escapes (`%2F`, `+`) read back.
fn decoded(s: &str) -> String {
    let (bytes, mut out, mut i) = (s.as_bytes(), Vec::with_capacity(s.len()), 0);
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
        let (byte, used) = match (bytes[i], hex) {
            (b'%', Some(b)) => (b, 3),
            (b'+', _) => (b' ', 1),
            (b, _) => (b, 1),
        };
        out.push(byte);
        i += used;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Write an answer and close the connection.
pub fn reply(mut conn: TcpStream, a: Answer) -> Result<()> {
    write!(
        conn,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        a.status,
        a.kind,
        a.body.len()
    )?;
    conn.write_all(&a.body)?;
    Ok(conn.flush()?)
}
