//! A vision decided: its files written into the tree, what it adds marked as prophecy.

use crate::model::{Graph, Node, NodeKind};

/// A vision decided: its files as they are written into the tree, every item the vision adds marked
/// as a prophecy in the language of its file.
#[derive(Debug, Default)]
pub struct Prophesied {
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
pub fn prophesy(
    repo: &std::path::Path,
    vision: &std::collections::BTreeMap<std::path::PathBuf, Option<String>>,
    supersedes: &std::collections::BTreeMap<String, String>,
    opts: &crate::Options,
) -> anyhow::Result<Prophesied> {
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
    let mut out = Prophesied::default();
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
                        .filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method | NodeKind::Class) && !n.library)
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
        let top: Vec<&Node> =
            added.iter().copied().filter(|n| !n.cls.as_deref().is_some_and(|c| added.iter().any(|a| a.id == c))).collect();
        let ext = rel.extension().and_then(|e| e.to_str()).unwrap_or("");
        let mut new_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        let mut at: Vec<(&Node, usize)> = top.iter().filter_map(|n| Some((*n, n.span?.0 as usize))).collect();
        at.sort_by_key(|(_, l)| std::cmp::Reverse(*l));
        for (n, line) in at {
            let indent: String = lines.get(line - 1).map(|l| l.chars().take_while(|c| c.is_whitespace()).collect()).unwrap_or_default();
            let by = supersedes.get(&n.id).map(String::as_str);
            let Some(marker) = marker(ext, &indent, by) else {
                out.notes.push(format!("`{}` is in a language Atlas does not mark yet: write its prophecy marker by hand", rel.display()));
                continue;
            };
            insert_marker(ext, &mut new_lines, line - 1, marker);
            out.marked.push(n.id.clone());
        }
        let mut body = new_lines.join("\n");
        if text.ends_with('\n') {
            body.push('\n');
        }
        if ext == "py" && !out.marked.is_empty() {
            let helper = after.iter().find_map(|g| {
                let h = g.nodes.iter().find(|n| n.kind == NodeKind::Function && n.name == "prophecy" && !n.library)?;
                Some((h, rel_of(g, h)))
            });
            match helper {
                Some((h, at)) => {
                    let imported = text.lines().any(|l| (l.starts_with("from ") || l.starts_with("import ")) && l.contains("prophecy"));
                    if !imported && !in_file(rel, &at) {
                        body = with_import(&body, &format!("from {} import prophecy", h.module));
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

/// The lines that mark the item that starts at a line, at the item's indentation.
fn marker(ext: &str, indent: &str, supersedes: Option<&str>) -> Option<String> {
    let by = supersedes.map(|s| format!("supersedes={s}"));
    Some(match ext {
        "py" => format!("{indent}@prophecy{}", supersedes.map(|s| format!("(supersedes=\"{s}\")")).unwrap_or_default()),
        "rs" => format!("{indent}#[doc = \"prophecy:{}\"]", by.map(|b| format!(" {b}")).unwrap_or_default()),
        "ts" | "tsx" => format!("{indent}/** @prophecy{} */", by.map(|b| format!(" {b}")).unwrap_or_default()),
        _ => return None,
    })
}

/// Put the marker above the item at `line` (0-based). A TypeScript item that has a JSDoc right
/// above it takes the tag into that comment, which a second comment would hide.
fn insert_marker(ext: &str, lines: &mut Vec<String>, line: usize, marker: String) {
    let tag = marker.trim().trim_start_matches("/**").trim_end_matches("*/").trim().to_string();
    let indent: String = marker.chars().take_while(|c| c.is_whitespace()).collect();
    if matches!(ext, "ts" | "tsx") && line > 0 && lines[line - 1].trim_end().ends_with("*/") {
        let above = lines[line - 1].trim().to_string();
        if above.starts_with("/**") && above.ends_with("*/") && above.len() > 5 {
            let said = above.trim_start_matches("/**").trim_end_matches("*/").trim().to_string();
            lines.splice(
                line - 1..line,
                [format!("{indent}/**"), format!("{indent} * {said}"), format!("{indent} * {tag}"), format!("{indent} */")],
            );
        } else if above.starts_with("*/") || above.starts_with('*') {
            lines.insert(line - 1, format!("{indent} * {tag}"));
        } else {
            lines.insert(line, marker);
        }
        return;
    }
    lines.insert(line, marker);
}

/// A Python file with an import added before its first import, or after its docstring.
fn with_import(text: &str, import: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    let first = lines.iter().position(|l| l.starts_with("import ") || l.starts_with("from "));
    let at = first.unwrap_or_else(|| {
        let doc = lines.first().is_some_and(|l| l.trim_start().starts_with("\"\"\""));
        if !doc {
            return 0;
        }
        let end = lines
            .iter()
            .enumerate()
            .skip(0)
            .find(|(i, l)| (*i > 0 || l.trim().len() > 3) && l.trim_end().ends_with("\"\"\""))
            .map_or(0, |(i, _)| i + 1);
        end
    });
    lines.insert(at, import);
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// The files with the seals of `ids` broken: `sealed=..` taken out of each prophecy's marker, in
/// whichever form its language writes it, and nothing else. Each file by its path from the
/// project's root. A seal is broken when its question is answered; Atlas does it when asked.
pub fn unseal(g: &Graph, ids: &[String]) -> anyhow::Result<std::collections::BTreeMap<std::path::PathBuf, String>> {
    let seal = regex::Regex::new(r#"\s*,?\s*\bsealed\s*=\s*(?:"[^"]*"|'[^']*'|[^\s,)"'\]]+)"#)?;
    let mut files: std::collections::BTreeMap<std::path::PathBuf, Vec<String>> = Default::default();
    for id in ids {
        let Some(n) = g.node(id).filter(|n| n.prophecy.as_ref().is_some_and(|p| p.sealed.is_some())) else {
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
