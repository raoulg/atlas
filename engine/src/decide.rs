//! A vision decided: its files written into the tree, what it adds marked as prophecy.

use crate::model::{Graph, Node, NodeKind};
use crate::repo::Repo;
use anyhow::Context;

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
    let crate::changes::Envisioned { parts: after, dir } = crate::changes::envisioned(repo, vision, opts)?;
    let known: std::collections::HashSet<&str> = before.iter().flat_map(|g| g.nodes.iter().map(|n| n.id.as_str())).collect();
    for (new, old) in supersedes {
        if !known.contains(old.as_str()) {
            bail!("`{new}` is said to supersede `{old}`, which the code does not hold");
        }
    }
    let placed = Placed::of(repo, &before, dir);
    let mut out = Prophetized::default();
    for (rel, text) in vision {
        let Some(text) = text else { bail!("a vision that removes `{}` is no prophecy: nothing is written for a removal", rel.display()) };
        let lines: Vec<&str> = text.lines().collect();
        let tree_text = Repo::text_at(&repo.join(rel)).unwrap_or_default();
        let tree_lines: Vec<&str> = tree_text.lines().collect();
        let slice = |ls: &[&str], n: &Node| -> String {
            n.span.map(|(a, b)| ls.get(a as usize - 1..(b as usize).min(ls.len())).unwrap_or(&[]).join("\n")).unwrap_or_default()
        };
        let mine = |gs: &[Graph], at: &dyn Fn(&Node) -> Option<std::path::PathBuf>| -> Vec<Node> {
            gs.iter()
                .flat_map(|g| &g.nodes)
                .filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method | NodeKind::Class) && !n.library)
                .filter(|n| at(n).as_deref() == Some(rel.as_path()))
                .cloned()
                .collect()
        };
        let (was, now) = (mine(&before, &|n| placed.in_tree(n)), mine(&after, &|n| placed.in_vision(n)));
        for n in &was {
            match now.iter().find(|m| m.id == n.id) {
                None => bail!("the vision removes `{}` from `{}`: a vision that replaces what exists is no prophecy; give the new item a new name that `supersedes` it", n.id, rel.display()),
                Some(m) if slice(&tree_lines, n) != slice(&lines, m) => bail!(
                    "the vision changes `{}` in `{}`: writing it would replace what is built with a sketch. Give the new item a new name that `supersedes` it",
                    n.id,
                    rel.display()
                ),
                Some(_) => {}
            }
        }
        let added: Vec<&Node> = now.iter().filter(|n| !was.iter().any(|w| w.id == n.id)).collect();
        // an item inside an added class is marked with its class
        let top: Vec<&Node> =
            added.iter().copied().filter(|n| !n.cls.as_deref().is_some_and(|c| added.iter().any(|a| a.id == c))).collect();
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
            for n in added.iter().copied().filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method)) {
                let gives = n.returns.as_ref().is_some_and(|r| !r.written.is_empty() && r.written != "None");
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
            let helper = after.iter().flat_map(|g| &g.nodes).find(|n| n.kind == NodeKind::Function && n.name == "prophecy" && !n.library);
            match helper {
                Some(h) => {
                    let at = placed.in_vision(h);
                    let imported = text.lines().any(|l| (l.starts_with("from ") || l.starts_with("import ")) && l.contains("prophecy"));
                    if !imported && at.as_deref() != Some(rel.as_path()) {
                        body = with_import(&body, &h.module);
                        if let Some(module) = top.first().map(|n| n.module.as_str()) {
                            let helper_text = at.as_ref().and_then(|a| vision.get(a).cloned().flatten()).or_else(|| placed.text_in_tree(h)).unwrap_or_default();
                            // the package a relative import in the helper's file is taken from
                            let package = if std::path::Path::new(&h.file).file_name().is_some_and(|f| f == "__init__.py") {
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

/// Where the items of the tree and of the vision lie, each file by its path from the directory the
/// vision is asked for, the one its paths start at. The tree's files are where the disk resolves
/// them; the vision's are in the copy it is read in, by their path from the directory `envisioned`
/// read it from.
struct Placed {
    /// Each file of the tree, as its items name it, by its path from the asked directory.
    tree: std::collections::HashMap<String, std::path::PathBuf>,
    /// The file of each item of the tree, by its id.
    files: std::collections::HashMap<String, String>,
    /// The directory of the copy the vision is read from, in place of the asked one.
    copy: std::path::PathBuf,
}

impl Placed {
    fn of(repo: &std::path::Path, before: &[Graph], copy: std::path::PathBuf) -> Placed {
        let asked = Repo::real(repo);
        let tree_items: Vec<&Node> = before.iter().flat_map(|g| &g.nodes).filter(|n| !n.library && !n.file.is_empty()).collect();
        let mut tree = std::collections::HashMap::new();
        for n in &tree_items {
            if !tree.contains_key(&n.file) {
                if let Ok(at) = Repo::real(std::path::Path::new(&n.file)).strip_prefix(&asked) {
                    tree.insert(n.file.clone(), at.to_path_buf());
                }
            }
        }
        let files: std::collections::HashMap<String, String> = tree_items.iter().map(|n| (n.id.clone(), n.file.clone())).collect();
        Placed { tree, files, copy }
    }

    /// The path of the file that holds `n`, an item of the tree.
    fn in_tree(&self, n: &Node) -> Option<std::path::PathBuf> {
        self.tree.get(&n.file).cloned()
    }

    /// The path of the file that holds `n`, an item of the vision.
    fn in_vision(&self, n: &Node) -> Option<std::path::PathBuf> {
        std::path::Path::new(&n.file).strip_prefix(&self.copy).ok().map(std::path::Path::to_path_buf)
    }

    /// The text of the tree's file that holds the item `n` is, where the tree holds it.
    fn text_in_tree(&self, n: &Node) -> Option<String> {
        Repo::text_at(std::path::Path::new(self.files.get(&n.id)?))
    }
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

/// A seal in a marker: `sealed=` and its value, quoted as a call writes it or bare as an attribute's
/// or a JSDoc tag's text does, where it ends before a space, a comma, a quote or what closes the marker.
const SEAL: &str = r#"\bsealed\s*=\s*(?:"[^"]*"|'[^']*'|[^\s,)"'\]*]+)"#;

/// The files with the seals of `ids` broken: `sealed=..` taken out of each prophecy's marker with
/// the separator that joins it, in whichever form its language writes it, and nothing else. Each
/// file by its path from the project's root. A seal is broken when its question is answered; Atlas
/// does it when asked. The files are read now, the graph may be older: a file that no longer
/// reaches an item's line is an error.
pub fn unseal(g: &Graph, ids: &[String]) -> anyhow::Result<std::collections::BTreeMap<std::path::PathBuf, String>> {
    let seal = regex::Regex::new(SEAL)?;
    let mut files: std::collections::BTreeMap<std::path::PathBuf, String> = Default::default();
    for id in ids {
        let Some(n) = g.node(id).filter(|n| n.prophecy.as_ref().is_some_and(|p| p.sealed.is_some())) else {
            anyhow::bail!("`{id}` is no sealed prophecy");
        };
        let rel = std::path::Path::new(&n.file)
            .strip_prefix(&g.project_root)
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|_| n.file.clone().into());
        let text = match files.entry(rel) {
            std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(Repo::text_at(std::path::Path::new(&n.file)).with_context(|| format!("reading {}", n.file))?)
            }
        };
        let seal_at = {
            // each line by the byte it starts at, without its line end
            let mut start = 0;
            let lines: Vec<(usize, &str)> = text
                .split_inclusive('\n')
                .map(|l| {
                    let at = start;
                    start += l.len();
                    (at, l.trim_end_matches(['\n', '\r']))
                })
                .collect();
            let from = n.span.map_or(1, |s| s.0 as usize).saturating_sub(1);
            if from >= lines.len() {
                anyhow::bail!(
                    "`{}` has {} lines, and `{id}` was read at line {}: the file changed since it was read; read it again",
                    n.file,
                    lines.len(),
                    from + 1
                );
            }
            (from.saturating_sub(10)..=from).rev().find_map(|i| {
                let (at, line) = lines[i];
                let seal = line.contains("prophecy").then(|| seal_in(&seal, line))??;
                Some(at + seal.start..at + seal.end)
            })
        };
        let Some(seal_at) = seal_at else { anyhow::bail!("the marker of `{id}` is not near its line") };
        text.replace_range(seal_at, "");
    }
    Ok(files)
}

/// The bytes of a marker's line that hold its seal: `sealed=..` with the separator that joins it to
/// the argument after it, else to the one before it, else to what closes the marker; what is left
/// is the marker as its language writes it without a seal (`@prophecy()`, `"prophecy:"`,
/// `@prophecy */`).
fn seal_in(seal: &regex::Regex, line: &str) -> Option<std::ops::Range<usize>> {
    let m = seal.find(line)?;
    let separator = |s: &str| s.len() - s.trim_start().trim_start_matches(',').trim_start().len();
    let (before, after) = (&line[..m.start()], &line[m.end()..]);
    let next = separator(after);
    let argument_follows = next > 0 && after[next..].starts_with(|c: char| c.is_alphanumeric() || c == '_');
    let last = before.len() - before.trim_end().trim_end_matches(',').trim_end().len();
    Some(match (argument_follows, last > 0) {
        (true, _) => m.start()..m.end() + next,
        (false, true) => m.start() - last..m.end(),
        (false, false) => m.start()..m.end() + next,
    })
}

#[cfg(test)]
mod tests {
    use super::{imports, seal_in, with_import};
    use crate::prophecy::Form;

    /// A seal broken leaves the marker its form writes without one: the line before, with the bytes
    /// `seal_in` names taken out, is the line `Form::marker` writes, wherever the seal stood among the
    /// arguments and however it was spaced.
    #[test]
    fn a_broken_seal_leaves_the_marker_without_it() {
        let seal = regex::Regex::new(super::SEAL).expect("compiles");
        let by = Some("pkg.Old");
        let sealed = [
            (Form::Decorator, None, "    @prophecy(sealed=\"#7\")"),
            (Form::Decorator, None, "    @prophecy(sealed = '#7',)"),
            (Form::Decorator, by, "    @prophecy(sealed=\"#7\", supersedes=\"pkg.Old\")"),
            (Form::Decorator, by, "    @prophecy(supersedes=\"pkg.Old\", sealed=\"#7\")"),
            (Form::Attribute, None, "    #[doc = \"prophecy: sealed=#7\"]"),
            (Form::Attribute, by, "    #[doc = \"prophecy: sealed=#7 supersedes=pkg.Old\"]"),
            (Form::Attribute, by, "    #[doc = \"prophecy: supersedes=pkg.Old sealed=#7\"]"),
            (Form::JsDoc, None, "    /** @prophecy sealed=#7 */"),
            (Form::JsDoc, by, "    /** @prophecy sealed=#7 supersedes=pkg.Old */"),
            (Form::JsDoc, by, "    /** @prophecy supersedes=pkg.Old sealed=#7 */"),
        ];
        for (form, by, line) in sealed {
            let at = seal_in(&seal, line).expect("the seal is found");
            let mut broken = line.to_string();
            broken.replace_range(at, "");
            assert_eq!(broken, form.marker("    ", by), "{line}");
        }
    }

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
