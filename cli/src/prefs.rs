//! The reader's choices of the page (the lines' shape, the code panel, the legend's help), kept in one file of the
//! reader's: every repository is served at an address of its own, whose browser storage is its own, and a choice is
//! the reader's, not the repository's.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::{Map, Value};

/// The file the choices are kept in: `$XDG_CONFIG_HOME/atlas/viewer.json`, else `~/.config/atlas/viewer.json`, and on
/// Windows `%APPDATA%\atlas\viewer.json`.
fn file() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::home_dir().map(|h| h.join(".config")))
    };
    base.map(|b| b.join("atlas").join("viewer.json"))
}

/// The choices kept, an empty object when none are.
pub(crate) fn read() -> Result<Value> {
    file().map_or_else(|| Ok(Value::Object(Map::new())), |path| read_at(&path))
}

/// Keep the choices `said` (an object of names to strings) beside those kept, and answer what is kept now.
pub(crate) fn add(said: &Value) -> Result<Value> {
    let Some(path) = file() else { bail!("this machine names no home for the reader's choices") };
    add_at(&path, said)
}

fn read_at(path: &Path) -> Result<Value> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(serde_json::from_str::<Value>(&text).ok().filter(Value::is_object).unwrap_or_else(|| Value::Object(Map::new()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Map::new())),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

fn add_at(path: &Path, said: &Value) -> Result<Value> {
    let Some(said) = said.as_object().filter(|o| o.values().all(Value::is_string)) else {
        bail!("a choice of the page is a name and a string")
    };
    let mut kept = read_at(path)?;
    let map = kept.as_object_mut().expect("read answers an object");
    for (k, v) in said {
        map.insert(k.clone(), v.clone());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&kept)?).with_context(|| format!("writing {}", path.display()))?;
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Interface contract: a choice kept is read back on the next page, and keeping one leaves the others as they were.
    #[test]
    fn a_choice_kept_is_read_back_and_leaves_the_others() {
        let dir = std::env::temp_dir().join(format!("atlas-prefs-{}", std::process::id()));
        let path = dir.join("atlas").join("viewer.json");
        assert_eq!(read_at(&path).unwrap(), json!({}), "no file: no choices");
        add_at(&path, &json!({"atlas.lines": "square", "atlas.code": "1"})).unwrap();
        assert_eq!(add_at(&path, &json!({"atlas.lines": "round"})).unwrap(), json!({"atlas.lines": "round", "atlas.code": "1"}));
        assert_eq!(read_at(&path).unwrap(), json!({"atlas.lines": "round", "atlas.code": "1"}));
        assert!(add_at(&path, &json!({"atlas.lines": 1})).is_err(), "a choice is a string");
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(read_at(&path).unwrap(), json!({}), "a file that is not an object holds no choices");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
