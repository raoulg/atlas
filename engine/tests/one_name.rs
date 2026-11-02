//! A construct has one name wherever Atlas says it. `atlas changes` lists the nodes a change touches and says what a
//! changed type was by the types and functions at its arrows; it says each by the name the view draws it by. The page's
//! own model (`viewer/plan.js`, run in node on the page's data of the union) gives the view's names: `typeLabel` a
//! type's, `stepName` a function's as a step of a route. The program is a Python package of blocks whose rows are shaped
//! arrays with a dot in their shape (`{self.size}`, `...`), where a name cut at the last dot of an id is no name the view
//! draws; two of its types go by one name, `Rows`, in two modules; a method, an initializer, a function on its class and
//! a function of its module stand at their arrows; and a sum written in place, whose id is no path, changes.
//!
//! `route`'s text and `check`'s words say the same names, held on a second program, a shop: two packages, `orders` and
//! `billing`, each with a module `model` that holds a `Frame`, so that the last part of their modules does not tell them
//! apart; an initializer called by a subclass's (`super().__init__`), a method run on a subclass, a class built in place,
//! a door whose initializer does not type what it takes, and a sum written in place that two functions take unopened. In
//! its second commit a variant of that sum is renamed, so that the sum is renamed with it, and side A of a page of
//! changes names it as A's reader did.

use std::path::{Path, PathBuf};
use std::process::Command;

use atlas_engine::changes::{self, Changes};
use atlas_engine::diff::{self, Diff, Reading};
use atlas_engine::model::{self, EdgeKind, NodeKind, Shared};
use atlas_engine::{aside, flow, parts, story, Graph, Options};

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=atlas", "-c", "user.email=atlas@example.org", "-c", "commit.gpgsign=false"])
        .args(args)
        .status()
        .expect("git runs")
        .success();
    assert!(ok, "git {args:?}");
}

const PYPROJECT: &str = "[project]\nname = \"blocks\"\nversion = \"0.1.0\"\nrequires-python = \">=3.12\"\n\n\
                         [tool.ty.environment]\nextra-paths = [\"typings\"]\n";
const JAXTYPING: &str = "from typing import Annotated\n\nFloat = Annotated\n";
const TORCH: &str =
    "class Tensor:\n    def __mul__(self, other: Tensor) -> Tensor: ...\n    def __ne__(self, other: object) -> Tensor: ...\n";

const DENSE: &str = r#""""Rows of a block's size, in one batch or under any leading dimensions."""

from jaxtyping import Float
from torch import Tensor

Rows = Float[Tensor, "batch {self.size}"] | Float[Tensor, "... {self.size}"]


def ones(size: int) -> Rows:
    return Tensor()
"#;

const SPARSE: &str = r#""""Rows kept beside where they are not zero."""

from torch import Tensor

from blocks import dense


class Rows:
    def __init__(self, rows: dense.Rows, nonzero: Tensor) -> None:
        self.rows = rows
        self.nonzero = nonzero

    @classmethod
    def of(cls, rows: dense.Rows) -> "Rows":
        return cls(rows, rows != 0)

    def filled(self) -> dense.Rows:
        return self.rows
"#;

const BLOCKS: &str = r#""""Blocks that scale rows of the size each block names, and mask them where a sparse mask is not zero."""

from jaxtyping import Float
from torch import Tensor

from blocks import sparse
from blocks.dense import Rows


class Block:
    def __init__(self, size: int, mask: sparse.Rows) -> None:
        self.size = size
        self.mask = mask

    def scaled(self, x: Rows, by: Tensor) -> Rows:
        return x * by

    def masked(self, x: Float[Tensor, "batch {self.size}"] | Tensor, by: Tensor) -> Tensor:
        return x * by
"#;

/// The block with two more methods: one on its rows, so `Rows` gains arrows, and one that takes the union `masked` writes
/// in place, so that sum gains an arrow; `changes` says what each was, and what the block was.
const MORE: &str = r#"
    def doubled(self, x: Rows, by: Tensor) -> Rows:
        return self.scaled(x, by) * by

    def unmasked(self, x: Float[Tensor, "batch {self.size}"] | Tensor) -> Tensor:
        return x
"#;

/// The package `blocks` in a repository of its own under `dir`, committed once as written and once with `MORE` added.
fn repository(dir: &str) -> PathBuf {
    let repo = std::env::temp_dir().join(format!("atlas-one-name-{dir}-{}", std::process::id())).join("blocks");
    let _ = std::fs::remove_dir_all(&repo);
    for (file, text) in [
        ("pyproject.toml", PYPROJECT),
        ("typings/jaxtyping/__init__.pyi", JAXTYPING),
        ("typings/torch/__init__.pyi", TORCH),
        ("src/blocks/__init__.py", BLOCKS),
        ("src/blocks/dense.py", DENSE),
        ("src/blocks/sparse.py", SPARSE),
    ] {
        std::fs::create_dir_all(repo.join(file).parent().unwrap()).unwrap();
        std::fs::write(repo.join(file), text).unwrap();
    }
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "rows"]);
    std::fs::write(repo.join("src/blocks/__init__.py"), format!("{BLOCKS}{MORE}")).unwrap();
    git(&repo, &["commit", "-q", "-am", "more"]);
    repo
}

/// The program read at both commits: what `changes` says, and the union it is read from, with its reading A.
struct Read {
    dir: &'static str,
    c: Changes,
    report: String,
    union: Graph,
    d: Diff,
    a: Graph,
}

fn read(dir: &'static str) -> Read {
    let repo = repository(dir);
    let opts = Options::default();
    let before = changes::parts_at(&repo, Some("HEAD~1"), &opts).expect("the first commit reads");
    let after = changes::parts_at(&repo, Some("HEAD"), &opts).expect("the second commit reads");
    let c = changes::between("HEAD~1", Some("HEAD"), &before, &after);
    let (union, d) = diff::union("blocks", before, after);
    let a = diff::reading(&union, &d, Reading::A);
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    Read { dir, report: changes::report(&c), c, union, d, a }
}

/// The names the view draws the nodes `ids` by: `typeLabel` (a type) or `stepName` (a function, as a step of a route) of
/// the page's model, run in node on the page's data of `r`'s union (its graph, its flow and how its things stand to the
/// two readings).
fn drawn(r: &Read, by: &str, ids: &[String]) -> Vec<String> {
    let mut data = serde_json::to_value(&r.union).expect("the graph is data");
    data["flow"] = serde_json::to_value(flow::of(&r.union)).expect("the flow is data");
    data["diff"] = serde_json::to_value(&r.d).expect("the diff is data");
    let file = std::env::temp_dir().join(format!("atlas-one-name-{}-{}.json", r.dir, std::process::id()));
    std::fs::write(&file, data.to_string()).expect("writes the data");
    let model = r#"
        const fs = require("node:fs");
        const [planJs, dataJson, by, ...ids] = process.argv.slice(1);
        const D = JSON.parse(fs.readFileSync(dataJson, "utf8"));
        const M = new Function("D", "window", fs.readFileSync(planJs, "utf8") + "\nreturn {typeLabel, stepName, V};")(D, {});
        // a function is drawn as a step of a route; outside one, the box of an entry says its route instead
        M.V.route = ids[0];
        process.stdout.write(JSON.stringify(ids.map(id => M[by](id))));
    "#;
    let out = Command::new("node")
        .arg("-e")
        .arg(model)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../viewer/plan.js"))
        .arg(&file)
        .arg(by)
        .args(ids)
        .output()
        .expect("node runs the viewer's model: the test needs node on the PATH");
    let _ = std::fs::remove_file(&file);
    assert!(out.status.success(), "the model fails\n{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).expect("the model prints the names")
}

/// Whether `text` says `name` as one of the names it lists: after a space, and before a comma, a space or its end.
fn says(text: &str, name: &str) -> bool {
    text.match_indices(name)
        .any(|(i, _)| text[..i].ends_with(' ') && matches!(text[i + name.len()..].chars().next(), None | Some(',') | Some(' ')))
}

/// The arrows of reading A that the words of each changed type say (what takes it, gives it, builds it, changes it, holds it
/// or is held by it, its bases, its variants and its members): the words, and the id at the arrow's other end.
fn at_arrows<'r>(r: &'r Read) -> Vec<(&'r str, &'r str)> {
    let changed = r.c.routes.iter().flat_map(|x| &x.steps_changed).chain(&r.c.elsewhere.changed);
    let typed = |id: &str| r.a.node(id).map_or(false, |n| matches!(n.kind, NodeKind::Class | NodeKind::External));
    let mut said = vec![];
    for s in changed.filter(|s| typed(&s.id)) {
        for e in &r.a.edges {
            let into = e.dst == s.id
                && matches!(
                    e.kind,
                    EdgeKind::Returns
                        | EdgeKind::Instantiates
                        | EdgeKind::Takes
                        | EdgeKind::Mutates
                        | EdgeKind::Has
                        | EdgeKind::Inherits
                        | EdgeKind::Variant
                );
            let out = e.src == s.id && matches!(e.kind, EdgeKind::Has | EdgeKind::Inherits | EdgeKind::Variant | EdgeKind::Contains);
            if into || out {
                said.push((s.was.as_str(), if into { e.src.as_str() } else { e.dst.as_str() }));
            }
        }
    }
    said
}

/// Law: what `changes` says a changed type was names each type at its arrows by the name the view draws that type by.
#[test]
fn changes_names_a_type_as_the_view_draws_it() {
    let r = read("types");
    let typed = |id: &str| r.a.node(id).map_or(false, |n| matches!(n.kind, NodeKind::Class | NodeKind::External));
    let said: Vec<(&str, &str)> = at_arrows(&r).into_iter().filter(|(_, id)| typed(id)).collect();
    let ids: Vec<String> = said.iter().map(|(_, id)| id.to_string()).collect();
    let names = drawn(&r, "typeLabel", &ids);
    assert!(
        names.iter().any(|n| n.find('[').map_or(false, |i| n[i..].contains('.'))),
        "the program has a changed type at a shaped array whose shape holds a dot: {names:?}\n{}",
        r.report
    );
    let shared = |n: &str| r.union.nodes.iter().filter(|m| m.kind == NodeKind::Class && m.name == n).count() > 1;
    assert!(
        said.iter().any(|(_, id)| r.a.node(id).map_or(false, |n| shared(&n.name))),
        "the program has a changed type at a type whose name another type shares: {names:?}\n{}",
        r.report
    );
    for ((was, id), name) in said.iter().zip(&names) {
        assert!(
            says(was, name),
            "`changes` says what a type was without the name the view draws {id} by, `{name}`:\n{was}\n\n{}",
            r.report
        );
    }
}

/// Law: what `changes` says a changed type was names each function at its arrows by the name the view draws it by as a step:
/// a method with its class, an initializer as the call to its class.
#[test]
fn changes_names_a_function_at_a_types_arrows_as_the_view_draws_it() {
    let r = read("functions");
    let function = |id: &str| r.a.node(id).map_or(false, |n| matches!(n.kind, NodeKind::Function | NodeKind::Method));
    let said: Vec<(&str, &str)> = at_arrows(&r).into_iter().filter(|(_, id)| function(id)).collect();
    let ids: Vec<String> = said.iter().map(|(_, id)| id.to_string()).collect();
    let names = drawn(&r, "stepName", &ids);
    let kinds = |pick: &dyn Fn(&atlas_engine::model::Node) -> bool| said.iter().any(|(_, id)| r.a.node(id).map_or(false, |n| pick(n)));
    assert!(
        kinds(&|n| n.cls.is_some() && !n.ctor) && kinds(&|n| n.ctor && !n.on_class) && kinds(&|n| n.cls.is_none()),
        "the program has a method, an initializer and a function of its module at a changed type's arrows: {names:?}\n{}",
        r.report
    );
    for ((was, id), name) in said.iter().zip(&names) {
        assert!(
            says(was, name),
            "`changes` says what a type was without the name the view draws {id} by, `{name}`:\n{was}\n\n{}",
            r.report
        );
    }
}

/// Law: `changes` lists a node of the program's own whose id lies under no part's root (a sum written in place, whose id
/// is the set of its variants, no path) by the name the view draws it by.
#[test]
fn changes_lists_a_node_with_no_path_as_the_view_draws_it() {
    let r = read("listed");
    let own = |id: &str| r.union.node(id).map_or(false, |n| n.kind != NodeKind::External && !n.library);
    let under = |id: &str| r.c.roots.iter().any(|root| id.starts_with(&format!("{root}.")));
    let e = &r.c.elsewhere;
    let mut listed: Vec<(char, &str, bool)> = vec![];
    for x in &r.c.routes {
        listed.extend(x.steps_added.iter().map(|id| ('+', id.as_str(), true)));
        listed.extend(x.steps_removed.iter().map(|id| ('-', id.as_str(), true)));
        listed.extend(x.steps_changed.iter().map(|s| ('~', s.id.as_str(), true)));
    }
    listed.extend(e.added.iter().map(|id| ('+', id.as_str(), false)));
    listed.extend(e.removed.iter().map(|id| ('-', id.as_str(), false)));
    listed.extend(e.changed.iter().map(|s| ('~', s.id.as_str(), false)));
    let pathless: Vec<(char, &str, bool)> = listed.into_iter().filter(|(_, id, _)| own(id) && !under(id)).collect();
    assert!(!pathless.is_empty(), "the program lists a node of its own whose id lies under no root:\n{}", r.report);
    let ids: Vec<String> = pathless.iter().map(|(_, id, _)| id.to_string()).collect();
    let names = drawn(&r, "typeLabel", &ids);
    for ((sign, id, step), name) in pathless.iter().zip(&names) {
        let head = format!("    {sign} {}{name}", if *step { "step " } else { "" });
        let line = r.report.lines().any(|l| if *sign == '~' { l.starts_with(&format!("{head}: ")) } else { l == head });
        assert!(line, "`changes` lists {id} without the name the view draws it by, `{name}`:\n\n{}", r.report);
    }
}

const SHOP_PYPROJECT: &str = "[project]\nname = \"shop\"\nversion = \"0.1.0\"\nrequires-python = \">=3.12\"\n";

const SHOP: &str = r#""""A shop that keeps its orders and its bills, each as frames of its own."""

from shop.billing import model as billing
from shop.billing.model import Frame as Bill
from shop.orders import model as orders

__all__ = ["Bill", "billed", "counted", "found", "ordered", "shown", "sized"]


def ordered(lines: list[str]) -> str:
    kept, copied = orders.Kept(lines), orders.Copied(lines)
    return kept.first() + copied.first()


def billed(cents: list[int]) -> int:
    return billing.Frame(cents).total()


def found(frames: list[orders.Frame], at: int) -> str:
    return shown(orders.find(frames, at))


def counted(frames: list[orders.Frame], at: int) -> int:
    return sized(orders.find(frames, at))


def shown(frame: orders.Frame | orders.Missing) -> str:
    match frame:
        case orders.Missing():
            return ""
        case _:
            return frame.first()


def sized(frame: orders.Frame | orders.Missing) -> int:
    match frame:
        case orders.Missing():
            return 0
        case _:
            return len(frame.rows)
"#;

const ORDERS: &str = r#""""Orders, as frames of lines."""


class Frame:
    def __init__(self, rows: list[str]) -> None:
        self.rows = rows

    def first(self) -> str:
        return self.rows[0]


class Kept(Frame):
    def __init__(self, rows: list[str]) -> None:
        super().__init__(rows)


class Copied(Frame):
    def __init__(self, rows: list[str]) -> None:
        super().__init__(rows)


class Missing:
    pass


def find(frames: list[Frame], at: int) -> Frame | Missing:
    return frames[at] if at < len(frames) else Missing()
"#;

const BILLING: &str = r#""""Bills, as frames of cents."""


class Frame:
    def __init__(self, cents):
        self.cents = cents

    def total(self) -> int:
        return sum(self.cents)
"#;

/// The shop in a repository of its own under `dir`, committed once as written and once with `Missing` named `Absent`.
fn shop(dir: &str) -> PathBuf {
    let repo = std::env::temp_dir().join(format!("atlas-one-name-{dir}-{}", std::process::id())).join("shop");
    let _ = std::fs::remove_dir_all(&repo);
    for (file, text) in [
        ("pyproject.toml", SHOP_PYPROJECT),
        ("src/shop/__init__.py", SHOP),
        ("src/shop/orders/__init__.py", ""),
        ("src/shop/orders/model.py", ORDERS),
        ("src/shop/billing/__init__.py", ""),
        ("src/shop/billing/model.py", BILLING),
    ] {
        std::fs::create_dir_all(repo.join(file).parent().unwrap()).unwrap();
        std::fs::write(repo.join(file), text).unwrap();
    }
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "shop"]);
    std::fs::write(repo.join("src/shop/__init__.py"), SHOP.replace("Missing", "Absent")).unwrap();
    std::fs::write(repo.join("src/shop/orders/model.py"), ORDERS.replace("Missing", "Absent")).unwrap();
    git(&repo, &["commit", "-q", "-am", "absent"]);
    repo
}

/// What the page's model (`viewer/plan.js`) says, run in node on the page's data `data`: `body` is JavaScript that reads
/// the model as `M` (its names, and `side(s)` to draw reading `s`) and `args`, and returns what it prints as JSON.
fn on_page(data: &serde_json::Value, tag: &str, body: &str, args: &[String]) -> serde_json::Value {
    let file = std::env::temp_dir().join(format!("atlas-one-name-{tag}-{}.json", std::process::id()));
    std::fs::write(&file, data.to_string()).expect("writes the data");
    let model = format!(
        r#"
        const fs = require("node:fs");
        const [planJs, dataJson, ...args] = process.argv.slice(1);
        const D = JSON.parse(fs.readFileSync(dataJson, "utf8"));
        const M = new Function("D", "window", fs.readFileSync(planJs, "utf8") +
            "\nreturn {{typeLabel, stepName, stepLabel, plan, treeInfo, byId, V, side: s => {{ SIDE = s; }}}};")(D, {{}});
        process.stdout.write(JSON.stringify((() => {{ {body} }})()));
    "#
    );
    let out = Command::new("node")
        .arg("-e")
        .arg(model)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../viewer/plan.js"))
        .arg(&file)
        .args(args)
        .output()
        .expect("node runs the viewer's model: the test needs node on the PATH");
    let _ = std::fs::remove_file(&file);
    assert!(out.status.success(), "the model fails\n{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).expect("the model prints JSON")
}

/// The page's data of a graph: the graph and its flow.
fn page_data(g: &Graph) -> serde_json::Value {
    let mut data = serde_json::to_value(g).expect("the graph is data");
    data["flow"] = serde_json::to_value(flow::of(g)).expect("the flow is data");
    data
}

/// The arguments that ask the page for the names of `ids`: a function as a step of a route (`stepName`, `f`), a type by
/// `typeLabel` (`t`).
fn asking(g: &Graph, ids: &[String]) -> Vec<String> {
    let function = |id: &str| g.node(id).map_or(false, |n| matches!(n.kind, NodeKind::Function | NodeKind::Method));
    ids.iter().map(|id| format!("{}{id}", if function(id) { "f" } else { "t" })).collect()
}

/// The JavaScript that names each node `asking` asks for, on the reading `side` draws.
const NAMES: &str = r#"M.V.route = "·"; return args.map(a => a[0] === "f" ? M.stepName(a.slice(1)) : M.typeLabel(a.slice(1)));"#;

/// The name the page draws each node of `ids` by (`asking`).
fn page_names(g: &Graph, data: &serde_json::Value, tag: &str, ids: &[String]) -> Vec<String> {
    serde_json::from_value(on_page(data, tag, NAMES, &asking(g, ids))).expect("names")
}

/// The shop read at its first commit, as one graph and as its parts.
fn shop_read(dir: &'static str) -> (Graph, Vec<Graph>) {
    let repo = shop(dir);
    let parts = changes::parts_at(&repo, Some("HEAD~1"), &Options::default()).expect("the first commit reads");
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    (parts::joined("shop", parts.clone()), parts)
}

/// Whether `text` says `name` as a whole: at its start or after a space, a backtick or a parenthesis, and before its end,
/// a space, a comma, a backtick, a parenthesis, a question mark, or a full stop or colon that ends a sentence.
fn says_whole(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(i, _)| {
        let after = &text[i + name.len()..];
        let stop = (after.starts_with('.') || after.starts_with(':')) && after[1..].chars().next().map_or(true, char::is_whitespace);
        (i == 0 || text[..i].ends_with([' ', '`', '(']))
            && (stop || matches!(after.chars().next(), None | Some(' ' | ',' | '`' | ')' | '?')))
    })
}

/// Law (#387): a type is said by one rule in the engine and on the page (`model::type_label`, `typeLabel`), and one name
/// says one type: two types are said alike only where their whole dotted names are one. The shop has two `Frame`s that
/// stand in modules of one last part, `model`.
#[test]
fn a_shared_name_carries_as_much_of_its_module_as_tells_the_types_apart() {
    let (g, _) = shop_read("shared");
    let shared = Shared::among(&g.nodes);
    let types: Vec<&model::Node> = g.nodes.iter().filter(|n| matches!(n.kind, NodeKind::Class | NodeKind::External)).collect();
    let frames: Vec<&str> = types.iter().filter(|n| n.name == "Frame").map(|n| n.id.as_str()).collect();
    assert!(
        frames.len() == 2 && frames.iter().all(|id| id.ends_with(".model.Frame")),
        "the program has two `Frame`s in modules named `model`: {frames:?}"
    );
    let ids: Vec<String> = types.iter().map(|n| n.id.clone()).collect();
    let page = page_names(&g, &page_data(&g), "shared", &ids);
    let mut said: std::collections::BTreeMap<String, &str> = Default::default();
    let dotted = |id: &str| model::dotted(id).map_or_else(|| id.to_string(), str::to_string);
    for (n, drawn) in types.iter().zip(&page) {
        let engine = model::type_label(&n.id, |x| g.node(x), &shared);
        assert_eq!(&engine, drawn, "the engine and the page say {} apart", n.id);
        if let Some(other) = said.insert(engine.clone(), n.id.as_str()) {
            assert_eq!(dotted(other), dotted(&n.id), "`{engine}` says two types");
        }
    }
}

/// Law (#386): `route`'s text says each step it tells by the name the page draws it by at that occurrence (`stepLabel`),
/// written as the call it is: a name that is a call (`Frame()`) with what its signature takes in its parentheses, any
/// other followed by its signature or by nothing.
#[test]
fn route_says_each_step_as_the_page_draws_it() {
    let (g, _) = shop_read("route");
    let data = page_data(&g);
    let (mut compared, mut fails): (Vec<String>, Vec<String>) = (vec![], vec![]);
    for (kind, _, route) in story::entries(&g) {
        if kind == model::EntryKind::Test {
            continue;
        }
        let text = story::route(&g, route, 12).expect("an entry is a route");
        let told = story::told_steps(&g, route, 12).expect("an entry is a route");
        let ats: Vec<String> = std::iter::once(route.to_string()).chain(told.iter().map(|t| t.at.clone())).collect();
        let body = r#"M.V.route = args[0]; M.plan(); return args.slice(1).map(k => M.treeInfo.has(k) ? M.stepLabel(k) : null);"#;
        let names: Vec<Option<String>> = serde_json::from_value(on_page(&data, "route", body, &ats)).expect("names");
        for (t, name) in told.iter().zip(names).filter(|(t, _)| t.label != "the entry") {
            let Some(name) = name else { continue };
            let open = name.strip_suffix("()").map(|callee| format!("{callee}("));
            let says = |rest: &str| match &open {
                Some(open) => rest.starts_with(open.as_str()),
                None => rest.strip_prefix(name.as_str()).is_some_and(|after| after.is_empty() || after.starts_with(['(', ' '])),
            };
            if !text.lines().filter_map(|l| l.trim_start().strip_prefix(&format!("{}  ", t.label))).any(says) {
                fails.push(format!("{route} {}: the page draws `{name}`\n{text}", t.label));
            }
            compared.push(name);
        }
    }
    let shared = compared.iter().any(|n| n.ends_with("()") && n.contains(".model.Frame"));
    assert!(
        shared && compared.iter().any(|n| n == "Kept.first"),
        "the shop builds or initializes a shared class, and runs a method on a subclass: {compared:?}"
    );
    assert!(fails.is_empty(), "`route` says steps otherwise than the page:\n{}", fails.join("\n"));
}

/// Law (#386): where a finding's words name the node it is about, or a function or type it is about beside it
/// (`Finding::about`), they name it by the name the page draws it by, the parts of the repository counted together. Its
/// words name it where they say its own name as a word (`__init__` in a quote of its signature, `Missing`, `find`); a
/// finding may be about more than it names (a cycle's edges, counted).
#[test]
fn check_says_each_function_and_type_as_the_page_draws_it() {
    let (g, parts) = shop_read("check");
    let found: Vec<_> = aside::check_all(&parts).into_iter().flatten().collect();
    let named = |id: &str| {
        g.node(id).is_some_and(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method | NodeKind::Class | NodeKind::External))
    };
    let mut asked: Vec<(String, String)> = vec![];
    for f in &found {
        let words = format!("{} {} {}", f.what, f.evidence.join(" "), f.question);
        let ids = std::iter::once(f.node.as_str()).chain(f.about.iter().map(|a| a.id.as_str())).filter(|id| named(id));
        let names_it = |id: &&str| g.node(id).is_some_and(|n| says_whole(&words, &n.name) || words.contains(&format!("{}(", n.name)));
        asked.extend(ids.filter(names_it).map(|id| (id.to_string(), words.clone())));
    }
    let ids: Vec<String> = asked.iter().map(|(id, _)| id.clone()).collect();
    let names = page_names(&g, &page_data(&g), "check", &ids);
    let initializer = asked.iter().any(|(id, _)| g.node(id).is_some_and(|n| n.ctor && !n.on_class));
    assert!(
        initializer && names.iter().any(|n| n.contains(".model.Frame")),
        "the shop has a finding about an initializer and one about a shared `Frame`: {names:?}"
    );
    for ((id, words), name) in asked.iter().zip(&names) {
        assert!(says_whole(words, name), "a finding says {id} without the name the page draws it by, `{name}`:\n{words}");
    }
}

/// Law (#387): side A of a page of changes names a node B renamed as A's reader did (`model::said` on reading A), never by
/// a cut of its old id: the sum written in place `orders.Frame | orders.Missing`, renamed with its variant.
#[test]
fn side_a_names_a_renamed_node_as_a_reads_it() {
    let repo = shop("renamed");
    let opts = Options::default();
    let before = changes::parts_at(&repo, Some("HEAD~1"), &opts).expect("the first commit reads");
    let after = changes::parts_at(&repo, Some("HEAD"), &opts).expect("the second commit reads");
    let _ = std::fs::remove_dir_all(repo.parent().unwrap());
    let in_a: Vec<model::Node> = before.iter().flat_map(|g| g.nodes.clone()).collect();
    let (union, d) = diff::union("shop", before, after);
    assert!(d.renamed.values().any(|old| model::sum::is(old)), "the program renames a sum written in place: {:?}", d.renamed);
    let shared = Shared::among(&union.nodes);
    let (news, olds): (Vec<String>, Vec<&String>) = d.renamed.iter().map(|(new, old)| (new.clone(), old)).unzip();
    let mut data = page_data(&union);
    data["diff"] = serde_json::to_value(&d).expect("the diff is data");
    let body = format!(r#"M.side("a"); {NAMES}"#);
    let drawn: Vec<String> = serde_json::from_value(on_page(&data, "renamed", &body, &asking(&union, &news))).expect("names");
    for ((new, old), name) in news.iter().zip(olds).zip(drawn) {
        let a = model::said(old, |id| in_a.iter().find(|n| n.id == id), &shared);
        assert_eq!(name, a, "side A draws {new}, {old} in A, otherwise than A's reading says it");
    }
}
