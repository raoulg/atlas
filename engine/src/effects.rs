//! Boundaries: calls that leave the package, classified by the part of the world they
//! touch. The engine knows libraries, never a project.

use std::collections::HashSet;

const NET_PKGS: &[&str] = &["requests", "httpx", "urllib", "http", "socket", "aiohttp"];
const LLM_PKGS: &[&str] = &["openai", "anthropic"];
/// Libraries that compute in memory. Most of their calls stay in the program (`torch.cat`,
/// `nn.Linear`); `ml` names the few that cross to a place.
const ML_PKGS: &[&str] = &["sentence_transformers", "torch", "transformers", "huggingface_hub", "datasets", "torchvision"];
/// The Hugging Face Hub: what comes down, and what goes up.
const HUB_READ: &[&str] = &["snapshot_download", "hf_hub_download", "load_dataset", "repo_info", "dataset_info", "model_info", "list_repo_files"];
/// Pretrained weights, downloaded from the Hub by name: `AutoModel.from_pretrained(name)`.
const HUB_MODELS: &[&str] = &["from_pretrained", "SentenceTransformer", "CrossEncoder", "pipeline"];
const HUB_WRITE: &[&str] = &["upload_folder", "upload_file", "upload_large_folder", "create_repo", "create_commit", "delete_file", "delete_folder", "push_to_hub"];
const HUB_HOST: &str = "huggingface.co";
const FILE_PKGS: &[&str] = &[
    "json", "yaml", "pickle", "csv", "tarfile", "zipfile", "shutil", "os", "pathlib", "pandas",
    "geopandas", "numpy", "rasterio", "lxml", "PIL", "matplotlib", "plt", "ruamel", "np", "pd",
];
const READ_BOUND: &[&str] = &[
    "read_text", "read_bytes", "read_csv", "read_parquet", "read_file", "read_json", "read_excel",
    "iterdir", "glob", "rglob", "load", "load_workbook", "parse", "fromfile", "imread",
    "open_dataset", "read", "readlines", "safe_load", "listdir", "walk", "open",
];
const WRITE_BOUND: &[&str] = &[
    "write_text", "write_bytes", "to_csv", "to_parquet", "to_json", "dump", "safe_dump", "savefig",
    "save", "mkdir", "copy", "copyfile", "copytree", "move", "unlink", "rmtree", "touch", "imwrite",
    "extractall", "makedirs", "rename", "replace", "writelines",
];
const READ_UNBOUND: &[&str] = &[
    "read_text", "read_bytes", "iterdir", "glob", "rglob", "read", "readlines", "read_csv",
    "read_parquet", "read_file",
];
const WRITE_UNBOUND: &[&str] = &[
    "write_text", "write_bytes", "mkdir", "unlink", "touch", "to_csv", "to_parquet", "to_json",
    "savefig", "writelines", "rmtree",
];

fn db_pkg(pkg: &str) -> Option<&'static str> {
    match pkg {
        "duckdb" => Some("db_duckdb"),
        "neo4j" | "mgclient" => Some("db_memgraph"),
        "sqlite3" => Some("db_sqlite"),
        "psycopg2" | "sqlalchemy" => Some("db_sql"),
        _ => None,
    }
}

/// Classify a Rust call by its full path, e.g. `std::fs::read_to_string` -> `disk_read`.
pub fn boundary_rust(parts: &[String]) -> Option<&'static str> {
    let head = parts.first().map(String::as_str).unwrap_or("");
    let last = parts.last().map(String::as_str).unwrap_or("");
    let has = |p: &str| parts.iter().any(|x| x == p);
    if has("fs") {
        return Some(match last {
            "read" | "read_to_string" | "read_dir" | "metadata" | "canonicalize" | "exists" | "open" | "read_link" => "disk_read",
            "write" | "create" | "create_dir" | "create_dir_all" | "remove_file" | "remove_dir" | "remove_dir_all" | "rename" | "copy" | "set_permissions" => "disk_write",
            _ => "disk",
        });
    }
    if has("process") || head == "Command" {
        return Some("process");
    }
    match head {
        "walkdir" | "WalkDir" | "glob" | "notify" => Some("disk_read"),
        "reqwest" | "ureq" | "hyper" | "tungstenite" | "tokio_tungstenite" | "curl" | "axum" | "TcpStream" | "TcpListener" => Some("net"),
        "rusqlite" | "sqlx" | "diesel" | "duckdb" | "postgres" | "tokio_postgres" | "sea_orm" => Some("db_sql"),
        "git2" | "gix" => Some("disk_read"),
        _ => None,
    }
}

/// Classify a call that leaves the package.
///
/// `pkg` is the external top-level package the call's head name is bound to, if any;
/// `parts` the dotted call name as written; `ext` the external packages the module imports.
pub fn boundary(pkg: Option<&str>, parts: &[String], ext: &HashSet<String>) -> Option<&'static str> {
    let head = parts.first().map(String::as_str).unwrap_or("");
    let last = parts.last().map(String::as_str).unwrap_or("");
    if let Some(pkg) = pkg {
        if matches!((pkg, last), ("click" | "typer", "echo" | "secho") | ("rich", "print") | ("pprint", "pprint" | "pp"))
            || (pkg == "sys" && parts.len() == 3 && matches!(parts[1].as_str(), "stdout" | "stderr"))
        {
            return Some("screen");
        }
        if NET_PKGS.contains(&pkg) {
            return Some("net");
        }
        if let Some(db) = db_pkg(pkg) {
            return Some(db);
        }
        if LLM_PKGS.contains(&pkg) {
            return Some("llm");
        }
        if ML_PKGS.contains(&pkg) {
            return ml(pkg, parts, last);
        }
        if pkg == "subprocess" {
            return Some("process");
        }
        if FILE_PKGS.contains(&pkg) {
            if WRITE_BOUND.contains(&last) {
                return Some("disk_write");
            }
            if READ_BOUND.contains(&last) {
                return Some("disk_read");
            }
        }
        return None;
    }
    if parts.len() == 1 && head == "print" {
        return Some("screen");
    }
    if parts.len() == 1 && head == "open" {
        return Some("disk");
    }
    if READ_UNBOUND.contains(&last) {
        return Some("disk_read");
    }
    if WRITE_UNBOUND.contains(&last) {
        return Some("disk_write");
    }
    let has = |p: &str| ext.contains(p);
    if (last == "run" || last == "execute_query") && (has("neo4j") || has("mgclient")) {
        return Some("db_memgraph");
    }
    if ["execute", "executemany", "sql", "register"].contains(&last) && has("duckdb") {
        return Some("db_duckdb");
    }
    if ["execute", "executemany"].contains(&last) && has("sqlite3") {
        return Some("db_sqlite");
    }
    if last == "create" && LLM_PKGS.iter().any(|p| has(p)) {
        return Some("llm");
    }
    if ["encode", "embed"].contains(&last) && has("sentence_transformers") {
        return Some("model");
    }
    if last == "push_to_hub" && (has("datasets") || has("huggingface_hub") || has("transformers")) {
        return Some("net");
    }
    if last == "save_to_disk" && has("datasets") {
        return Some("disk_write");
    }
    None
}

/// The calls of a machine-learning library that cross to a place: pretrained weights load a
/// model, the Hub sends and receives files, a checkpoint or a saved dataset is a file.
fn ml(pkg: &str, parts: &[String], last: &str) -> Option<&'static str> {
    let hub_load = pkg == "torch" && parts.iter().any(|p| p == "hub") && last == "load";
    let pretrained = last == "from_pretrained" || matches!((pkg, last), ("sentence_transformers", "SentenceTransformer" | "CrossEncoder") | ("transformers", "pipeline"));
    if pretrained || hub_load {
        return Some("net");
    }
    if matches!(pkg, "huggingface_hub" | "datasets") && (HUB_READ.contains(&last) || HUB_WRITE.contains(&last)) {
        return Some("net");
    }
    match (pkg, last) {
        ("torch" | "datasets", "load" | "load_from_disk") => Some("disk_read"),
        ("torch" | "datasets", "save" | "save_to_disk") => Some("disk_write"),
        _ => None,
    }
}

/// `Path.exists()` and friends on a receiver whose type is unknown still read the disk.
pub const RUST_PATH_READS: &[&str] = &["exists", "try_exists", "is_file", "is_dir", "read_dir", "canonicalize", "symlink_metadata"];

/// `println!` and friends print to the terminal.
pub fn screen_macro(name: &str) -> bool {
    matches!(name, "println" | "print" | "eprintln" | "eprint")
}

/// What a boundary call is, once the string literals under it have been read: the part of the
/// world it touches, which file or table it names, and whether a database is read or written.
#[derive(Debug, PartialEq)]
pub struct Facts {
    pub kind: &'static str,
    pub artifact: String,
    pub dir: &'static str,
}

/// Refine a boundary by its literals: `open(p, "w")` writes, `to_csv(path)` writes a csv,
/// `Path("out.json")` names its file, `execute("INSERT INTO nodes ..")` writes the table `nodes`.
pub fn facts(kind: &'static str, pkg: Option<&str>, op: &str, literals: &[String]) -> Facts {
    // a file call reads unless a mode says it writes; `open` is one whatever its package
    let file = kind == "disk";
    let kind = if op == "open" || file {
        match literals.iter().find(|l| is_mode(l)) {
            Some(m) if m.contains(['w', 'a', 'x', '+']) => "disk_write",
            Some(_) => "disk_read",
            None if file => "disk_read",
            None => kind,
        }
    } else {
        kind
    };
    if kind.starts_with("db_") {
        let Some((dir, table)) = literals.iter().find_map(|l| statement(l)) else {
            return Facts { kind, artifact: String::new(), dir: "" };
        };
        return Facts { kind, artifact: table, dir };
    }
    if kind == "net" {
        // pretrained weights named by a local path are a file, by a name a download from the Hub
        if HUB_MODELS.contains(&op) {
            if let Some(local) = literals.first().filter(|l| l.starts_with(['.', '/', '~'])) {
                return Facts { kind: "disk_read", artifact: local.clone(), dir: "" };
            }
        }
        // a web place is named by its host; the Hub's operations say which way the files go
        let hub = HUB_READ.contains(&op) || HUB_WRITE.contains(&op) || HUB_MODELS.contains(&op);
        let artifact = if hub { HUB_HOST.to_string() } else { literals.iter().find_map(|l| host(l)).unwrap_or_default() };
        let dir = if HUB_WRITE.contains(&op) { "write" } else if hub { "read" } else { "" };
        return Facts { kind, artifact, dir };
    }
    if !kind.starts_with("disk") {
        return Facts { kind, artifact: String::new(), dir: "" };
    }
    let artifact = literals
        .iter()
        .find(|l| path_like(l))
        .cloned()
        .or_else(|| literals.iter().find_map(|l| extension(l)).map(|e| format!("*.{e}")))
        .or_else(|| format_of(pkg, op).map(|f| format!("*.{f}")))
        .unwrap_or_default();
    Facts { kind, artifact, dir: "" }
}

/// A file mode literal: `r`, `wb`, `a+`.
/// Path operations whose place is the path they are called on; their argument is a pattern
/// below it or what is written: `cache_dir.glob("*.arrow")`, `p.write_text("done")`.
pub fn on_receiver(op: &str) -> bool {
    matches!(
        op,
        "glob" | "rglob" | "iterdir" | "joinpath" | "read_text" | "read_bytes" | "write_text" | "write_bytes" | "exists" | "is_file"
            | "is_dir" | "mkdir" | "unlink" | "rmdir" | "touch" | "stat"
    )
}

/// The host of a URL: `https://huggingface.co/api/x` -> `huggingface.co`.
fn host(s: &str) -> Option<String> {
    let rest = s.split_once("://")?.1;
    let h = rest.split(['/', '?', '#']).next()?;
    (!h.is_empty() && !h.contains('{')).then(|| h.to_string())
}

/// A boundary whose literal argument names a place someone could want elsewhere: a file or
/// a URL. A table in a statement or a command line is what the call does, not where.
pub fn locates(kind: &str) -> bool {
    kind.starts_with("disk") || kind == "net"
}

pub fn is_mode(s: &str) -> bool {
    (1..=3).contains(&s.len()) && s.chars().all(|c| "rwabxt+".contains(c))
}

/// The format a call implies when the code names no file.
fn format_of(pkg: Option<&str>, op: &str) -> Option<&'static str> {
    let by_op = match op {
        "read_csv" | "to_csv" => Some("csv"),
        "read_parquet" | "to_parquet" => Some("parquet"),
        "read_json" | "to_json" => Some("json"),
        "read_excel" | "to_excel" | "load_workbook" => Some("xlsx"),
        "read_feather" | "to_feather" => Some("feather"),
        "savefig" | "imsave" => Some("png"),
        "safe_load" | "safe_dump" => Some("yaml"),
        _ => None,
    };
    if by_op.is_some() {
        return by_op;
    }
    match pkg? {
        "json" => Some("json"),
        "yaml" | "ruamel" => Some("yaml"),
        "pickle" => Some("pkl"),
        "csv" => Some("csv"),
        "toml" | "tomllib" | "tomli" => Some("toml"),
        "lxml" => Some("xml"),
        "rasterio" => Some("tif"),
        "numpy" | "np" if matches!(op, "save" | "load") => Some("npy"),
        _ => None,
    }
}

/// A literal that names a file: `out.json`, `assets/{id}/x.xml`, `Makefile`. Not a mode, an
/// encoding, a URL or a sentence.
fn path_like(s: &str) -> bool {
    let s = &holes(s);
    let Some(first) = s.chars().next() else { return false };
    s.len() >= 2
        && !s.contains("://")
        && !is_mode(s)
        && s.chars().all(|c| c.is_alphanumeric() || "._-/{}*~".contains(c))
        && (s.contains('/') || extension(s).is_some() || (first.is_ascii_uppercase() && s.chars().all(|c| c.is_alphanumeric())))
}

/// The extension a literal ends in: `{name}.png` -> `png`, `.env` -> `env`.
/// A pattern with its interpolations as holes, whatever fills them:
/// `figures/{safe(name)}.png` -> `figures/{}.png`.
fn holes(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '{' => {
                if depth == 0 {
                    out.push('{');
                }
                depth += 1;
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    out.push('}');
                }
            }
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn extension(s: &str) -> Option<String> {
    let (_, e) = s.rsplit_once('.')?;
    let ok = (1..=8).contains(&e.len()) && e.chars().all(|c| c.is_ascii_alphanumeric()) && e.chars().any(|c| c.is_ascii_alphabetic());
    (ok && !s.contains(char::is_whitespace)).then(|| e.to_ascii_lowercase())
}

/// A SQL or Cypher statement: whether it reads or writes, and the table or label it names.
/// Any write verb makes it a write (`MATCH .. MERGE`, `WITH .. INSERT`).
pub fn statement(s: &str) -> Option<(&'static str, String)> {
    const WRITE: &[&str] = &["insert", "update", "delete", "create", "merge", "copy", "drop", "alter", "replace", "upsert", "set", "detach", "remove"];
    const READ: &[&str] = &["select", "match", "with", "return", "show", "describe", "pragma", "call", "unwind"];
    let words: Vec<String> = s.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).map(|w| w.to_ascii_lowercase()).collect();
    let first = words.first()?;
    if !WRITE.contains(&first.as_str()) && !READ.contains(&first.as_str()) {
        return None;
    }
    let dir = if words.iter().any(|w| WRITE.contains(&w.as_str())) { "write" } else { "read" };
    // a Cypher label `(n:Regeling)`, else the word after a SQL keyword that names a table
    let label = s.match_indices(':').find_map(|(i, _)| {
        let before = s[..i].chars().last()?;
        let in_node = s[..i].rfind('(') > s[..i].rfind(')');
        if !(in_node && (before.is_alphanumeric() || before == '_' || before == '(')) {
            return None;
        }
        let l: String = s[i + 1..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        (!l.is_empty()).then_some(l)
    });
    let table = label.or_else(|| {
        words.windows(2).find(|w| matches!(w[0].as_str(), "into" | "from" | "update" | "table" | "join")).map(|w| w[1].clone())
    });
    Some((dir, table.unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lits(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    fn parts(xs: &[&str]) -> Vec<String> {
        lits(xs)
    }

    /// A machine-learning library computes in memory; only loading weights, moving files to
    /// and from the Hub, and checkpoints cross to a place.
    #[test]
    fn a_library_is_not_a_place() {
        let none = HashSet::new();
        assert_eq!(boundary(Some("torch"), &parts(&["torch", "cat"]), &none), None);
        assert_eq!(boundary(Some("torch"), &parts(&["nn", "Linear"]), &none), None);
        assert_eq!(boundary(Some("transformers"), &parts(&["AutoModel", "from_pretrained"]), &none), Some("net"), "weights come down from the Hub");
        assert_eq!(boundary(Some("torch"), &parts(&["torch", "save"]), &none), Some("disk_write"));
        assert_eq!(boundary(Some("huggingface_hub"), &parts(&["snapshot_download"]), &none), Some("net"));
        assert_eq!(boundary(Some("huggingface_hub"), &parts(&["HfApi"]), &none), None, "building a client moves nothing");
        assert_eq!(boundary(Some("datasets"), &parts(&["load_dataset"]), &none), Some("net"));
        let down = facts("net", Some("huggingface_hub"), "snapshot_download", &lits(&["dataset"]));
        assert_eq!((down.artifact.as_str(), down.dir), ("huggingface.co", "read"), "the Hub is a host; a download reads");
        assert_eq!(facts("net", Some("huggingface_hub"), "upload_folder", &[]).dir, "write");
        assert_eq!(facts("net", Some("requests"), "get", &lits(&["https://api.example.org/v1/x"])).artifact, "api.example.org");
        let model = facts("net", Some("transformers"), "from_pretrained", &lits(&["facebook/dinov2-small", "auto"]));
        assert_eq!((model.kind, model.artifact.as_str(), model.dir), ("net", "huggingface.co", "read"));
        assert_eq!(facts("net", Some("transformers"), "from_pretrained", &lits(&["./checkpoints/best"])).kind, "disk_read", "a local checkpoint is a file");
    }

    #[test]
    fn files_are_named_by_literal_then_format() {
        assert_eq!(facts("disk_write", None, "write_text", &lits(&["out.json"])).artifact, "out.json");
        assert_eq!(facts("disk_write", None, "to_csv", &lits(&[";"])).artifact, "*.csv");
        assert_eq!(facts("disk_write", None, "savefig", &lits(&["{name}.svg"])).artifact, "{name}.svg");
        assert_eq!(facts("disk_write", Some("json"), "dump", &[]).artifact, "*.json");
        assert_eq!(facts("disk_read", None, "read_text", &lits(&["utf-8"])).artifact, "");
        assert_eq!(facts("disk_read", None, "read_to_string", &lits(&["Makefile"])).artifact, "Makefile");
    }

    #[test]
    fn open_mode_decides_read_or_write() {
        assert_eq!(facts("disk", None, "open", &lits(&["w"])).kind, "disk_write");
        assert_eq!(facts("disk", None, "open", &lits(&["data.csv"])).kind, "disk_read");
        assert_eq!(facts("disk", None, "open", &lits(&["data.csv", "w"])).artifact, "data.csv");
    }

    #[test]
    fn statements_read_or_write_a_table() {
        assert_eq!(statement("INSERT INTO nodes VALUES (?)"), Some(("write", "nodes".into())));
        assert_eq!(statement("select * from edges where x = 1"), Some(("read", "edges".into())));
        assert_eq!(statement("MATCH (n:Regeling) RETURN n"), Some(("read", "Regeling".into())));
        assert_eq!(statement("MATCH (a) MERGE (b:Node {id: $id})"), Some(("write", "Node".into())));
        assert_eq!(statement("hello world"), None);
    }
}
