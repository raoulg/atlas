//! Boundaries: calls that leave the package, classified by the part of the world they
//! touch. The engine knows libraries, never a project.

use std::collections::HashSet;

const NET_PKGS: &[&str] = &["requests", "httpx", "urllib", "http", "socket", "aiohttp"];
const LLM_PKGS: &[&str] = &["openai", "anthropic"];
const MODEL_PKGS: &[&str] = &[
    "sentence_transformers",
    "torch",
    "transformers",
    "huggingface_hub",
    "datasets",
    "torchvision",
];
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

/// Classify a call that leaves the package.
///
/// `pkg` is the external top-level package the call's head name is bound to, if any;
/// `parts` the dotted call name as written; `ext` the external packages the module imports.
pub fn boundary(pkg: Option<&str>, parts: &[String], ext: &HashSet<String>) -> Option<&'static str> {
    let head = parts.first().map(String::as_str).unwrap_or("");
    let last = parts.last().map(String::as_str).unwrap_or("");
    if let Some(pkg) = pkg {
        if NET_PKGS.contains(&pkg) {
            return Some("net");
        }
        if let Some(db) = db_pkg(pkg) {
            return Some(db);
        }
        if LLM_PKGS.contains(&pkg) {
            return Some("llm");
        }
        if MODEL_PKGS.contains(&pkg) {
            return Some("model");
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
    None
}
