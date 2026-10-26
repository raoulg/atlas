//! The workspace's files: the listing Atlas reads, asked of the sidecar, and a file's bytes for
//! its preview, read only inside the open workspace.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

/// The largest file a preview reads.
const MOST: u64 = 8 << 20;

/// The files Atlas reads in the workspace served at `url`, each with what the graph defines in it,
/// as `atlas serve` answers at `/api/files`.
pub fn listing(url: &str) -> Result<String, String> {
    let at = url.strip_prefix("http://").ok_or("Atlas serves at an address Studio does not know")?.trim_end_matches('/');
    let mut conn = TcpStream::connect(at).map_err(|error| format!("Atlas does not answer: {error}"))?;
    conn.set_read_timeout(Some(Duration::from_secs(30))).map_err(|error| error.to_string())?;
    write!(conn, "GET /api/files HTTP/1.1\r\nHost: {at}\r\nConnection: close\r\n\r\n").map_err(|error| error.to_string())?;
    let mut answer = String::new();
    conn.read_to_string(&mut answer).map_err(|error| error.to_string())?;
    let (head, body) = answer.split_once("\r\n\r\n").ok_or("Atlas gave no answer")?;
    let status = head.lines().next().unwrap_or_default();
    if !status.contains(" 200 ") {
        return Err(format!("Atlas could not list the files: {status} {}", body.trim()));
    }
    Ok(body.to_string())
}

/// The bytes of the file at `relative` in `workspace`, for a preview.
pub fn read(workspace: &Path, relative: &str) -> Result<Vec<u8>, String> {
    let root = workspace.canonicalize().map_err(|error| format!("Cannot open workspace: {error}"))?;
    let path = root.join(relative).canonicalize().map_err(|error| format!("Cannot open {relative}: {error}"))?;
    if !path.starts_with(&root) {
        return Err("Studio reads files inside the open workspace only".into());
    }
    let size = std::fs::metadata(&path).map_err(|error| error.to_string())?.len();
    if size > MOST {
        return Err(format!("{relative} is {} MB, more than a preview reads", size >> 20));
    }
    std::fs::read(&path).map_err(|error| format!("Cannot read {relative}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
