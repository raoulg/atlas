//! HTTP/1.1 for the pages Atlas serves on this machine (`atlas-code history`, `atlas-code serve`): one request
//! per connection, read whole, and one answer written back. A server answers only at the addresses it
//! serves, so a page of another site that renamed itself to this machine is refused, and a request
//! that changes something or runs an analysis must come from Atlas's own page (`Request::own`).
//!
//! What a request may cost is bounded: it is read within `WAIT`, its head in at most `HEAD` bytes and
//! `HEADERS` headers, its body in at most `MOST` bytes; one that is not is answered with why (400, 408,
//! 413, 414, 431) and closed. At most `CONNECTIONS` are answered at a time; one more is told the server
//! is busy (503).

use std::any::Any;
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;

/// A request as a server reads it: what it asks, where, with what in its query and body, the address
/// the browser sent it to, and whether it came from a page of Atlas's own.
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    pub host: String,
    /// Whether it came from the page this server served, or from the reader at the browser's address
    /// bar: it carries `X-Atlas`, which a page of another site cannot send without asking first; or the
    /// browser says so (`Sec-Fetch-Site: same-origin` or `none`), which no page can say for it; or, from
    /// a browser that does not say where a request comes from, its `Origin` or `Referer` is this
    /// address. A link that is followed carries no header of the page's, so the browser's word stands
    /// for it.
    pub own: bool,
    pub body: Vec<u8>,
}

/// An answer: its status line, the type of its body, the body, and the name of the file it is when
/// the browser is to keep it.
pub struct Answer {
    pub status: &'static str,
    pub kind: &'static str,
    pub body: Vec<u8>,
    pub file: Option<String>,
}

/// The largest body a request may carry.
const MOST: usize = 4 << 20;

/// The most a request's head (its request line and its headers) may take.
const HEAD: usize = 1 << 20;

/// The most headers a request may carry.
const HEADERS: usize = 100;

/// How long a request may take to arrive, head and body: a browser on this machine sends it at once.
const WAIT: Duration = Duration::from_secs(10);

/// How many connections are answered at a time.
const CONNECTIONS: usize = 32;

impl Answer {
    pub fn page(html: String) -> Answer {
        Answer { status: "200 OK", kind: "text/html; charset=utf-8", body: html.into_bytes(), file: None }
    }

    /// A page for the browser to keep as the file `name`.
    pub fn download(name: String, html: String) -> Answer {
        Answer { file: Some(name), ..Answer::page(html) }
    }

    pub fn json(json: String) -> Answer {
        Answer { status: "200 OK", kind: "application/json", body: json.into_bytes(), file: None }
    }

    pub fn text(status: &'static str, text: String) -> Answer {
        Answer { status, kind: "text/plain; charset=utf-8", body: text.into_bytes(), file: None }
    }

    /// What went wrong, in words, where the page would have been.
    pub fn failed(e: anyhow::Error) -> Answer {
        Answer::text("500 Internal Server Error", format!("{e:#}"))
    }

    /// Nothing to say.
    pub fn none() -> Answer {
        Answer { status: "204 No Content", kind: "text/plain", body: vec![], file: None }
    }
}

/// Answer every connection `listener` accepts with what `respond` says to its request, each on a
/// thread of its own, at most `CONNECTIONS` at a time, until the process stops. A connection that
/// cannot be accepted is said once through `say` until one can again, and accepting waits a while
/// between tries. A request that panics is answered with what it said.
pub fn listen(listener: TcpListener, say: impl Fn(&str), respond: impl Fn(&Request) -> Answer + Send + Sync + 'static) {
    let respond = Arc::new(respond);
    let open = Arc::new(AtomicUsize::new(0));
    let (mut failing, mut pause) = (false, Duration::from_millis(10));
    loop {
        let conn = match listener.accept() {
            Ok((conn, _)) => conn,
            Err(e) => {
                if !failing {
                    say(&format!("could not accept a connection: {e}"));
                    failing = true;
                }
                std::thread::sleep(pause);
                pause = (pause * 2).min(Duration::from_secs(1));
                continue;
            }
        };
        if failing {
            say("accepting connections again");
            (failing, pause) = (false, Duration::from_millis(10));
        }
        if open.fetch_add(1, Ordering::SeqCst) >= CONNECTIONS {
            open.fetch_sub(1, Ordering::SeqCst);
            let _ = conn.set_write_timeout(Some(Duration::from_secs(1)));
            let _ =
                reply(conn, Answer::text("503 Service Unavailable", "Atlas is answering as many requests as it takes; ask again".into()));
            continue;
        }
        let (slot, respond) = (Slot(Arc::clone(&open)), Arc::clone(&respond));
        // a thread that cannot start drops the connection and frees its slot
        let _ = std::thread::Builder::new().spawn(move || {
            let _slot = slot;
            answer(conn, &*respond);
        });
    }
}

/// One of the `CONNECTIONS` being answered, given back when its answer is written.
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Read one request from `conn` and write its answer: what `respond` says, or why the request is
/// refused, after which what the client still sends is read and let go, so that it reads the answer
/// rather than a reset connection.
fn answer(conn: TcpStream, respond: &(impl Fn(&Request) -> Answer + ?Sized)) {
    let _ = conn.set_write_timeout(Some(WAIT));
    match request(&conn) {
        Ok(req) => {
            let a =
                guarded(|| respond(&req)).unwrap_or_else(|said| Answer::text("500 Internal Server Error", format!("Atlas failed: {said}")));
            let _ = reply(conn, a);
        }
        Err(refused) => match conn.try_clone() {
            Ok(writer) => {
                let _ = reply(writer, refused);
                linger(&conn);
            }
            Err(_) => {
                let _ = reply(conn, refused);
            }
        },
    }
}

/// What `f` gives, or what it said when it panicked.
pub fn guarded<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(AssertUnwindSafe(f)).map_err(|p| panicked(&*p))
}

/// What a panic said.
fn panicked(p: &(dyn Any + Send)) -> String {
    p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "a panic".into())
}

/// What a client sends after its answer, read for a moment and let go, then the connection closed.
fn linger(conn: &TcpStream) {
    let _ = conn.shutdown(Shutdown::Write);
    let until = Instant::now() + Duration::from_secs(1);
    let mut sink = [0; 16 << 10];
    let _ = conn.set_read_timeout(Some(Duration::from_millis(100)));
    while Instant::now() < until {
        match (&*conn).read(&mut sink) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

/// A connection read until a moment: each read waits only for what is left of the time.
struct Timed<'a> {
    conn: &'a TcpStream,
    until: Instant,
}

impl Read for Timed<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        self.conn.set_read_timeout(Some(left))?;
        self.conn.read(buf)
    }
}

/// Why a request could not be read.
enum Fault {
    /// It is longer than what it may take.
    Long,
    /// It did not arrive in time.
    Slow,
    /// The connection ended before it did.
    Ended,
    /// It is not text, or not a number where one stands.
    Garbled,
}

impl Fault {
    fn of(e: io::Error) -> Fault {
        match e.kind() {
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => Fault::Slow,
            io::ErrorKind::InvalidData => Fault::Garbled,
            _ => Fault::Ended,
        }
    }

    /// The answer to a request that could not be read, `long` the status of one that is too long.
    fn answer(self, long: &'static str, what: &str) -> Answer {
        match self {
            Fault::Long => Answer::text(long, format!("{what} is longer than Atlas reads")),
            Fault::Slow => Answer::text("408 Request Timeout", format!("{what} did not arrive within {} s", WAIT.as_secs())),
            Fault::Ended => Answer::text("400 Bad Request", format!("the connection ended in {what}")),
            Fault::Garbled => Answer::text("400 Bad Request", format!("{what} does not read as HTTP")),
        }
    }
}

/// One line of a request's head, without its line end, in at most `left` bytes, which it takes from.
fn head_line(reader: &mut impl BufRead, left: &mut usize) -> Result<String, Fault> {
    let mut bytes = Vec::new();
    let read = reader.take(*left as u64).read_until(b'\n', &mut bytes).map_err(Fault::of)?;
    *left -= read;
    if bytes.last() != Some(&b'\n') {
        return Err(if read > 0 && *left == 0 { Fault::Long } else { Fault::Ended });
    }
    let line = String::from_utf8(bytes).map_err(|_| Fault::Garbled)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

/// One request: its request line, its headers and the body its `Content-Length` names; or the answer
/// to one that cannot be read within the bounds.
pub fn request(conn: &TcpStream) -> Result<Request, Answer> {
    let mut reader = BufReader::new(Timed { conn, until: Instant::now() + WAIT });
    let mut left = HEAD;
    let line = head_line(&mut reader, &mut left).map_err(|f| f.answer("414 URI Too Long", "the request line"))?;
    let mut words = line.split_whitespace();
    let (method, target) = (words.next().unwrap_or("").to_string(), words.next().unwrap_or("/").to_string());
    let mut headers: HashMap<String, String> = HashMap::new();
    for count in 0.. {
        let header = head_line(&mut reader, &mut left).map_err(|f| f.answer("431 Request Header Fields Too Large", "the headers"))?;
        if header.trim().is_empty() {
            break;
        }
        if count == HEADERS {
            return Err(Answer::text("431 Request Header Fields Too Large", format!("a request carries at most {HEADERS} headers")));
        }
        let Some((name, value)) = header.split_once(':') else { continue };
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    let length: usize = match headers.get("content-length") {
        None => 0,
        Some(n) => n.parse().map_err(|_| Answer::text("400 Bad Request", "Content-Length is no length".into()))?,
    };
    if length > MOST {
        return Err(Answer::text("413 Content Too Large", format!("a body is at most {} MiB", MOST >> 20)));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).map_err(|e| Fault::of(e).answer("413 Content Too Large", "the body"))?;
    let host = headers.get("host").cloned().unwrap_or_default();
    let own = own(&headers, &host);
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let query = query
        .split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| kv.split_once('=').unwrap_or((kv, "")))
        .map(|(k, v)| (decoded(k), decoded(v)))
        .collect();
    Ok(Request { method, path: decoded(path), query, host, own, body })
}

/// Whether a request with `headers`, sent to `host`, came from Atlas's own page or its reader, as
/// `Request::own` says.
fn own(headers: &HashMap<String, String>, host: &str) -> bool {
    if headers.contains_key("x-atlas") {
        return true;
    }
    if let Some(site) = headers.get("sec-fetch-site") {
        return site == "same-origin" || site == "none";
    }
    let at = |url: &str| url.strip_prefix("http://").map(|rest| rest.split('/').next().unwrap_or("") == host);
    headers.get("origin").or_else(|| headers.get("referer")).and_then(|u| at(u)).unwrap_or(false)
}

/// A part of an address with its escapes (`%2F`, `+`) read back.
pub(crate) fn decoded(s: &str) -> String {
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
    // a file name the browser keeps: what a name may hold in a header, nothing else
    let keep = a.file.map_or(String::new(), |f| {
        let name: String = f.chars().filter(|c| c.is_ascii_alphanumeric() || "._-".contains(*c)).collect();
        format!("Content-Disposition: attachment; filename=\"{name}\"\r\n")
    });
    let head = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n{keep}Cache-Control: no-cache\r\nConnection: close\r\n\r\n",
        a.status,
        a.kind,
        a.body.len()
    );
    conn.write_all(head.as_bytes())?;
    conn.write_all(&a.body)?;
    Ok(conn.flush()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    /// A request is the page's own when it says so, when the browser says it comes from this address
    /// or from its reader, or, from a browser that says nothing of where, when its origin is this
    /// address; never when the browser says another site sent it, whatever the page's header would say
    /// of an older browser.
    #[test]
    fn a_request_is_own_only_when_the_page_or_the_browser_says_so() {
        let host = "127.0.0.1:8778";
        assert!(own(&headers(&[("x-atlas", "1")]), host));
        assert!(own(&headers(&[("sec-fetch-site", "same-origin")]), host));
        assert!(own(&headers(&[("sec-fetch-site", "none")]), host), "the reader typed it, or opened a link from outside the browser");
        assert!(!own(&headers(&[("sec-fetch-site", "cross-site")]), host));
        assert!(!own(&headers(&[("sec-fetch-site", "same-site")]), host), "another server on this machine is another site");
        assert!(!own(&headers(&[("sec-fetch-site", "cross-site"), ("referer", "http://127.0.0.1:8778/")]), host));
        assert!(own(&headers(&[("referer", "http://127.0.0.1:8778/history?a=1")]), host));
        assert!(own(&headers(&[("origin", "http://127.0.0.1:8778")]), host));
        assert!(!own(&headers(&[("origin", "http://127.0.0.1:8779")]), host));
        assert!(!own(&headers(&[("referer", "https://example.org/")]), host));
        assert!(!own(&headers(&[]), host), "a client that says nothing of itself is not the page");
    }

    /// A request that panics is answered with what the panic said, and the server goes on.
    #[test]
    fn a_panic_is_what_it_said() {
        assert_eq!(guarded(|| -> u8 { panic!("a step made to panic") }), Err("a step made to panic".into()));
        assert_eq!(guarded(|| -> u8 { panic!("{} made to panic", "a step") }), Err("a step made to panic".into()));
        assert_eq!(guarded(|| 7), Ok(7));
    }
}
