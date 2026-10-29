//! The workspace's files: the listing Atlas reads, asked of the sidecar, and a file's bytes for
//! its preview, read only inside the open workspace. An HTML file is previewed at `preview:`, where
//! the files it refers to load from the workspace and nothing runs.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use crate::workspace::Workspace;

/// The largest file a preview reads.
const MOST: u64 = 8 << 20;

/// What `atlas serve` at `url` answers to a GET of `path`: the body, and the name of the file it is
/// when the answer is one to keep.
pub fn get(url: &str, path: &str) -> Result<(Vec<u8>, Option<String>), String> {
    let at = url.strip_prefix("http://").ok_or("Atlas serves at an address Studio does not know")?.trim_end_matches('/');
    let mut conn = TcpStream::connect(at).map_err(|error| format!("Atlas does not answer: {error}"))?;
    conn.set_read_timeout(Some(Duration::from_secs(120))).map_err(|error| error.to_string())?;
    write!(conn, "GET {path} HTTP/1.1\r\nHost: {at}\r\nX-Atlas: 1\r\nConnection: close\r\n\r\n").map_err(|error| error.to_string())?;
    let mut answer = Vec::new();
    conn.read_to_end(&mut answer).map_err(|error| error.to_string())?;
    let split = answer.windows(4).position(|window| window == b"\r\n\r\n").ok_or("Atlas gave no answer")?;
    let head = String::from_utf8_lossy(&answer[..split]).into_owned();
    let body = answer[split + 4..].to_vec();
    let status = head.lines().next().unwrap_or_default();
    if !status.contains(" 200 ") {
        return Err(format!("Atlas answered {status}: {}", String::from_utf8_lossy(&body).trim()));
    }
    let name = head
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("content-disposition:"))
        .and_then(|line| line.split("filename=").nth(1))
        .map(|name| name.trim().trim_matches('"').to_string());
    Ok((body, name))
}

/// The files Atlas reads in the workspace served at `url`, each with what the graph defines in it,
/// as `atlas serve` answers at `/api/files`.
pub fn listing(url: &str) -> Result<String, String> {
    let (body, _) = get(url, "/api/files")?;
    String::from_utf8(body).map_err(|error| error.to_string())
}

/// `text` as it may stand in a URL's query.
pub fn query(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// The bytes of the file at `relative` in `workspace`, for a preview.
pub fn read(folder: &Path, relative: &str) -> Result<Vec<u8>, String> {
    let workspace = Workspace::open(folder)?;
    let path = workspace.inside(&workspace.root().join(relative), relative, "Studio reads files inside the open workspace only")?;
    let size = workspace.stamp(&path).map_or(0, |(size, _)| size);
    workspace.bytes(&path, MOST, format!("{relative} is {} MB, more than a preview reads", size >> 20), relative)
}

/// What a page at `preview:` may load: files of the workspace, styles written in it, and nothing
/// else; no script, no plugin, no frame, no form, nothing from the network.
pub const PREVIEW_POLICY: &str = "default-src preview: http://preview.localhost https://preview.localhost data: blob:; \
    style-src preview: http://preview.localhost https://preview.localhost data: 'unsafe-inline'; script-src 'none'; \
    object-src 'none'; frame-src 'none'; form-action 'none'; base-uri 'none'";

/// The answer at `preview:` for `path` (as it stands in the URL) in `workspace`: the file, with its
/// type and the preview's policy; or why not.
pub fn preview(workspace: Option<&Path>, path: &str) -> tauri::http::Response<Vec<u8>> {
    let answer = |status: u16, kind: &str, body: Vec<u8>| {
        tauri::http::Response::builder()
            .status(status)
            .header("Content-Type", kind)
            .header("Content-Security-Policy", PREVIEW_POLICY)
            .header("X-Content-Type-Options", "nosniff")
            .body(body)
            .unwrap_or_default()
    };
    let Some(workspace) = workspace else { return answer(404, "text/plain", b"No workspace is open".to_vec()) };
    let relative = unquote(path.trim_start_matches('/'));
    match read(workspace, &relative) {
        Ok(bytes) => answer(200, kind(&relative), bytes),
        Err(error) => answer(404, "text/plain", error.into_bytes()),
    }
}

/// A path as it stands in a URL, `%20` and all, as the file system names it.
fn unquote(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (bytes[at], bytes.get(at + 1).and_then(|b| hex(*b)), bytes.get(at + 2).and_then(|b| hex(*b))) {
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                at += 3;
            }
            (byte, _, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A file's type, by its extension.
fn kind(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default().to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "txt" | "md" | "csv" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preview_serves_the_workspace_only_and_runs_nothing() {
        let base = std::env::temp_dir().join(format!("atlas-studio-preview-{}", std::process::id()));
        let workspace = base.join("work space");
        std::fs::create_dir_all(workspace.join("docs/img")).unwrap();
        std::fs::write(workspace.join("docs/img/a b.svg"), "<svg/>").unwrap();
        std::fs::write(base.join("secret.txt"), "secret").unwrap();

        let image = preview(Some(&workspace), "/docs/img/a%20b.svg");
        assert_eq!(image.status(), 200);
        assert_eq!(image.headers()["Content-Type"], "image/svg+xml");
        assert!(image.headers()["Content-Security-Policy"].to_str().unwrap().contains("script-src 'none'"));
        assert_eq!(preview(Some(&workspace), "/..%2Fsecret.txt").status(), 404);
        assert_eq!(preview(None, "/docs/img/a%20b.svg").status(), 404);

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_view_stands_in_a_query_as_it_was() {
        assert_eq!(query(r#"{"route":"a b"}"#), "%7B%22route%22%3A%22a%20b%22%7D");
    }

    #[test]
    fn a_file_outside_the_workspace_is_not_read() {
        let base = std::env::temp_dir().join(format!("atlas-studio-files-{}", std::process::id()));
        let workspace = base.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("inside.txt"), "inside").unwrap();
        std::fs::write(base.join("outside.txt"), "outside").unwrap();

        assert_eq!(read(&workspace, "inside.txt").unwrap(), b"inside");
        assert!(read(&workspace, "../outside.txt").is_err());

        std::fs::remove_dir_all(&base).unwrap();
    }
}
