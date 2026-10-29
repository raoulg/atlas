//! A vision decided: its files written into the tree, what it adds marked as prophecy.

use crate::model::{Graph, Node, NodeKind};

/// A vision decided: its files as they are written into the tree, every item the vision adds marked
/// as a prophecy in the language of its file.
#[derive(Debug, Default)]
pub struct Prophetized {
    /// Each file by its path from the repository's root, whole.
    pub files: std::collections::BTreeMap<std::path::PathBuf, String>,
    /// What was marked: the ids of the items.
    pub marked: Vec<String>,
    /// What the person has to do or know, as the vision cannot do it for them.
    pub notes: Vec<String>,
}

/// Decide a vision. Its files replace the tree's, so a vision that changes or removes something
/// that exists is no prophecy: one id cannot be in a module twice, and what replaces something
/// is a new name that `supersedes` it. `supersedes` says, for an added item by its id, the
/// existing item it replaces. Nothing is written here.
pub fn prophetize(
    repo: &std::path::Path,
    vision: &std::collections::BTreeMap<std::path::PathBuf, Option<String>>,
    supersedes: &std::collections::BTreeMap<String, String>,
    opts: &crate::Options,
) -> anyhow::Result<Prophetized> {
    use anyhow::bail;
    let before = crate::parts::analyze_parts(&[repo.to_path_buf()], opts)?;
    let after = crate::changes::envisioned(repo, vision, opts)?;
    let known: std::collections::HashSet<&str> = before.iter().flat_map(|g| g.nodes.iter().map(|n| n.id.as_str())).collect();
    for (new, old) in supersedes {
        if !known.contains(old.as_str()) {
            bail!("`{new}` is said to supersede `{old}`, which the code does not hold");
        }
    }
    let rel_of = |g: &Graph, n: &Node| -> std::path::PathBuf {
        std::path::Path::new(&n.file)
            .strip_prefix(&g.project_root)
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|_| n.file.clone().into())
    };
    let in_file = |rel: &std::path::Path, r: &std::path::Path| r.ends_with(rel);
    let mut out = Prophetized::default();
    for (rel, text) in vision {
        let Some(text) = text else { bail!("a vision that removes `{}` is no prophecy: nothing is written for a removal", rel.display()) };
        let lines: Vec<&str> = text.lines().collect();
        let tree_text = std::fs::read_to_string(repo.join(rel)).unwrap_or_default();
        let tree_lines: Vec<&str> = tree_text.lines().collect();
        let slice = |ls: &[&str], n: &Node| -> String {
            n.span.map(|(a, b)| ls.get(a as usize - 1..(b as usize).min(ls.len())).unwrap_or(&[]).join("\n")).unwrap_or_default()
        };
        let mine = |gs: &[Graph]| -> Vec<(Node, std::path::PathBuf)> {
            gs.iter()
                .flat_map(|g| {
                    g.nodes
                        .iter()
                        .filter(|n| matches!(n.kind.tag(), NodeKind::Function | NodeKind::Method | NodeKind::Class) && !n.library)
                        .map(move |n| (n.clone(), rel_of(g, n)))
                })
                .filter(|(_, r)| in_file(rel, r))
                .collect()
        };
        let (was, now) = (mine(&before), mine(&after));
        for (n, _) in &was {
            match now.iter().find(|(m, _)| m.id == n.id) {
                None => bail!("the vision removes `{}` from `{}`: a vision that replaces what exists is no prophecy; give the new item a new name that `supersedes` it", n.id, rel.display()),
                Some((m, _)) if slice(&tree_lines, n) != slice(&lines, m) => bail!(
                    "the vision changes `{}` in `{}`: writing it would replace what is built with a sketch. Give the new item a new name that `supersedes` it",
                    n.id,
                    rel.display()
                ),
                Some(_) => {}
            }
        }
        let added: Vec<&Node> = now.iter().map(|(n, _)| n).filter(|n| !was.iter().any(|(w, _)| w.id == n.id)).collect();
        // an item inside an added class is marked with its class
        let top: Vec<&Node> = added.iter().copied().filter(|n| !n.cls().is_some_and(|c| added.iter().any(|a| a.id == c))).collect();
        let lang = crate::language::of_file(rel);
        // what only Python asks: a body ty accepts in a sketch, and the `prophecy` the decorator calls
        let python = lang.as_ref().is_some_and(|l| l.name() == "python");
        let mut new_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        // what is written at a line: the body of a Python sketch that ty would refuse, and the marker
        // of an item; from the bottom up, so that a line added does not move one still to come
        enum Edit<'n> {
            Body,
            Marker(&'n Node),
        }
        let mut edits: Vec<(usize, u8, Edit)> = vec![];
        for n in top.iter().copied() {
            if let Some((start, _)) = n.span {
                edits.push((start as usize, 0, Edit::Marker(n)));
            }
        }
        if python {
            for n in added.iter().copied().filter(|n| matches!(n.kind.tag(), NodeKind::Function | NodeKind::Method)) {
                let gives = n.returns().is_some_and(|r| !r.written.is_empty() && r.written != "None");
                if let (true, Some((_, end))) = (gives, n.span) {
                    edits.push((end as usize, 1, Edit::Body));
                }
            }
        }
        edits.sort_by_key(|(line, order, _)| std::cmp::Reverse((*line, *order)));
        for (line, _, edit) in edits {
            match edit {
                Edit::Body => {
                    if let Some(rewritten) = sketch_body(&new_lines[line - 1]) {
                        new_lines.splice(line - 1..line, rewritten);
                    }
                }
                Edit::Marker(n) => {
                    let indent: String =
                        lines.get(line - 1).map(|l| l.chars().take_while(|c| c.is_whitespace()).collect()).unwrap_or_default();
                    let by = supersedes.get(&n.id).map(String::as_str);
                    let Some(form) = lang.as_ref().map(|l| l.marks()) else {
                        out.notes.push(format!("`{}` is in no language Atlas reads: write its prophecy marker by hand", rel.display()));
                        continue;
                    };
                    form.insert(&mut new_lines, line - 1, form.marker(&indent, by));
                    out.marked.push(n.id.clone());
                }
            }
        }
        let mut body = new_lines.join("\n");
        if text.ends_with('\n') {
            body.push('\n');
        }
        if python && !out.marked.is_empty() {
            let helper = after.iter().find_map(|g| {
                let h = g.nodes.iter().find(|n| n.kind == NodeKind::Function && n.name == "prophecy" && !n.library)?;
                Some((h, rel_of(g, h)))
            });
            match helper {
                Some((h, at)) => {
                    let imported = text.lines().any(|l| (l.starts_with("from ") || l.starts_with("import ")) && l.contains("prophecy"));
                    if !imported && !in_file(rel, &at) {
                        body = with_import(&body, &h.module);
                        if let Some(module) = top.first().map(|n| n.module.as_str()) {
                            let helper_text = std::fs::read_to_string(repo.join(&at)).unwrap_or_default();
                            // the package a relative import in the helper's file is taken from
                            let package = if at.file_name().is_some_and(|f| f == "__init__.py") {
                                h.module.as_str()
                            } else {
                                h.module.rsplit_once('.').map_or("", |(p, _)| p)
                            };
                            if imports(&helper_text, package, module) {
                                out.notes.push(format!(
                                    "`prophecy` is defined in `{}`, which imports `{module}`: the import of `prophecy` in `{}` goes in a circle, and the module fails to import. Keep `prophecy` in a module that imports nothing of the project",
                                    h.module,
                                    rel.display()
                                ));
                            }
                        }
                    }
                }
                None => out.notes.push(
                    "the project has no `prophecy` function: Python reads `@prophecy(..)` by its name, so a no-op decorator has to exist, and be imported in the sketched files".into(),
                ),
            }
        }
        out.files.insert(rel.clone(), body);
    }
    out.marked.sort();
    Ok(out)
}

/// The last line of a Python sketch whose body is `...`, as ty accepts it where the function gives a
/// value: ty refuses an empty body outside stubs, protocols and abstract methods, so the body raises
/// `NotImplementedError`.
fn sketch_body(line: &str) -> Option<Vec<String>> {
    let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
    let trimmed = line.trim_end();
    if trimmed.trim() == "..." {
        return Some(vec![format!("{indent}raise NotImplementedError")]);
    }
    let signature = trimmed.strip_suffix(": ...")?;
    Some(vec![format!("{signature}:"), format!("{indent}    raise NotImplementedError")])
}

/// A Python file with `from {module} import prophecy` added where isort puts it: in the block of
/// imports from the same top-level package, before the first one whose module sorts after it; with
/// no such block, as a block of its own after the last import; with no import, after the docstring.
fn with_import(text: &str, module: &str) -> String {
    let import = format!("from {module} import prophecy");
    let mut lines: Vec<&str> = text.lines().collect();
    let package = module.split('.').next().unwrap_or(module);
    // each top-level import statement: its first line, its last, and the module it names
    let mut statements: Vec<(usize, usize, &str)> = vec![];
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        if l.starts_with("import ") || l.starts_with("from ") {
            let named = l.split_whitespace().nth(1).unwrap_or("");
            let mut end = i;
            if l.contains('(') && !l.contains(')') {
                while end + 1 < lines.len() && !lines[end].contains(')') {
                    end += 1;
                }
            }
            statements.push((i, end, named));
            i = end + 1;
        } else {
            i += 1;
        }
    }
    let same = |named: &str| named.split('.').next() == Some(package);
    let block_of_its_own = !statements.is_empty() && !statements.iter().any(|(_, _, named)| same(named));
    let at = match statements.iter().find(|(_, _, named)| same(named) && *named > module) {
        Some((start, _, _)) => *start,
        None => match statements.iter().rev().find(|(_, _, named)| same(named)).or(statements.last()) {
            Some((_, end, _)) => end + 1,
            None => {
                let doc = lines.first().is_some_and(|l| l.trim_start().starts_with("\"\"\""));
                if doc {
                    lines
                        .iter()
                        .enumerate()
                        .find(|(i, l)| (*i > 0 || l.trim().len() > 3) && l.trim_end().ends_with("\"\"\""))
                        .map_or(0, |(i, _)| i + 1)
                } else {
                    0
                }
            }
        },
    };
    lines.insert(at, &import);
    if block_of_its_own {
        lines.insert(at, "");
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Whether a Python text imports module `module`: by its name, or relative to `package`, the package
/// the text's relative imports are taken from.
fn imports(text: &str, package: &str, module: &str) -> bool {
    let (parent, last) = module.rsplit_once('.').unwrap_or(("", module));
    text.lines().map(str::trim).any(|l| {
        let named =
            |m: &str| l == format!("import {m}") || l.starts_with(&format!("import {m} ")) || l.starts_with(&format!("from {m} import"));
        named(module)
            || (!parent.is_empty()
                && l.starts_with(&format!("from {parent} import"))
                && l.split("import").nth(1).is_some_and(|r| r.split(',').any(|n| n.trim().split_whitespace().next() == Some(last))))
            || (parent == package
                && (l.starts_with(&format!("from .{last} import"))
                    || (l.starts_with("from . import")
                        && l.split("import")
                            .nth(1)
                            .is_some_and(|r| r.split(',').any(|n| n.trim().split_whitespace().next() == Some(last))))))
    })
}

/// The files with the seals of `ids` broken: `sealed=..` taken out of each prophecy's marker, in
/// whichever form its language writes it, and nothing else. Each file by its path from the
/// project's root. A seal is broken when its question is answered; Atlas does it when asked.
pub fn unseal(g: &Graph, ids: &[String]) -> anyhow::Result<std::collections::BTreeMap<std::path::PathBuf, String>> {
    let seal = regex::Regex::new(r#"\s*,?\s*\bsealed\s*=\s*(?:"[^"]*"|'[^']*'|[^\s,)"'\]]+)"#)?;
    let mut files: std::collections::BTreeMap<std::path::PathBuf, Vec<String>> = Default::default();
    for id in ids {
        let Some(n) = g.node(id).filter(|n| n.prophecy().is_some_and(|p| p.sealed.is_some())) else {
            anyhow::bail!("`{id}` is no sealed prophecy");
        };
        let rel = std::path::Path::new(&n.file)
            .strip_prefix(&g.project_root)
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|_| n.file.clone().into());
        let lines = match files.entry(rel) {
            std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::btree_map::Entry::Vacant(e) => {
                let text = std::fs::read_to_string(&n.file)?;
                e.insert(text.lines().map(str::to_string).collect())
            }
        };
        let from = n.span.map_or(1, |s| s.0 as usize).saturating_sub(1);
        let marked = (from.saturating_sub(10)..=from.min(lines.len().saturating_sub(1)))
            .rev()
            .find(|i| lines[*i].contains("prophecy") && lines[*i].contains("sealed"));
        let Some(i) = marked else { anyhow::bail!("the marker of `{id}` is not near its line") };
        let broken =
            seal.replace(&lines[i], "").replace("(, ", "(").replace("prophecy: ", "prophecy:").replace("prophecy:\"", "prophecy:\"");
        lines[i] = broken.replace("prophecy:  ", "prophecy: ").trim_end().to_string();
    }
    let keep_newline = |p: &std::path::Path| std::fs::read_to_string(p).map_or(true, |t| t.ends_with('\n'));
    Ok(files
        .into_iter()
        .map(|(rel, lines)| {
            let mut text = lines.join("\n");
            if keep_newline(&std::path::Path::new(&g.project_root).join(&rel)) {
                text.push('\n');
            }
            (rel, text)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{imports, with_import};

    #[test]
    fn the_import_goes_where_isort_puts_it() {
        let text = "\"\"\"Doc.\"\"\"\n\nfrom pathlib import Path\n\nimport pandas as pd\n\nfrom sunspots.series import load\nfrom sunspots.settings import (\n    Settings,\n)\n\nX = 1\n";
        assert_eq!(
            with_import(text, "sunspots"),
            "\"\"\"Doc.\"\"\"\n\nfrom pathlib import Path\n\nimport pandas as pd\n\nfrom sunspots import prophecy\nfrom sunspots.series import load\nfrom sunspots.settings import (\n    Settings,\n)\n\nX = 1\n"
        );
        let after = with_import(text, "sunspots.zz");
        assert!(after.contains("    Settings,\n)\nfrom sunspots.zz import prophecy\n"), "{after}");
        assert_eq!(
            with_import("from pathlib import Path\n\nX = 1\n", "pkg"),
            "from pathlib import Path\n\nfrom pkg import prophecy\n\nX = 1\n"
        );
        assert_eq!(with_import("\"\"\"Doc.\"\"\"\nX = 1\n", "pkg"), "\"\"\"Doc.\"\"\"\nfrom pkg import prophecy\nX = 1\n");
    }

    #[test]
    fn a_module_that_imports_the_marked_one_is_seen() {
        assert!(imports("from sunspots.plot import Trend\n", "sunspots", "sunspots.plot"));
        assert!(imports("from sunspots import plot, series\n", "sunspots", "sunspots.series"));
        assert!(imports("from .plot import Trend\n", "sunspots", "sunspots.plot"));
        assert!(!imports("from .plot import Trend\n", "sunspots.sub", "sunspots.plot"));
        assert!(!imports("from typing import Callable\n", "sunspots", "sunspots.plot"));
        assert!(!imports("from sunspots.plotting import x\n", "sunspots", "sunspots.plot"));
    }
}
