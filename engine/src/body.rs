//! What a body does, read the same way in every language: its steps in the order they run, the
//! edges they make, and what its calls touch. A reader says what each call is and how its
//! language writes a string and a choice; the rest is read here.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};

use tree_sitter::Node as TsNode;

use crate::builder::GraphBuilder;
use crate::effects::{self, Facts};
use crate::model::{Crossing, Dir, EdgeKind, Held, NodeKind, Set, Site, Slot, Test, Touch, Use, World};
use crate::tree::{
    beneath, guarded_by, guards, named_children, number_choices, payload_of, starts_at_literal, text, walk, walk_all, what_of, wrapped,
    wraps, ChoiceSyntax, Part, Wrappers,
};

/// A call that crosses into the world, as the code writes it.
pub struct Crosses {
    /// What the effect tables say, refined by the literals the call is handed.
    pub facts: Facts,
    /// What it touches as the code names it: the literal it is handed, else the expression.
    pub what: String,
    /// What flows through it, as the author named it.
    pub payload: String,
    /// The code alone decides where: the path or URL starts at a literal.
    pub fixed: bool,
    pub op: String,
    pub line: u32,
}

impl Crosses {
    /// Read a call that crosses `kind` through operation `op` of `lib`: the facts its literals
    /// refine, what it touches, what flows (by the types of the names `held`), and whether its
    /// place starts at a literal. `strings` are the kinds of node a string is in the language, and
    /// `path_function` says which path function a call's function is, as the reader's evaluator
    /// of paths finds it.
    #[allow(clippy::too_many_arguments)]
    pub fn read(
        c: TsNode,
        src: &[u8],
        kind: Crossing,
        lib: Option<&str>,
        op: &str,
        literals: &[String],
        strings: &[&str],
        held: &HashMap<String, String>,
        path_function: &dyn Fn(TsNode) -> Option<crate::knowledge::PathFn>,
    ) -> Crosses {
        let facts = effects::facts(kind, lib, op, literals);
        let what = what_of(c, src, strings, "arguments");
        let payload = payload_of(c, src, &what, "arguments", held);
        let fixed = facts.kind.located() && starts_at_literal(c, src, strings, "arguments", effects::on_receiver(op), path_function);
        Crosses { facts, what, payload, fixed, op: op.to_string(), line: c.start_position().row as u32 + 1 }
    }

    /// The touches it makes, one per place it names; where the call leaves the direction open,
    /// the one the body `said` to its driver.
    pub fn touches(&self, said: Option<Dir>) -> Vec<Touch> {
        self.facts
            .places()
            .into_iter()
            .map(|(artifact, dir)| {
                // a URL without a host is the page's own origin, decided by whoever serves it
                let fixed = self.fixed && (self.facts.kind != World::Net || !artifact.is_empty());
                Touch {
                    what: self.what.clone(),
                    payload: self.payload.clone(),
                    op: self.op.clone(),
                    artifact,
                    fixed,
                    line: self.line,
                    ..Touch::new(self.facts.kind, dir.or(said))
                }
            })
            .collect()
    }
}

/// The ways a file call goes with the first two paths it is handed: one that moves or copies
/// reads the first and writes the second (`fs::copy(a, b)`, `shutil.move(src, dst)`,
/// `copyFile(a, b)`); any other goes its own way with each.
pub fn sides(op: &str, paths: usize) -> Option<[Crossing; 2]> {
    (paths > 1 && matches!(effects::fs_op(op), effects::FsOp::Moves | effects::FsOp::Copies))
        .then_some([Crossing::read(World::Disk), Crossing::write(World::Disk)])
}

/// How a language writes where a value goes once an expression gives it: the names it binds, the
/// ways out of a function, the ways to an error, and what a call is handed.
pub struct UseSyntax {
    /// The names a node binds and the value it binds them to: `x = v`, `let x = v`, `for x in v`.
    pub binds: fn(TsNode) -> Option<(TsNode, TsNode)>,
    /// Does what this node holds leave the function `body`: a `return`, a `yield`, the last
    /// expression of a block that gives its value.
    pub leaves: fn(TsNode, TsNode) -> bool,
    /// Does what this node holds go to an error: `raise`, `throw`, `assert`, `bail!`.
    pub fails: fn(TsNode, &[u8]) -> bool,
    /// Is `child` what the call `node` is handed: its arguments.
    pub hands: fn(TsNode, TsNode) -> bool,
    /// The names a call `node` changes among what it is handed, where the language writes it
    /// (Rust's `&mut s`): the call makes them anew from what it runs on.
    pub changes: fn(TsNode) -> Vec<TsNode>,
    /// Is `child` what `node` takes apart into the names its arms bind: a `match`'s subject.
    /// Where those names go is not followed, so the value is taken as going on.
    pub splits: fn(TsNode, TsNode) -> bool,
    /// The kinds of node a name is.
    pub names: &'static [&'static str],
    /// The kinds of target that keep a value beyond a name: a field, an element (`self.n = v`,
    /// `rows[i] = v`).
    pub keeps: &'static [&'static str],
    /// The value a field of it is read or set through and the field's name, as the code writes it:
    /// `x` and `f` of `x.f`; none for an element (`rows[i]`).
    pub field: for<'n> fn(TsNode<'n>, &[u8]) -> Option<(TsNode<'n>, String)>,
    /// Are a binding's targets and its value written as as many parts each, so that each target is
    /// bound to the part of the value at its place: `a.x, a.y = b, c`, `(a.x, a.y) = (b, c)`.
    pub pairs: fn(TsNode, TsNode) -> bool,
    /// Does the language's own code compute what `node` gives of its parts, with no call: an operator, a display of several
    /// values, a format (`a - b`, `!ok`, `[a, b]`, `(a, b)`, `f"{a}"`, `` `${a}` ``). A choice is none (`a if c else b`, `x ??
    /// y`, `a && b`): its arms are the body's choices (`computed_of`).
    pub computes: fn(TsNode, &[u8]) -> bool,
    /// What hands on the value inside it (`tree::Wrappers`): a call written in one is that call to
    /// what it stands in (`await f(x)`, `f(x)?`).
    pub wrappers: Wrappers,
    /// Does `node` throw away what `child` gives, `child` being all it is made of: a statement that is
    /// one expression (`f(x)`, `f(x);`), `void f(x)`, `let _ = f(x)`.
    pub drops: fn(TsNode, TsNode, &[u8]) -> bool,
    /// The kinds of node a body defines that are bodies of their own (a function, a class): what is
    /// written there is theirs.
    pub nested: &'static [&'static str],
    /// The kinds of node besides a name that read a value: the receiver a method runs on, where the
    /// language writes it as no name (`self`, `this`), and a string that writes names into its text
    /// (`"{total}"`).
    pub reads: &'static [&'static str],
    /// Is the name `n` a label where it stands, no value read: the attribute of `x.f`, the keyword of
    /// `f(k=v)`, a part of a path (`Error::Empty`).
    pub label: fn(TsNode, &[u8]) -> bool,
    /// Whether `child` is the part of `node` that binds names, and whether those are its own: a
    /// function written in place binds its parameters (`lambda x: ..`, `|x| ..`, `x => ..`), which hold
    /// no value of the body around it; a pattern binds the parts of a value the body takes apart (a
    /// `case`, a `match` arm, a `catch`), which are the body's.
    pub binders: fn(TsNode, TsNode) -> Option<bool>,
    /// The kinds of binding that read what they bind as well: `x += v`.
    pub updates: &'static [&'static str],
    /// Is `child` what the names `node` binds are parts of, beyond what `binds` binds: what a
    /// comprehension's `for` goes over.
    pub parts: fn(TsNode, TsNode) -> bool,
    /// Does `node` end the body with what `child` gives as an error: a `raise`, a `throw`.
    pub raises: fn(TsNode, TsNode, &[u8]) -> bool,
    /// Does `node` hold what `child` gives as an error, which ends the body where `node` leaves it or is
    /// handed on (`Err(child)`)?
    pub errs: fn(TsNode, TsNode, &[u8]) -> bool,
    /// The kinds of node whose value is that of their last part, as far as an error held in it leaves
    /// the body with it: a block, an `else`, a `match` arm.
    pub tails: &'static [&'static str],
    /// The kinds of node that hand on the error what they hold fails with (`x?`); none where every
    /// call hands on what its callee raises, as a `raise` propagates.
    pub propagates: Option<&'static [&'static str]>,
    /// Where `child` is the part of `node` its handlers watch (the body of a `try`): the classes they
    /// catch as the code names them, none where one catches whatever is raised.
    pub catches: fn(TsNode, TsNode, &[u8]) -> Option<Vec<String>>,
    /// The arguments call `node` writes, each with the parameter it fills as the call writes it
    /// (`Slot`): the value it is called on, each argument by its place up to a spread, each by its name.
    pub args: fn(TsNode, &[u8]) -> Vec<(Range, Slot)>,
}

/// Where code is written, as a byte range of its file.
pub type Range = (usize, usize);

/// Where the value `h` comes from: the name a binding's projection of it starts from, where the reader followed one
/// (`Held::bound`), else where the code writes it.
fn origin(h: &Held) -> u32 {
    if h.bound != 0 {
        h.bound
    } else {
        h.at
    }
}

/// The arguments a list of them writes by place and by name: each by its place until a spread (`*xs`,
/// `...xs`), which hands the places from its own on, as does every argument by place after it.
/// `spread` and `named` say which an argument is: a spread of places (`Slot::Spread`) or of names
/// (`Slot::Keywords`), and the name a keyword argument writes with the value it hands.
pub fn placed<'t>(
    args: impl IntoIterator<Item = TsNode<'t>>,
    spread: impl Fn(TsNode<'t>) -> Option<Slot>,
    named: impl Fn(TsNode<'t>) -> Option<(String, TsNode<'t>)>,
) -> Vec<(Range, Slot)> {
    let range = |n: TsNode| (n.start_byte(), n.end_byte());
    let (mut out, mut at, mut from) = (vec![], 0u32, None);
    for a in args {
        if let Some((name, value)) = named(a) {
            out.push((range(value), Slot::Name(name)));
            continue;
        }
        match spread(a) {
            Some(Slot::Keywords) => out.push((range(a), Slot::Keywords)),
            Some(_) => out.push((range(a), Slot::Spread(*from.get_or_insert(at)))),
            None => {
                out.push((range(a), from.map_or(Slot::At(at), Slot::Spread)));
                at += 1;
            }
        }
    }
    out
}

/// The names the value `v` is computed of by the language's own code (`UseSyntax::computes`): through its operators,
/// displays and formats down to the parts that are none, each of which is what `part` says, the reader's: the names it is a
/// projection of (`a` of `a.x[k]`), none for a literal, `None` for a part made otherwise (a call, a function written in
/// place, a choice), and then for the whole. An operator is no occurrence: it is the language's operation
/// handed its parts, glued into what takes its value as a projection is (`mathematics.md`, "One fine plan"), so a step
/// handed `height - margin` is handed what `height` and `margin` hold.
pub fn computed_of<'t>(
    v: TsNode<'t>,
    u: &UseSyntax,
    src: &[u8],
    part: &dyn Fn(TsNode<'t>) -> Option<Vec<TsNode<'t>>>,
) -> Option<Vec<TsNode<'t>>> {
    let v = beneath(v, u.wrappers);
    if !(u.computes)(v, src) {
        return part(v);
    }
    let mut out = vec![];
    for c in named_children(v) {
        out.extend(computed_of(c, u, src, part)?);
    }
    Some(out)
}

/// The names a name bound to `v` stands for: those `v` is computed of, where it is a computation of at least one name and
/// no part of it is made otherwise (`computed_of`). Such a name is the body's own (`Node::binds`) but holds no value of its
/// own: `bottom = height - margin; f(bottom)` is `f(height - margin)`, and own code that reads `bottom` reads `height` and
/// `margin`, in every language. None for any other value: a name, a projection, a literal (a value the body makes itself), a
/// call's result.
pub fn stands_for<'t>(v: TsNode<'t>, u: &UseSyntax, src: &[u8], part: &dyn Fn(TsNode<'t>) -> Option<Vec<TsNode<'t>>>) -> Vec<TsNode<'t>> {
    let v = beneath(v, u.wrappers);
    if !(u.computes)(v, src) {
        return vec![];
    }
    computed_of(v, u, src, part).unwrap_or_default()
}

/// One call as a step of the route through its body: what it runs, on which class and how the
/// call finds it (`Site::named`, `Site::after`), what it is handed and what it gives back, and the
/// options of the command line it parses when it parses the one the program is run with.
#[derive(Default)]
pub struct Step {
    pub on: String,
    /// The value the call runs on is built at the call (`Site::built`).
    pub built: bool,
    pub on_receiver: bool,
    pub named: bool,
    /// The call is made through its method's class or path, and the first argument by place is the
    /// value the method runs on (`Cached.read(self)`, `Trend::save(&t, ..)`, `C.prototype.f.call(v)`).
    pub receiver_first: bool,
    pub after: String,
    pub hands: Vec<Held>,
    pub gives: Vec<Held>,
    pub command_line: Option<Vec<String>>,
    /// Where the call is written from, where that lies before the node it is read at: among a macro's tokens a method's
    /// call is the group it is handed, written from its receiver on (`Plain.name()`), as a call written outside one is.
    pub from: Option<usize>,
}

/// What a call gives: the values it returns, and the named values it changes in place
/// (`rows.append(row)` gives `rows`).
fn changed_values(mut gives: Vec<Held>, changed: Vec<Held>) -> Vec<Held> {
    for value in changed.into_iter().filter(|h| !h.name.is_empty()) {
        if !gives.contains(&value) {
            gives.push(value);
        }
    }
    gives
}

/// The steps of a body in the order they run, the names they keep, and the edges they make: read
/// the same way for every language. The reader says what each call is; where it sits among the
/// body's choices is read here.
pub struct Steps<'t, 's> {
    body: TsNode<'t>,
    src: &'s [u8],
    syntax: &'s ChoiceSyntax,
    uses: &'s UseSyntax,
    choices: HashMap<usize, u32>,
    sites: Vec<Site>,
    edges: Vec<(String, EdgeKind, Option<String>, Vec<(u32, u32)>)>,
    unseen: Vec<(u32, String)>,
    own: Vec<Use>,
    /// The steps whose result the body's own code reads, by no name (`Steps::read`).
    read: Vec<usize>,
    /// The error the body's own code hands on outside its steps (`Node::fails`).
    fails: Option<String>,
    /// What the body's own code sets a part of a value to (`Node::sets`).
    sets: Vec<Set>,
    /// The fields the conditions of the body's choices read (`Node::tests`).
    tests: Vec<Test>,
    /// The error type what hands on an error converts it to (`?`), where the function the body is of
    /// says it fails.
    converts: Option<String>,
    /// The names the steps so far have kept.
    pub kept: HashSet<String>,
    /// The values the body binds a name to by a literal (`Steps::literal`), by where each is written.
    literals: HashSet<Range>,
    /// The body read once, when a question first climbs it or follows a name through it.
    index: OnceCell<Index<'t, 's>>,
}

/// A body read once: the node each of its nodes stands in, the named nodes each holds, and the names written in it in
/// source order, also by what each writes. tree-sitter finds a node's parent, and so its siblings, by walking down from
/// the root of its tree, and a body's questions climb from a call or a name up to the body, look at the statements
/// before each node on the way, and follow a name to every later place it is written.
struct Index<'t, 's> {
    /// Of each node under the body, the node it stands in and how many of that node's named children come before it.
    parents: HashMap<usize, (TsNode<'t>, usize)>,
    /// The named children of each node of the body that has any, in source order.
    named: HashMap<usize, Vec<TsNode<'t>>>,
    names: Vec<TsNode<'t>>,
    written: HashMap<&'s str, Vec<TsNode<'t>>>,
}

impl<'t, 's> Index<'t, 's> {
    fn new(body: TsNode<'t>, src: &'s [u8], names: &[&str]) -> Self {
        let (mut parents, mut named) = (HashMap::new(), HashMap::new());
        let (mut cursor, mut stack) = (body.walk(), vec![body]);
        while let Some(n) = stack.pop() {
            let mut own: Vec<TsNode<'t>> = vec![];
            for c in n.children(&mut cursor) {
                parents.insert(c.id(), (n, own.len()));
                if c.is_named() {
                    own.push(c);
                }
                stack.push(c);
            }
            if !own.is_empty() {
                named.insert(n.id(), own);
            }
        }
        let names = walk(body, names);
        let mut written: HashMap<&'s str, Vec<TsNode<'t>>> = HashMap::new();
        for n in &names {
            written.entry(text(*n, src)).or_default().push(*n);
        }
        Index { parents, named, names, written }
    }
}

impl<'t, 's> Steps<'t, 's> {
    pub fn new(body: TsNode<'t>, src: &'s [u8], syntax: &'s ChoiceSyntax, uses: &'s UseSyntax) -> Self {
        let choices = number_choices(body, src, syntax);
        Steps {
            body,
            src,
            syntax,
            uses,
            choices,
            sites: vec![],
            edges: vec![],
            unseen: vec![],
            own: vec![],
            read: vec![],
            fails: None,
            sets: vec![],
            tests: vec![],
            converts: None,
            kept: HashSet::new(),
            literals: HashSet::new(),
            index: OnceCell::new(),
        }
    }

    fn index(&self) -> &Index<'t, 's> {
        self.index.get_or_init(|| Index::new(self.body, self.src, self.uses.names))
    }

    /// The node `n` stands in.
    fn up(&self, n: TsNode<'t>) -> Option<TsNode<'t>> {
        match self.index().parents.get(&n.id()) {
            Some((p, _)) => Some(*p),
            None => n.parent(),
        }
    }

    /// The named nodes before `n` in the node it stands in, the nearest first.
    fn before(&self, n: TsNode<'t>) -> Vec<TsNode<'t>> {
        let index = self.index();
        match index.parents.get(&n.id()) {
            Some((p, at)) => index.named.get(&p.id()).map_or(vec![], |all| all[..*at].iter().rev().copied().collect()),
            None => std::iter::successors(n.prev_named_sibling(), |s| s.prev_named_sibling()).collect(),
        }
    }

    /// The outermost wrapper `n` stands in, or `n` itself (`tree::outermost`).
    fn outermost(&self, n: TsNode<'t>) -> TsNode<'t> {
        let wrappers = self.uses.wrappers;
        let mut n = n;
        while let Some(p) = self.up(n).filter(|p| wraps(wrappers, *p) && wrapped(*p).map(|x| x.id()) == Some(n.id())) {
            n = p;
        }
        n
    }

    /// The names written in the body after byte `after` that write `name`, in source order.
    fn written_after<'a>(&'a self, name: &'a str, after: usize) -> impl Iterator<Item = TsNode<'t>> + 'a {
        self.index().written.get(name).into_iter().flatten().copied().filter(move |n| n.start_byte() >= after)
    }

    /// The error type what hands on an error converts it to (`?` by `From`), where the signature of the
    /// function the body is of says it fails: the program's type it names, `""` for one that is no type
    /// of the code's (`io::Error`, `anyhow::Error`). A call it is wrapped around fails with that type.
    pub fn converts(&mut self, error: Option<String>) {
        self.converts = error;
    }

    /// A site of call `c`, as the body's statements stand to it: whether the body throws away what
    /// it gives, whether that is the error the body ends with, and what hands on or catches the error
    /// it fails with.
    fn site(&self, c: TsNode<'t>, site: Site) -> Site {
        let (handed_on, explicit, raises) = (self.handed_on(c), self.uses.propagates.is_some(), self.raised(c));
        // what the body gives back as a call gives it, the call's error among it, the body fails with
        let fails = self.converts.clone().filter(|_| explicit && (handed_on || !raises && self.gives_back(c))).into_iter().collect();
        // the error a constructor builds to raise is a value of its class, whether or not a name holds it
        let mut gives = site.gives;
        if raises && gives.is_empty() && !site.to.is_empty() {
            gives.push(Held::new(site.to.clone(), ""));
        }
        let in_function = self.in_function(c);
        Site {
            dropped: self.drops(c),
            raises,
            handed_on: handed_on && !explicit,
            fails,
            caught: self.caught(c),
            gives,
            in_function,
            ..site
        }
    }

    /// Whether the body throws away what call `c` gives: through what hands its value on unchanged,
    /// the call is all a statement is made of (`other.take_from(board)`, `store.add(x)?;`), or what
    /// discards a value is handed it (`void remove(..)`, `let _ = f(x)`).
    fn drops(&self, c: TsNode<'t>) -> bool {
        let v = self.outermost(c);
        self.up(v).is_some_and(|p| (self.uses.drops)(p, v, self.src))
    }

    /// Whether what call `c` gives ends the body as an error: a `raise` or a `throw` raises it, or what
    /// holds it as an error (`Err(c)`) leaves the body, as what it returns or its last value, or is
    /// handed on by `?`.
    pub fn raised(&self, c: TsNode<'t>) -> bool {
        let u = self.uses;
        let v = self.outermost(c);
        let Some(p) = self.up(v) else { return false };
        if (u.raises)(p, v, self.src) {
            return true;
        }
        let Some(holder) = self.up(p).filter(|_| (u.errs)(p, v, self.src) && !self.in_function(p)) else { return false };
        let mut at = holder;
        loop {
            // what the body's block ends with is what the function gives
            if (u.leaves)(at, self.body) || at.id() == self.body.id() {
                return true;
            }
            let Some(up) = self.up(at) else { return false };
            if u.propagates.is_some_and(|p| p.contains(&up.kind())) || (u.leaves)(up, self.body) {
                return true;
            }
            let last = up.named_child(up.named_child_count().saturating_sub(1)).map(|l| l.id()) == Some(at.id());
            let through = (wraps(u.wrappers, up) && wrapped(up).map(|w| w.id()) == Some(at.id()))
                || (u.tails.contains(&up.kind()) && last && !(u.drops)(up, at, self.src))
                || matches!((self.syntax.part)(up, at, self.src), Part::Arm(_));
            if !through {
                return false;
            }
            at = up;
        }
    }

    /// Whether what call `c` gives is what the body gives back as it is: what the body ends with or
    /// returns, through what hands the value on unchanged and the blocks and arms whose value is their
    /// last part (`self.write(..)` last in a body that gives what `write` gives).
    fn gives_back(&self, c: TsNode<'t>) -> bool {
        let u = self.uses;
        if self.in_function(c) {
            return false;
        }
        let mut at = c;
        loop {
            // what the body's block ends with is what the function gives (`match v { .. }` last, without `;`)
            if (u.leaves)(at, self.body) || at.id() == self.body.id() {
                return true;
            }
            let Some(up) = self.up(at).filter(|_| at.id() != self.body.id()) else { return false };
            let last = named_children(up).into_iter().filter(|x| !x.kind().ends_with("comment")).last().is_some_and(|l| l.id() == at.id());
            let through = (wraps(u.wrappers, up) && wrapped(up).is_some_and(|w| w.id() == at.id()))
                || (u.tails.contains(&up.kind()) && last && !(u.drops)(up, at, self.src))
                || matches!((self.syntax.part)(up, at, self.src), Part::Arm(_));
            if !through && !(u.leaves)(up, self.body) {
                return false;
            }
            at = up;
        }
    }

    /// The error a `?` written in a call that is no step hands on is handed on by the step it is glued
    /// into, the smallest written around it (`Graaf { nodes: fs::read_to_string(p)?.lines()..}`): the
    /// error the function's signature converts it to. Written in no step, the body's own code hands it
    /// on (`Node::fails`).
    fn glued_fails(&mut self) {
        let (u, Some(error)) = (self.uses, self.converts.clone()) else { return };
        let Some(kinds) = u.propagates else { return };
        let tries: Vec<TsNode> = walk(self.body, kinds).into_iter().filter(|t| !self.in_function(*t)).collect();
        for t in tries {
            let Some(inner) = wrapped(t) else { continue };
            let (a, b) = (inner.start_byte() as u32, inner.end_byte() as u32);
            if self.sites.iter().any(|s| s.span == (a, b)) {
                continue;
            }
            let around =
                self.sites.iter_mut().filter(|s| s.span != (0, 0) && s.span.0 <= a && b <= s.span.1).min_by_key(|s| s.span.1 - s.span.0);
            match around {
                Some(s) if s.fails.is_empty() && !s.raises => s.fails = vec![error.clone()],
                Some(_) => {}
                None => self.fails = Some(error.clone()),
            }
        }
    }

    /// Whether call `c` hands on the error it fails with: what hands it on is wrapped around the call
    /// (`f(x)?`), or the language hands on every error a call fails with.
    fn handed_on(&self, c: TsNode<'t>) -> bool {
        let u = self.uses;
        let Some(propagates) = u.propagates else { return true };
        let mut at = c;
        while let Some(p) = self.up(at).filter(|p| wraps(u.wrappers, *p) && wrapped(*p).map(|w| w.id()) == Some(at.id())) {
            if propagates.contains(&p.kind()) {
                return !self.in_function(p);
            }
            at = p;
        }
        false
    }

    /// Whether a binding decides a choice by what it binds: `if let Some(x) = v`, `let Some(x) = v
    /// else { .. }`. What it takes apart is read there, as a `match` reads its subject.
    fn decides(&self, binding: TsNode<'t>) -> bool {
        if (self.syntax.chooses)(binding, self.src) {
            return true;
        }
        let mut at = binding;
        for _ in 0..2 {
            let Some(p) = self.up(at) else { return false };
            if (self.syntax.part)(p, at, self.src) == Part::Decides {
                return true;
            }
            at = p;
        }
        false
    }

    /// Whether `n` stands in a function written in place in the body (`|x| ..`, `x => ..`): what
    /// it returns or hands on by `?` leaves that function, not the body.
    fn in_function(&self, n: TsNode<'t>) -> bool {
        self.function_around(n).is_some()
    }

    /// The handlers that watch call `c`, innermost first, each by the classes it catches.
    fn caught(&self, c: TsNode<'t>) -> Vec<Vec<String>> {
        let mut out = vec![];
        let mut child = c;
        while let Some(p) = self.up(child).filter(|_| child.id() != self.body.id()) {
            out.extend((self.uses.catches)(p, child, self.src));
            child = p;
        }
        out
    }

    /// What the body's own code uses outside its steps (`Node::uses`): each value a name holds where
    /// the body reads it itself, in a condition, an operator, a format, what it returns or what it hands
    /// a call that is no step (`print(store.path)`, `if done:`, `Ok(path)`, `f.write(..)`). What a step
    /// is handed is the step's. `held` is the reader's: the values a name, or a string that writes
    /// names, holds where it stands, as a call written there would be handed them; none for one that
    /// holds no value (a function, a module, a class). Each value once in each arm it is used in.
    pub fn uses(&mut self, held: &dyn Fn(TsNode<'t>) -> Vec<Held>) {
        let u = self.uses;
        let steps: Vec<(usize, usize)> =
            self.sites.iter().filter(|s| s.span != (0, 0)).map(|s| (s.span.0 as usize, s.span.1 as usize)).collect();
        let in_step = |n: TsNode| steps.iter().any(|(a, b)| *a <= n.start_byte() && n.end_byte() <= *b);
        let mut out: Vec<Use> = vec![];
        let mut stack = vec![self.body];
        while let Some(n) = stack.pop() {
            if (n.id() != self.body.id() && u.nested.contains(&n.kind())) || in_step(n) {
                continue;
            }
            if (u.names.contains(&n.kind()) || u.reads.contains(&n.kind())) && self.read_here(n) {
                let arms = self.arms(n);
                for h in held(n).into_iter().filter(|h| !h.ty.is_empty() || !h.name.is_empty()) {
                    let one = Use { name: h.name, ty: h.ty, arms: arms.clone(), param: h.param };
                    if !out.contains(&one) {
                        out.push(one);
                    }
                }
            }
            stack.extend(named_children(n).into_iter().rev());
        }
        // a name a step gave holds what the step gave, where the reader knows no type for it where it is read, or one of a
        // library's: the value is the step's, of no type of the code's where the step gives none (`fig, ax = plt.subplots()`)
        for u in out.iter_mut().filter(|u| (u.ty.is_empty() || u.ty.starts_with("ext:")) && !u.name.is_empty()) {
            let types: HashSet<&String> = self.sites.iter().flat_map(|s| &s.gives).filter(|g| g.name == u.name).map(|g| &g.ty).collect();
            let types: Vec<&String> = types.into_iter().collect();
            match types[..] {
                [t] if !t.is_empty() || u.ty.starts_with("ext:") => u.ty = t.clone(),
                _ => {
                    if let [t] = types.iter().filter(|t| !t.is_empty()).collect::<Vec<_>>()[..] {
                        if u.ty.is_empty() {
                            u.ty = (*t).clone();
                        }
                    }
                }
            }
        }
        self.read = (0..self.sites.len()).filter(|&i| self.read_by_own_code(i)).collect();
        self.literals = self.literals(held);
        self.sets = self.sets(held);
        self.tests = self.tests(held);
        let mut once: Vec<Use> = vec![];
        for u in out {
            if !once.contains(&u) {
                once.push(u);
            }
        }
        self.own = once;
    }

    /// The values the body binds a name to by a literal, outside its choices (`Node::binds`): a binding of names, no change of a
    /// value, no update, no condition, whose value is made by no step and reads no value: no name, no instance it runs on, no
    /// string that writes a name (`let mut held = vec![]`, `items = []`, `const xs: Item[] = []`), also in a function written in
    /// place, whose steps are the body's. Such a value the body makes itself, as a call handed nothing makes it (`Vec::new()`,
    /// `list()`). `held` is the reader's, as for `uses`.
    fn literals(&self, held: &dyn Fn(TsNode<'t>) -> Vec<Held>) -> HashSet<Range> {
        let u = self.uses;
        let steps: Vec<Range> = self.sites.iter().filter(|s| s.span != (0, 0)).map(|s| (s.span.0 as usize, s.span.1 as usize)).collect();
        let mut out = HashSet::new();
        let mut stack = vec![self.body];
        while let Some(b) = stack.pop() {
            if b.id() != self.body.id() && u.nested.contains(&b.kind()) {
                continue;
            }
            stack.extend(named_children(b).into_iter().rev());
            let Some((targets, value)) = (u.binds)(b) else { continue };
            if b.child_by_field_name("body").is_some()
                || !walk(targets, u.keeps).is_empty()
                || u.updates.contains(&b.kind())
                || self.decides(b)
            {
                continue;
            }
            let v = beneath(value, u.wrappers);
            if !self.arms(v).is_empty() || self.choices.contains_key(&v.id()) {
                continue;
            }
            let made = steps.iter().any(|(a, z)| v.start_byte() <= *a && *z <= v.end_byte());
            let reads =
                walk(v, u.names).into_iter().any(|n| !(u.label)(n, self.src)) || walk(v, u.reads).into_iter().any(|r| !held(r).is_empty());
            if !made && !reads {
                out.insert((v.start_byte(), v.end_byte()));
            }
        }
        out
    }

    /// What the body's own code sets a part of a value to (`Node::sets`): each binding whose target is a
    /// field or an element of a value a name holds (`other.fig = self.fig`, `rows[i] = row`, `self.n +=
    /// 1`), once for each part it sets: the value it changes, as a call is handed it, and the field of it
    /// that is set, with the values the assigned expression reads, outside the steps written in it, and
    /// those steps' results that a name holds, and the field of a value the expression is. A binding of a
    /// name is no change of a value (`Node::binds`).
    fn sets(&self, held: &dyn Fn(TsNode<'t>) -> Vec<Held>) -> Vec<Set> {
        let u = self.uses;
        let kinds: Vec<&str> = u.names.iter().chain(u.reads).copied().collect();
        let mut out: Vec<Set> = vec![];
        let mut stack = vec![self.body];
        while let Some(b) = stack.pop() {
            if b.id() != self.body.id() && u.nested.contains(&b.kind()) {
                continue;
            }
            stack.extend(named_children(b).into_iter().rev());
            let Some((targets, value)) = (u.binds)(b) else { continue };
            let kept = walk(targets, u.keeps);
            if kept.is_empty() || b.child_by_field_name("body").is_some() || self.in_function(b) {
                continue;
            }
            // each part a binding sets, with the part of the value at its place where both are written as as many parts
            // (`other.fig, other.ax = self.fig, self.ax`), else with the whole value (`self.fig, self.ax = plt.subplots()`)
            let each = |n: TsNode<'t>| named_children(n).into_iter().filter(|x| !x.kind().ends_with("comment")).collect::<Vec<_>>();
            let parts: Vec<(TsNode, TsNode)> = if (u.pairs)(targets, value) && each(targets).len() == each(value).len() {
                each(targets).into_iter().zip(each(value)).collect()
            } else {
                let within = |a: &TsNode, b: &TsNode| a.id() != b.id() && b.start_byte() <= a.start_byte() && a.end_byte() <= b.end_byte();
                kept.iter().filter(|k| !kept.iter().any(|o| within(k, o))).map(|k| (*k, value)).collect()
            };
            let read = |n: &TsNode| !(u.label)(*n, self.src);
            for (target, value) in parts {
                if walk(target, u.keeps).is_empty() {
                    continue;
                }
                let Some(owner) = walk(target, &kinds).into_iter().filter(read).min_by_key(|n| n.start_byte()) else { continue };
                let Some(on) = held(owner).into_iter().next() else { continue };
                let steps: Vec<&Site> = self
                    .sites
                    .iter()
                    .filter(|s| s.span != (0, 0) && value.start_byte() <= s.span.0 as usize && s.span.1 as usize <= value.end_byte())
                    .collect();
                let in_step = |n: &TsNode| steps.iter().any(|s| s.span.0 as usize <= n.start_byte() && n.end_byte() <= s.span.1 as usize);
                let mut from: Vec<Held> = vec![];
                let named: Vec<(TsNode, Held)> = walk(value, &kinds)
                    .into_iter()
                    .filter(|n| read(n) && !in_step(n))
                    .flat_map(|n| held(n).into_iter().map(move |h| (n, h)))
                    .collect();
                let made = steps.iter().flat_map(|s| s.gives.iter().filter(|g| !g.name.is_empty()).cloned());
                for h in named.iter().map(|(_, h)| h.clone()).chain(made).filter(|h| !h.ty.is_empty() || !h.name.is_empty()) {
                    if !from.contains(&h) {
                        from.push(Held { at: 0, fills: vec![], through: vec![], ..h });
                    }
                }
                // a projection bound to a name first is still the projection (#121): `f = self.fig; other.fig = f` sets the
                // field to `fig` of the instance, and `c = self.left; c.fig = ..` sets `left.fig` of it
                let of = match from.as_slice() {
                    [one] => named.iter().filter(|(_, h)| h == one).find_map(|(n, h)| self.read_of(value, *n, h, held)).unwrap_or_default(),
                    _ => String::new(),
                };
                let field = self.read_of(target, owner, &on, held).or_else(|| self.fields(target)).unwrap_or_default();
                out.push(Set {
                    on: Held { at: 0, fills: vec![], through: vec![], ..on },
                    field,
                    from,
                    of,
                    lines: Some((line(b), b.end_position().row as u32 + 1)),
                    arms: self.arms(b),
                    value: (value.start_byte() as u32, value.end_byte() as u32),
                });
            }
        }
        out
    }

    /// What the conditions that decide the body's choices read of the values names hold (`Node::tests`): for each
    /// choice, each value a name in a condition holds and the field the condition reads of it, through the fields it is
    /// reached by and the projection the name is bound to first (`read_of`): `ax` of `self.ax is None`, `left.ax` of
    /// `self.left.ax is None`. What a method is called on is read up to the method (`self.ax.is_none()` reads `ax`); a
    /// value read whole reads no field.
    fn tests(&self, held: &dyn Fn(TsNode<'t>) -> Vec<Held>) -> Vec<Test> {
        let u = self.uses;
        let kinds: Vec<&str> = u.names.iter().chain(u.reads).copied().collect();
        let callee = |q: TsNode<'t>| self.up(q).and_then(|c| c.child_by_field_name("function")).is_some_and(|f| f.id() == q.id());
        let mut out: Vec<Test> = vec![];
        for p in walk_all(self.body).into_iter().filter(|p| self.choices.contains_key(&p.id())) {
            let choice = self.choices[&p.id()];
            for c in named_children(p).into_iter().filter(|c| (self.syntax.part)(p, *c, self.src) == Part::Decides) {
                for n in walk(c, &kinds).into_iter().filter(|n| !(u.label)(*n, self.src)) {
                    // the projection the name is read through: the fields read of it, and what hands on the value inside it
                    let mut e = n;
                    while let Some(q) = self.up(e).filter(|_| e.id() != c.id()) {
                        let of = |x: TsNode| beneath(x, u.wrappers).id() == beneath(e, u.wrappers).id();
                        let wrapper = wraps(u.wrappers, q) && wrapped(q).is_some_and(|w| w.id() == e.id());
                        let reads = (u.field)(q, self.src).is_some_and(|(o, _)| of(o))
                            || (u.keeps.contains(&q.kind()) && q.named_child(0).is_some_and(of));
                        if !(wrapper || reads) || callee(q) {
                            break;
                        }
                        e = q;
                    }
                    for h in held(n) {
                        let Some(field) = self.read_of(e, n, &h, held).filter(|f| !f.is_empty()) else { continue };
                        let test = Test { choice, on: Held { at: 0, fills: vec![], through: vec![], ..h }, field };
                        if !out.contains(&test) {
                            out.push(test);
                        }
                    }
                }
            }
        }
        out
    }

    /// The fields an expression reads of the name it starts at, as the code writes them, the elements it
    /// goes through left out: `fig` of `other.fig`, `fig.title` of `self.fig.title`, `rows` of
    /// `self.rows[i]`. None where it starts at no name (a call's result) or reads no field.
    fn fields(&self, n: TsNode) -> Option<String> {
        self.projection(n).map(|(_, fields)| fields)
    }

    /// The name an expression is a projection of and the fields it reads of it (`fields`): `self` and `title` of
    /// `self.title`.
    fn projection<'n>(&self, n: TsNode<'n>) -> Option<(TsNode<'n>, String)> {
        let u = self.uses;
        let (mut k, mut fields) = (beneath(n, u.wrappers), vec![]);
        loop {
            if let Some((of, field)) = (u.field)(k, self.src) {
                fields.push(field);
                k = beneath(of, u.wrappers);
            } else if u.keeps.contains(&k.kind()) {
                k = beneath(k.named_child(0)?, u.wrappers);
            } else {
                break;
            }
        }
        fields.reverse();
        (!fields.is_empty() && (u.names.contains(&k.kind()) || u.reads.contains(&k.kind()))).then(|| (k, fields.join(".")))
    }

    /// Whether `n` is a part of a value the body has, no value of its own: a name, what a step gives, or a field or an element
    /// of either (`x`, `self.rows`, `sites[i]`, `figures().clean`).
    fn part_of_a_value(&self, n: TsNode<'t>) -> bool {
        let u = self.uses;
        let mut k = beneath(n, u.wrappers);
        loop {
            if let Some((of, _)) = (u.field)(k, self.src) {
                k = beneath(of, u.wrappers);
            } else if let Some(inner) = u.keeps.contains(&k.kind()).then(|| k.named_child(0)).flatten() {
                k = beneath(inner, u.wrappers);
            } else {
                return u.names.contains(&k.kind()) || u.reads.contains(&k.kind()) || self.sites.iter().any(|s| s.span == span(k));
            }
        }
    }

    /// The fields `e` reads of the value `h` the name `n` holds, where `e` is `n` or a projection of it: those it reads of
    /// `n` (`fields`) after the projection `n` is bound to first, where the reader finds the value at the name of a
    /// binding that reaches `n` (`mathematics.md`, "One fine plan": a projection bound to a name is still the projection).
    /// `t = self.title` makes `t`, and `t.x`, the projections `title` and `title.x` of the instance, and so through the
    /// names bound between (`a = self.chart; t = a.title` makes `t` `chart.title`). None where `e` is no projection of `n`,
    /// or where no binding of a projection makes `n` hold `h` (a loop's element, a `case` pattern): it holds the value
    /// whole, as far as the code says.
    fn read_of(&self, e: TsNode<'t>, n: TsNode<'t>, h: &Held, held: &dyn Fn(TsNode<'t>) -> Vec<Held>) -> Option<String> {
        let u = self.uses;
        let e = beneath(e, u.wrappers);
        let own = match self.projection(e) {
            Some((root, fields)) if root.id() == n.id() => fields,
            None if e.id() == n.id() => String::new(),
            _ => return None,
        };
        // a value the reader finds at the name itself, or at no place it says (the instance a method runs on), the name holds
        // as written
        let origin = origin(h) as usize;
        if origin == 0 || origin == n.start_byte() {
            return Some(own);
        }
        let first = self.body.named_descendant_for_byte_range(origin, origin + 1)?;
        let bound = self.bound_from(first, n, h, held, 0)?;
        Some([bound, own].into_iter().filter(|f| !f.is_empty()).collect::<Vec<_>>().join("."))
    }

    /// The fields of the value `h` a binding binds a name to, from `at`, where the reader finds the value: the binding
    /// whose value reads it, up through the fields and the elements it reads, and the fields of it it binds each name to
    /// (`bound_by`); on through the bindings of a projection of one of those names that holds the same value, up to the
    /// name `n` (`read_of`).
    fn bound_from(&self, at: TsNode<'t>, n: TsNode<'t>, h: &Held, held: &dyn Fn(TsNode<'t>) -> Vec<Held>, depth: usize) -> Option<String> {
        let u = self.uses;
        if depth > 8 {
            return None;
        }
        let mut k = at;
        while let Some(p) = self.up(k) {
            let of = |x: TsNode| beneath(x, u.wrappers).id() == beneath(k, u.wrappers).id();
            if (wraps(u.wrappers, p) && wrapped(p).is_some_and(|w| w.id() == k.id()))
                || (u.field)(p, self.src).is_some_and(|(o, _)| of(o))
                || (u.keeps.contains(&p.kind()) && p.named_child(0).is_some_and(of))
            {
                k = p;
            } else {
                break;
            }
        }
        // the binding of the value, or of the part of it at its place where the binding and its value are written as as
        // many parts (`fig, ax = self.fig, self.ax`)
        let at_value = |b: TsNode<'t>, v: TsNode<'t>| (u.binds)(b).filter(move |(_, value)| value.id() == v.id());
        let binding = self.up(k).filter(|b| at_value(*b, k).is_some()).or_else(|| {
            let list = self.up(k)?;
            self.up(list).filter(|b| at_value(*b, list).is_some_and(|(targets, value)| (u.pairs)(targets, value)))
        })?;
        let (targets, _) = (u.binds)(binding)?;
        let names: Vec<&str> = walk(targets, u.names).into_iter().filter(|x| !(u.label)(*x, self.src)).map(|x| text(x, self.src)).collect();
        if names.contains(&text(n, self.src)) {
            return self.bound_by(binding, text(n, self.src));
        }
        let later = |r: &TsNode| binding.end_byte() <= r.start_byte() && r.end_byte() <= n.start_byte();
        for r in self.index().names.iter().copied().filter(|r| later(r) && names.contains(&text(*r, self.src))) {
            if !held(r).iter().any(|x| x == h && origin(x) == origin(h)) {
                continue;
            }
            let (Some(here), Some(rest)) = (self.bound_by(binding, text(r, self.src)), self.bound_from(r, n, h, held, depth + 1)) else {
                continue;
            };
            return Some([here, rest].into_iter().filter(|f| !f.is_empty()).collect::<Vec<_>>().join("."));
        }
        None
    }

    /// The fields of the value it reads that `binding` binds `name` to: those its value reads of the name it is a
    /// projection of, or, where the binding and its value are written as as many parts, those the part at the name's place
    /// reads (`fig, ax = self.fig, self.ax` binds `ax` to `ax`). An element is left out (`row = self.rows[k]` is `rows`).
    fn bound_by(&self, binding: TsNode<'t>, name: &str) -> Option<String> {
        let u = self.uses;
        let part = self.bound_to(binding, name)?;
        match self.projection(part) {
            Some((_, fields)) => Some(fields),
            None if u.names.contains(&part.kind()) || u.reads.contains(&part.kind()) => Some(String::new()),
            None => None,
        }
    }

    /// The expression `binding` binds `name` to: its value, or, where the binding and its value are written as as many parts,
    /// the part at the name's place (`fig, ax = self.fig, self.ax` binds `ax` to `self.ax`).
    fn bound_to(&self, binding: TsNode<'t>, name: &str) -> Option<TsNode<'t>> {
        let u = self.uses;
        let (targets, value) = (u.binds)(binding)?;
        let each = |x: TsNode<'t>| named_children(x).into_iter().filter(|c| !c.kind().ends_with("comment")).collect::<Vec<_>>();
        let part = match ((u.pairs)(targets, value), each(targets), each(value)) {
            (true, ts, vs) if ts.len() == vs.len() => {
                let i = ts.iter().position(|t| walk(*t, u.names).iter().any(|x| text(*x, self.src) == name))?;
                vs[i]
            }
            _ => value,
        };
        Some(beneath(part, u.wrappers))
    }

    /// Whether the body's own code reads what step `i` of the program's code gives, by no name: through
    /// what hands it on unchanged and the parts of it it takes, what it is in is no step, no binding of a
    /// name, no statement of its own and not what leaves the body as it is (`mode.plan().timeout <= ..`,
    /// `ledger.merged(other).rows.length + ..`, `this.rows().map((row) => row.reading)`). A binding that
    /// keeps it in a field or an element of a value reads it (`self.left = Chart("left")`). What a function
    /// written in place gives back goes where the function's result goes: handed to a library's call that is
    /// no step, on in what that call gives (`.map(|t| t.drawn()).collect()`). A step written around it is
    /// handed it.
    fn read_by_own_code(&self, i: usize) -> bool {
        let (u, s) = (self.uses, &self.sites[i]);
        if s.to.is_empty() || s.span == (0, 0) || s.raises || s.gives.iter().any(|g| !g.name.is_empty()) {
            return false;
        }
        let Some(c) = self.read_at(s.span) else { return false };
        // what a function handed on by its name gives, the call's it is handed to (`lines.map(parse_line)`)
        if u.names.contains(&c.kind()) {
            return false;
        }
        let (mut child, mut part) = (c, false);
        while let Some(p) = self.up(child).filter(|_| child.id() != self.body.id()) {
            let last =
                named_children(p).into_iter().filter(|x| !x.kind().ends_with("comment")).last().is_some_and(|l| l.id() == child.id());
            if wraps(u.wrappers, p) && wrapped(p).is_some_and(|w| w.id() == child.id()) {
                child = p;
            } else if u.keeps.contains(&p.kind()) {
                (child, part) = (p, true);
            } else if (u.tails.contains(&p.kind()) && last && !(u.drops)(p, child, self.src))
                || matches!((self.syntax.part)(p, child, self.src), Part::Arm(_))
            {
                // the value of an arm, or of a block it ends, is the value of the choice or the block
                child = p;
            } else if named_children(p).into_iter().any(|x| x.id() != child.id() && (u.binders)(p, x) == Some(true)) {
                // what a function written in place gives is that function's (`let wrong = || Error::Amount(..)`); handed to
                // a library's call that is no step, it goes on in what that call gives, as a value handed to one does
                match self.glued_call(p) {
                    Some(call) => (child, part) = (call, false),
                    None => return false,
                }
            } else {
                let leaves = !part && ((u.leaves)(child, self.body) || (u.leaves)(p, self.body));
                // what a function written in place returns is what it gives back (`(t) => { return t.drawn(); }` is
                // `(t) => t.drawn()`)
                if let Some(f) = leaves.then(|| self.function_around(p)).flatten() {
                    match self.glued_call(f) {
                        Some(call) => {
                            (child, part) = (call, false);
                            continue;
                        }
                        None => return false,
                    }
                }
                // a step that stands where it is written (a loop over it) or around it is handed it
                let around = |(j, x): (usize, &Site)| {
                    j != i && x.span != (0, 0) && x.span.0 as usize <= child.start_byte() && child.end_byte() <= x.span.1 as usize
                };
                // a binding of a name hands the value to the name; one that keeps it in a field or an element of a value
                // (`self.left = Chart(..)`) is the body's own code putting it there, a lens's put
                let bound = (u.binds)(p).is_some_and(|(targets, value)| value.id() == child.id() && walk(targets, u.keeps).is_empty());
                return !self.sites.iter().enumerate().any(around)
                    && !bound
                    && !leaves
                    && !(u.drops)(p, child, self.src)
                    && !(u.fails)(p, self.src);
            }
        }
        false
    }

    /// The node a step is read at: the one its span covers, or for a call written from before that node (`Step::from`, a
    /// method's call among a macro's tokens), the outermost node within the span that ends where it does: the group the
    /// call is handed.
    fn read_at(&self, (a, b): (u32, u32)) -> Option<TsNode<'t>> {
        let (a, b) = (a as usize, b as usize);
        let n = self.body.descendant_for_byte_range(a, b)?;
        if n.byte_range() == (a..b) {
            return Some(n);
        }
        let mut t = self.body.descendant_for_byte_range(b.saturating_sub(1), b)?;
        while let Some(p) = t.parent().filter(|p| a <= p.start_byte() && p.end_byte() == b) {
            t = p;
        }
        Some(t)
    }

    /// The innermost function written in place that `n` stands in, within the body: the node that binds its own names
    /// beside the part `n` is in (`|x| ..`, `(x) => { .. }`).
    fn function_around(&self, n: TsNode<'t>) -> Option<TsNode<'t>> {
        let mut child = n;
        while let Some(p) = self.up(child).filter(|_| child.id() != self.body.id()) {
            if named_children(p).into_iter().any(|x| x.id() != child.id() && (self.uses.binders)(p, x) == Some(true)) {
                return Some(p);
            }
            child = p;
        }
        None
    }

    /// The call a function written in place at `f` is handed to as an argument (also by a name, `key=lambda r: ..`),
    /// where that call is no step of the body: a library's call, glued into the body's own code.
    fn glued_call(&self, f: TsNode<'t>) -> Option<TsNode<'t>> {
        let mut a = f;
        for _ in 0..3 {
            let p = self.up(a)?;
            if let Some(call) = self.up(p).filter(|c| (self.uses.hands)(*c, p)) {
                let at = (call.start_byte() as u32, call.end_byte() as u32);
                return (!self.sites.iter().any(|s| s.span == at)).then_some(call);
            }
            a = p;
        }
        None
    }

    /// Whether the name `n` is read where it stands: no label (the `f` of `x.f`), not what a binding
    /// or a pattern binds (`x` of `x = v`, the names of `Some(x) =>`), and not a name a function written
    /// in place binds for itself. A binding that reads what it binds reads it (`x += v`), and a change
    /// made through a name reads the name (`x.f = v`, `rows[i] = v`). A name a binding binds to a part
    /// of what it holds (`width = settings.width`, `row = rows[k]`, `for r in rows`) is read where the
    /// bound name is: that name stands for the part, and a step it is handed takes it, the projection
    /// glued into the step (`mathematics.md`, "One fine plan"). An operator, a format or a call the
    /// body computes a value with reads what it is computed from.
    fn read_here(&self, n: TsNode<'t>) -> bool {
        let u = self.uses;
        if (u.label)(n, self.src) {
            return false;
        }
        let name = text(n, self.src);
        // `chain`: what lies between `n` and `child` takes a part of what `n` holds, or hands it on
        let (mut child, mut through, mut chain) = (n, false, true);
        while let Some(p) = self.up(child).filter(|_| child.id() != self.body.id()) {
            if (u.binders)(p, child).is_some() {
                return false;
            }
            let own = |c: &TsNode| c.id() != child.id() && (u.binders)(p, *c) == Some(true);
            if named_children(p).into_iter().filter(own).any(|c| walk(c, u.names).iter().any(|x| text(*x, self.src) == name)) {
                return false;
            }
            if let Some((targets, value)) = (u.binds)(p) {
                if targets.id() == child.id() {
                    return through || u.updates.contains(&p.kind());
                }
                let kept = !walk(targets, u.keeps).is_empty();
                if value.id() == child.id() && chain && !kept && !u.updates.contains(&p.kind()) && !self.decides(p) {
                    return false;
                }
            }
            if (u.parts)(p, child) && chain {
                return false;
            }
            // a binding written as as many parts as its value binds each name to the part at its place, two bindings
            // (`fig, ax = self.fig, self.ax`)
            let paired = self.up(p).and_then(|b| (u.binds)(b)).is_some_and(|(t, v)| v.id() == p.id() && (u.pairs)(t, v));
            through |= u.keeps.contains(&p.kind());
            chain &= u.keeps.contains(&p.kind()) || wraps(u.wrappers, p) || paired;
            child = p;
        }
        true
    }

    /// A call `c`, `name`d as the code writes it, of which the graph holds no more than what it
    /// gives: a library's that crosses into no place, changes no value and ends nothing as the
    /// tables say, or one Atlas cannot follow. Where what it gives goes nowhere the function hands
    /// on, the call is made for an effect Atlas does not see.
    pub fn dropped(&mut self, c: TsNode<'t>, name: &str) {
        if !self.goes_on(c, &mut HashSet::new()) {
            self.unseen(c, &format!("{name}(..)"));
        }
    }

    /// What an initializer keeps of its parameters in the value it builds where its signature says so (TypeScript's
    /// `constructor(readonly until: string)`): the body's own code reads them and sets each into the field of its name of
    /// the value `on`, as `self.until = until` does. Called after `uses`.
    pub fn keeps(&mut self, on: Held, values: Vec<Held>) {
        for h in values {
            let one = Use { name: h.name.clone(), ty: h.ty.clone(), arms: vec![], param: h.param };
            if !self.own.contains(&one) {
                self.own.push(one);
            }
            self.sets.push(Set { on: on.clone(), field: h.name.clone(), from: vec![h], ..Set::default() });
        }
    }

    /// What the body does at `n` that the graph holds nothing else of, as the code writes it.
    pub fn unseen(&mut self, n: TsNode, written: &str) {
        let one = worded(written);
        if !self.unseen.iter().any(|(l, w)| *l == line(n) && *w == one) {
            self.unseen.push((line(n), one));
        }
    }

    /// Mark the touches call `c` made as checks when what it gives goes to nothing the function
    /// hands on.
    pub fn mark<'a>(&self, c: TsNode<'t>, touches: impl IntoIterator<Item = &'a mut Touch>) {
        let checks = !self.goes_on(c, &mut HashSet::new());
        for t in touches {
            t.checks = checks;
        }
    }

    /// Whether what `v` gives reaches what the function hands on: it is returned, handed to a
    /// call, kept in a field or an element, or taken apart by a `match`, or a name it is bound to is. What only decides a
    /// choice, goes to an error or is dropped goes no further. `bound` are the names followed.
    fn goes_on(&self, v: TsNode<'t>, bound: &mut HashSet<String>) -> bool {
        let (u, mut child, mut handed) = (self.uses, v, false);
        while let Some(p) = self.up(child) {
            if (u.leaves)(child, self.body) {
                return true;
            }
            if (self.syntax.part)(p, child, self.src) == Part::Decides || (u.fails)(p, self.src) {
                return false;
            }
            if (u.splits)(p, child) {
                return true;
            }
            handed |= (u.hands)(p, child);
            // a call that changes a value it is handed (`f.read_to_string(&mut s)`) makes it anew from
            // what it runs on: what the call is made of goes on where that value goes
            let after = p.end_byte();
            for changed in (u.changes)(p) {
                let name = text(changed, self.src).to_string();
                if !bound.insert(name.clone()) {
                    continue;
                }
                if self.written_after(&name, after).filter(|n| !self.rebinds(*n)).any(|n| self.goes_on(n, bound)) {
                    return true;
                }
            }
            if let Some((targets, value)) = (u.binds)(p).filter(|(_, value)| value.id() == child.id()) {
                if walk(targets, u.keeps).first().is_some() {
                    return true;
                }
                let after = value.end_byte();
                let names: Vec<String> =
                    walk(targets, u.names).into_iter().map(|n| text(n, self.src).to_string()).filter(|n| bound.insert(n.clone())).collect();
                return names
                    .into_iter()
                    .any(|name| self.written_after(&name, after).filter(|n| !self.rebinds(*n)).any(|n| self.goes_on(n, bound)));
            }
            if p.id() == self.body.id() {
                break;
            }
            child = p;
        }
        handed
    }

    /// Whether the name `n` is what a binding binds, not a value it reads.
    fn rebinds(&self, n: TsNode<'t>) -> bool {
        let mut child = n;
        while let Some(p) = self.up(child).filter(|p| p.id() != self.body.id()) {
            if let Some((targets, _)) = (self.uses.binds)(p) {
                return targets.id() == child.id();
            }
            child = p;
        }
        false
    }

    /// The choices of the body, numbered.
    pub fn choices(&self) -> &HashMap<usize, u32> {
        &self.choices
    }

    /// The arms of the choices `n` sits in, from the outermost in: of each choice that holds it, and of each guard before
    /// it in a block that holds it.
    pub fn arms(&self, n: TsNode<'t>) -> Vec<(u32, u32)> {
        let (ids, syn) = (&self.choices, self.syntax);
        let mut out = vec![];
        let mut child = n;
        while child.id() != self.body.id() {
            let Some(p) = self.up(child) else { break };
            out.extend(guarded_by(self.before(child), self.src, ids, syn));
            if let (Some(&c), Part::Arm(a)) = (ids.get(&p.id()), (syn.part)(p, child, self.src)) {
                out.push((c, a));
            }
            child = p;
        }
        out.reverse();
        out
    }

    /// Keep the names a call's value is held by.
    pub fn keep(&mut self, gives: &[Held]) {
        self.kept.extend(gives.iter().map(|g| g.name.clone()).filter(|n| !n.is_empty()));
    }

    /// A call `c` of the program's own code: a step to `to`, and an edge of `kind` (`via` the
    /// name it was called by, when no such function is known).
    pub fn call(&mut self, c: TsNode<'t>, to: &str, kind: EdgeKind, via: Option<String>, step: Step) {
        let arms = self.arms(c);
        self.keep(&step.gives);
        let (args, written) = self.written(c, step.receiver_first);
        let site = Site {
            line: line(c),
            to: to.to_string(),
            on: step.on,
            built: step.built,
            on_receiver: step.on_receiver,
            named: step.named,
            after: step.after,
            hands: step.hands,
            gives: step.gives,
            args,
            written,
            arms: arms.clone(),
            span: step.from.map_or(span(c), |at| (at as u32, c.end_byte() as u32)),
            command_line: step.command_line,
            ..Default::default()
        };
        let site = self.site(c, site);
        self.sites.push(site);
        self.edges.push((to.to_string(), kind, via, arms));
    }

    /// A function or class `to`, handed on as a value at `a` inside call `c`: run by whoever
    /// receives it. Where the call runs it on each element of what it goes over (`xs.map(f)`,
    /// `map(f, xs)`, `sorted(xs, key=f)`: `knowledge::RUST_EACH`, `PYTHON_EACH`, `TYPESCRIPT_EACH`),
    /// `each` says what the call hands it: the values the element is a part of, and the parameter the
    /// element fills. It is then a step where it is named, handed the element.
    pub fn handed(&mut self, c: TsNode<'t>, a: TsNode<'t>, to: &str, kind: EdgeKind, each: Option<(Vec<Held>, Slot)>) {
        if let Some((hands, slot)) = each {
            let hands = hands.into_iter().map(|h| Held { fills: vec![slot.clone()], ..h }).collect();
            self.call(a, to, kind, None, Step { hands, ..Default::default() });
            // the call hands it the element, which the body writes nowhere
            if let Some(s) = self.sites.last_mut() {
                (s.args, s.written) = (vec![slot], vec![(0, 0)]);
            }
            return;
        }
        let arms = self.arms(a);
        // what it raises, the call it is handed to hands on, as that call stands
        let site = self.site(c, Site { line: line(c), to: to.to_string(), arms: arms.clone(), ..Default::default() });
        self.sites.push(Site { dropped: false, raises: false, ..site });
        self.edges.push((to.to_string(), kind, None, arms));
    }

    /// A library's operation `op` that the body reads no further: a step when it makes a value the
    /// body keeps, whatever it is handed, or the error it raises (`raise RuntimeError(..)`, `throw new
    /// Error(..)`), of the class the reader gives.
    pub fn op(&mut self, c: TsNode<'t>, op: &str, hands: Vec<Held>, gives: Vec<Held>) {
        if gives.is_empty() {
            return;
        }
        self.unread(c, op, hands, gives, false);
    }

    /// A call `op` the body reads no further that changes the values `changed` it is called on
    /// (`rows.append(row)`, `random.shuffle(xs)`): a step that gives them anew from what it is
    /// handed, `T × A → T`, beside what it returns.
    pub fn changes(&mut self, c: TsNode<'t>, op: &str, hands: Vec<Held>, gives: Vec<Held>, changed: Vec<Held>) {
        let gives = changed_values(gives, changed);
        if gives.is_empty() {
            return;
        }
        self.unread(c, op, hands, gives, false);
        if let Some(s) = self.sites.last_mut() {
            s.changes = true;
        }
    }

    /// A library's call `op` that parses the command line the program is run with into values,
    /// with the `options` it declares: a step whatever it gives, since what it gives is the
    /// entry's.
    pub fn parses(&mut self, c: TsNode<'t>, op: &str, hands: Vec<Held>, gives: Vec<Held>, options: Vec<String>) {
        self.unread(c, op, hands, gives, false);
        if let Some(s) = self.sites.last_mut() {
            s.command_line = Some(options);
        }
    }

    /// A call Atlas cannot follow, `name`d as the code writes it: it runs all the same, so it is
    /// a step of the body, whatever it is handed and gives.
    pub fn not_followed(&mut self, c: TsNode<'t>, name: &str, hands: Vec<Held>, gives: Vec<Held>) {
        self.unread(c, name, hands, gives, true);
    }

    /// A call the body reads no further, `name`d as the code writes it: a library's operation,
    /// or one Atlas cannot follow. A line break and the indentation after it are the layout of the
    /// source, no part of the name: `text\n    .split` is `text.split`.
    fn unread(&mut self, c: TsNode<'t>, name: &str, hands: Vec<Held>, gives: Vec<Held>, not_followed: bool) {
        self.keep(&gives);
        let op = name.lines().map(str::trim).collect::<String>();
        let (args, written) = self.written(c, false);
        let site =
            Site { line: line(c), op, not_followed, hands, gives, args, written, arms: self.arms(c), span: span(c), ..Default::default() };
        let site = self.site(c, site);
        self.sites.push(site);
    }

    /// A loop as the step it is, where what it goes `over` is written: it takes what it goes over and
    /// gives each element under the names it binds (`for n in g.nodes`), `label`led as the code writes
    /// it. A call written there is a step of its own, which hands the loop what it gives.
    pub fn each(&mut self, over: TsNode<'t>, label: &str, hands: Vec<Held>, gives: Vec<Held>) {
        if gives.is_empty() {
            return;
        }
        self.unread(over, label, hands, gives, false);
        // what a loop goes over is no argument of a call written there
        if let Some(s) = self.sites.last_mut() {
            (s.args, s.written) = (vec![], vec![]);
        }
    }

    /// The arguments call `c` writes (`Site::args`), with where each is written. A method called
    /// through its class or its path runs on the first argument it is handed by place.
    fn written(&self, c: TsNode, receiver_first: bool) -> (Vec<Slot>, Vec<(u32, u32)>) {
        let mut args = (self.uses.args)(c, self.src);
        if receiver_first {
            args.retain(|(_, s)| *s != Slot::Receiver);
            for (_, s) in args.iter_mut() {
                let shifted = match &*s {
                    Slot::At(0) => Slot::Receiver,
                    Slot::At(n) => Slot::At(n - 1),
                    Slot::Spread(n) => Slot::Spread(n.saturating_sub(1)),
                    other => other.clone(),
                };
                *s = shifted;
            }
        }
        args.into_iter().map(|((a, b), s)| (s, (a as u32, b as u32))).unzip()
    }

    /// Which parameters each value a step is handed fills (`Held::fills`): the arguments of the call
    /// (`Site::args`) a name that holds it is written in, outside the steps written inside the call,
    /// whose names are theirs, and the one the reader found it in. What a step written inside gives
    /// fills the argument it stands in once the graph wires the steps (`builder::fill_nested`). `held`
    /// is the reader's, as for `uses`: the values a name holds where it stands. An argument that is a
    /// projection of the value a name holds (`self.title`, `self.fig.ax`) fills its parameter through
    /// the fields it reads (`Held::through`), the value it reads them of being what the call is handed.
    pub fn fills(&mut self, held: &dyn Fn(TsNode<'t>) -> Vec<Held>) {
        let u = self.uses;
        let kinds: Vec<&str> = u.names.iter().chain(u.reads).copied().collect();
        let spans: Vec<(u32, u32)> = self.sites.iter().map(|s| s.span).filter(|s| *s != (0, 0)).collect();
        for i in 0..self.sites.len() {
            let (call, written) = (self.sites[i].span, self.sites[i].written.clone());
            if written.is_empty() || self.sites[i].hands.is_empty() {
                continue;
            }
            let inner: Vec<(u32, u32)> = spans.iter().copied().filter(|s| *s != call && call.0 <= s.0 && s.1 <= call.1).collect();
            let within = |at: u32| inner.iter().any(|(a, b)| *a <= at && at < *b);
            let mut fills: Vec<Vec<Slot>> = self.sites[i].hands.iter().map(|h| h.fills.clone()).collect();
            let mut through: Vec<Vec<(Slot, String)>> = self.sites[i].hands.iter().map(|h| h.through.clone()).collect();
            // where an argument as a whole is a projection of one name, where that name is written and the fields it reads
            let mut projected: Vec<Option<(u32, String)>> = vec![];
            for (j, (a, b)) in written.iter().enumerate() {
                let slot = &self.sites[i].args[j];
                let arg = self.body.descendant_for_byte_range(*a as usize, *b as usize);
                let whole = arg.filter(|x| x.start_byte() == *a as usize && x.end_byte() == *b as usize);
                projected.push(whole.and_then(|x| self.projection(x)).map(|(root, f)| (root.start_byte() as u32, f)));
                let Some(arg) = arg else { continue };
                let written_here = |n: &TsNode| *a as usize <= n.start_byte() && n.end_byte() <= *b as usize;
                for n in walk(arg, &kinds).into_iter().filter(written_here) {
                    if within(n.start_byte() as u32) || !self.read_here(n) {
                        continue;
                    }
                    for h in held(n) {
                        if let Some(k) = self.sites[i].hands.iter().position(|x| *x == h) {
                            if !fills[k].contains(slot) {
                                fills[k].push(slot.clone());
                            }
                            // the argument as a whole is the name or a projection of it, through the projection the name is
                            // bound to first (`t = self.title; other.take(t, ..)` fills through `title`)
                            let field = whole.and_then(|x| self.read_of(x, n, &h, held)).filter(|f| !f.is_empty());
                            if let Some(f) = field.filter(|_| !through[k].iter().any(|(s, _)| s == slot)) {
                                through[k].push((slot.clone(), f));
                            }
                        }
                    }
                }
            }
            for (k, h) in self.sites[i].hands.iter().enumerate() {
                let Some(j) = written.iter().position(|(a, b)| h.at != 0 && *a <= h.at && h.at < *b) else { continue };
                let slot = &self.sites[i].args[j];
                if !fills[k].contains(slot) {
                    fills[k].push(slot.clone());
                }
                // the value the reader found where the projection's name is written fills the parameter through its fields
                if let Some((_, f)) = projected[j].as_ref().filter(|(root, _)| *root == h.at && !through[k].iter().any(|(s, _)| s == slot))
                {
                    through[k].push((slot.clone(), f.clone()));
                }
            }
            for ((h, f), t) in self.sites[i].hands.iter_mut().zip(fills).zip(through) {
                (h.fills, h.through) = (f, t);
            }
        }
    }

    /// An edge of `kind` to `to` at call `c`, with no step of its own.
    pub fn edge(&mut self, c: TsNode<'t>, to: &str, kind: EdgeKind) {
        self.edges.push((to.to_string(), kind, None, self.arms(c)));
    }

    /// Write the body's edges and its steps to the graph.
    pub fn finish(mut self, store: &mut GraphBuilder, id: &str) {
        self.glued_fails();
        let (arm_lines, loop_lines, whens, binds) = (self.arm_lines(), self.loop_lines(), self.whens(), self.binds());
        for (t, kind, via, arms) in self.edges {
            store.add_call(id, &t, kind, via, arms);
        }
        let mut sites = self.sites;
        let mut own = self.own;
        // what a step gives that the body's own code reads by no name is named by its step, as one handed to a step written
        // around it is, and is of the class the step builds or its function gives
        for &i in &self.read {
            let s = &mut sites[i];
            let given = match store.nodes.get(&s.to) {
                Some(n) if n.kind == NodeKind::Class => s.to.clone(),
                _ => store
                    .edges
                    .iter()
                    .find(|e| {
                        e.kind == EdgeKind::Returns && e.src == s.to && store.nodes.get(&e.dst).is_some_and(|d| d.kind == NodeKind::Class)
                    })
                    .map(|e| e.dst.clone())
                    .unwrap_or_default(),
            };
            if given.is_empty() {
                continue;
            }
            let name = format!("#{id}@{}", s.span.0);
            match s.gives.iter_mut().find(|g| g.ty == given) {
                Some(g) => g.name = name.clone(),
                None => s.gives.push(Held::new(given.clone(), name.clone())),
            }
            // a set that keeps what the step gives in a field reads it (`self.left = Chart("left")`)
            let one = Held::new(given.clone(), name.clone());
            for set in self.sets.iter_mut().filter(|t| t.value.0 <= s.span.0 && s.span.1 <= t.value.1 && !t.from.contains(&one)) {
                set.from.push(one.clone());
            }
            own.push(Use { name, ty: given, arms: s.arms.clone(), param: false });
        }
        if let Some(n) = store.nodes.get_mut(id) {
            n.sites = sites;
            (n.arm_lines, n.arm_choices) = arm_lines.into_iter().unzip();
            n.loop_lines = loop_lines;
            n.whens = whens;
            n.unseen = self.unseen;
            n.uses = own;
            n.binds = binds;
            n.fails = self.fails;
            n.sets = self.sets;
            n.tests = self.tests;
        }
    }

    /// What the body binds a name to by its own code (`Node::binds`): of each binding of the body, what
    /// it binds that no step makes. In an arm, either the binding standing in the arm (`unit = KELVIN`)
    /// or the value it binds being a choice whose arm gives it (`let out = match f { .. }`, `x = a if c
    /// else b`); outside the choices, a literal (`items = []`) or any other expression (`mask =
    /// samples.abs() > 0`, `let total = a + b`), whatever it reads: the name is the body's own. What
    /// the language's own code computes of names with no call (`a + b`) is no value of its own: the name
    /// stands for those names where it is read (`stands_for`).
    /// What a loop binds, its step gives; a field or an element is kept beyond a name; a name bound to
    /// another, to what a step gives or to a part of either (`width = settings.width`) holds that value
    /// (`Node::uses`); a function written in place is a function.
    fn binds(&self) -> Vec<Use> {
        let u = self.uses;
        let steps: HashSet<(u32, u32)> = self.sites.iter().map(|s| s.span).filter(|s| *s != (0, 0)).collect();
        let mut out: Vec<Use> = vec![];
        let mut stack = vec![self.body];
        while let Some(b) = stack.pop() {
            if b.id() != self.body.id() && u.nested.contains(&b.kind()) {
                continue;
            }
            stack.extend(named_children(b).into_iter().rev());
            let Some((targets, value)) = (u.binds)(b) else { continue };
            // a function written in place binds its own names, but for what it binds by a literal: its steps are the body's
            let literal = |v: TsNode| self.literals.contains(&(v.start_byte(), v.end_byte()));
            if b.child_by_field_name("body").is_some()
                || !walk(targets, u.keeps).is_empty()
                || (self.in_function(b) && !literal(beneath(value, u.wrappers)))
            {
                continue;
            }
            let names: Vec<String> =
                walk(targets, u.names).into_iter().filter(|n| !(u.label)(*n, self.src)).map(|n| text(n, self.src).to_string()).collect();
            for leaf in self.values_of(value, None) {
                let made = steps.contains(&span(leaf));
                if made || (u.leaves)(leaf, self.body) || (u.fails)(leaf, self.src) {
                    continue;
                }
                let arms = self.arms(leaf);
                let by_literal = arms.is_empty() && literal(leaf);
                // outside the choices, an expression that is no name, no step and no part of what either holds, and in which
                // no step gives the name: a value the body makes itself, as by a literal. Several names bound to one value it does not
                // write as as many parts are parts of that value; a function written in place is a function, no value
                let whole = beneath(value, u.wrappers);
                let by_own_code = |name: &str| {
                    arms.is_empty()
                        && !u.updates.contains(&b.kind())
                        && !self.decides(b)
                        && self.bound_to(b, name).is_some_and(|part| {
                            let within = |s: &Site| part.start_byte() <= s.span.0 as usize && s.span.1 as usize <= part.end_byte();
                            (names.len() == 1 || part.id() != whole.id())
                                && !self.part_of_a_value(part)
                                && !named_children(part).into_iter().any(|x| (u.binders)(part, x) == Some(true))
                                && !self
                                    .sites
                                    .iter()
                                    .filter(|s| s.span != (0, 0))
                                    .any(|s| within(s) && (s.span == span(part) || s.gives.iter().any(|g| g.name == name)))
                        })
                };
                for name in names.iter().filter(|n| !arms.is_empty() || by_literal || by_own_code(n)) {
                    let one = Use { name: name.clone(), ty: String::new(), arms: arms.clone(), param: false };
                    if !out.contains(&one) {
                        out.push(one);
                    }
                }
            }
        }
        // a name a step gives elsewhere holds what the step gives; one a literal binds, what the name holds where a step is
        // handed it or the body's own code reads it
        for b in out.iter_mut() {
            let given = self.sites.iter().flat_map(|s| &s.gives);
            let handed = self.sites.iter().flat_map(|s| &s.hands).filter(|_| b.arms.is_empty());
            let read = self.own.iter().filter(|_| b.arms.is_empty()).filter(|u| u.name == b.name && !u.ty.is_empty()).map(|u| &u.ty);
            let types: HashSet<&String> =
                given.chain(handed).filter(|g| g.name == b.name && !g.ty.is_empty()).map(|g| &g.ty).chain(read).collect();
            if let [t] = types.into_iter().collect::<Vec<_>>()[..] {
                b.ty = t.clone();
            }
        }
        out
    }

    /// The expressions whose value `v` gives: itself, or where it is a choice, those of each of its
    /// arms, through what hands on the value inside it and the blocks and arms whose value is their
    /// last part. `within` is the choice an arm is of: a part of it that continues it is no choice of
    /// its own (a `match` arm).
    fn values_of(&self, v: TsNode<'t>, within: Option<u32>) -> Vec<TsNode<'t>> {
        let u = self.uses;
        let v = beneath(v, u.wrappers);
        if let Some(&k) = self.choices.get(&v.id()).filter(|k| Some(**k) != within) {
            let arms = walk_all(v)
                .into_iter()
                .filter(|x| self.choices.get(&x.id()) == Some(&k))
                .flat_map(|x| named_children(x).into_iter().filter(move |c| matches!((self.syntax.part)(x, *c, self.src), Part::Arm(_))));
            return arms.collect::<Vec<_>>().into_iter().flat_map(|a| self.values_of(a, Some(k))).collect();
        }
        if u.tails.contains(&v.kind()) {
            let last = named_children(v).into_iter().filter(|c| !c.kind().ends_with("comment")).last();
            return last.filter(|l| !(u.drops)(v, *l, self.src)).map(|l| self.values_of(l, within)).unwrap_or_default();
        }
        vec![v]
    }

    /// What decides each arm of the body's choices, as the code writes it: `(choice, arm, when)`.
    /// An arm that decides itself (a `case`, a `match` arm) runs when its pattern matches; the
    /// first arm of a choice decided in one place (an `if`, an `elif`, a conditional expression)
    /// when that holds, and a later one otherwise. Of two arms nothing decides (a `try` and its
    /// handler) the first is tried and the second runs otherwise, and the one arm of a choice
    /// nothing decides (what an or-else falls back on) runs otherwise; the first, and the arms of a
    /// longer choice nothing decides, have no words.
    fn whens(&self) -> Vec<(u32, u32, String)> {
        let decided = |n: TsNode| -> String {
            let parts: Vec<&str> = named_children(n)
                .into_iter()
                .filter(|c| (self.syntax.part)(n, *c, self.src) == Part::Decides)
                .map(|c| crate::tree::text(c, self.src))
                .collect();
            worded(&parts.join(" if "))
        };
        let mut out = vec![];
        for p in walk_all(self.body).into_iter().filter(|p| self.choices.contains_key(&p.id())) {
            let k = self.choices[&p.id()];
            let arms: Vec<(TsNode, u32)> = named_children(p)
                .into_iter()
                .filter_map(|c| match (self.syntax.part)(p, c, self.src) {
                    Part::Arm(a) => Some((c, a)),
                    _ => None,
                })
                .collect();
            let first = arms.iter().map(|(_, a)| *a).min();
            let n_arms = arms.len();
            let own = decided(p);
            for (c, a) in arms {
                let itself = if self.choices.contains_key(&c.id()) { decided(c) } else { String::new() };
                let when = match () {
                    _ if !itself.is_empty() => itself,
                    _ if Some(a) == first && !(own.is_empty() && n_arms == 1) => own.clone(),
                    _ if n_arms == 1 => "otherwise".to_string(),
                    _ if !own.is_empty() || n_arms == 2 => "otherwise".to_string(),
                    _ => String::new(),
                };
                if !when.is_empty() {
                    out.push((k, a, when));
                }
            }
        }
        // what follows a guard runs when none of its arms does
        for (k, a, _, _) in guards(self.body, self.src, &self.choices, self.syntax) {
            out.push((k, a, "otherwise".to_string()));
        }
        out
    }

    /// The lines each loop of the body spans: a statement that binds a name to each element of a
    /// value and runs a body for it (`for x in v`).
    fn loop_lines(&self) -> Vec<(u32, u32)> {
        walk_all(self.body)
            .into_iter()
            .filter(|n| (self.uses.binds)(*n).is_some() && n.child_by_field_name("body").is_some())
            .map(|n| (line(n), n.end_position().row as u32 + 1))
            .collect()
    }

    /// The lines each arm of the body's choices spans, with the choice it is an arm of.
    fn arm_lines(&self) -> Vec<((u32, u32), u32)> {
        let mut out = vec![];
        for p in walk_all(self.body).into_iter().filter(|p| self.choices.contains_key(&p.id())) {
            for c in named_children(p) {
                if let Part::Arm(_) = (self.syntax.part)(p, c, self.src) {
                    out.push(((line(c), c.end_position().row as u32 + 1), self.choices[&p.id()]));
                }
            }
        }
        for (k, _, first, last) in guards(self.body, self.src, &self.choices, self.syntax) {
            out.push(((line(first), last.end_position().row as u32 + 1), k));
        }
        out
    }
}

/// The error type a written result names apart from its value, where it names one: the second
/// argument of `Result<T, E>`, as written.
pub fn error_written(written: &str) -> Option<&str> {
    let open = written.find('<')?;
    let inner = written[open + 1..].trim_end().strip_suffix('>')?;
    let (mut depth, mut at) = (0, None);
    for (i, c) in inner.char_indices() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => at = Some(i),
            _ => {}
        }
    }
    at.map(|i| inner[i + 1..].trim())
}

/// How a language binds names in a body, as far as a change made through a name is followed to
/// the value it changes.
pub struct BindSyntax {
    /// A statement that binds one name to a whole value: the name, and the names whose value it may
    /// be or be a part of (`x = graph`, `x = self.items[0]`, `x = a or b`); none for a value made
    /// anew (`x = copy(v)`, `x = xs[:]`). An augmented assignment (`x += v`) binds nothing new.
    pub binds: for<'a> fn(TsNode, &'a [u8]) -> Option<(&'a str, Vec<&'a str>)>,
    /// A loop that binds one name to each element of a value: the name, the name that value is a
    /// part of (`for x in graph.nodes`), and the loop's body.
    pub loops: for<'t, 'a> fn(TsNode<'t>, &'a [u8]) -> Option<(&'a str, Option<&'a str>, TsNode<'t>)>,
}

/// What a change made through `name` at byte `at` of `body` changes: the name whose value that
/// is when the change runs, or none when it is a value the body made. A loop binds the name to an
/// element of what it goes over, so inside the loop a change through it changes that value (`for
/// node in graph.nodes: node.status = ..` changes `graph`). A statement of the body's own, which
/// runs whenever the body does, decides what the name holds after it: another name or a part of
/// one makes it that value (`g = graph`), a value that may be what the name held keeps that, and
/// a new value makes a change after it one to the new value, not to what the name held before
/// (`graph = copy.copy(graph)`). A binding in a branch or a loop may not have run, so the name
/// holds what it held before it: the change may be one to that.
pub fn changed_through<'a>(body: TsNode, src: &'a [u8], syn: &BindSyntax, name: &'a str, at: usize) -> Option<&'a str> {
    let nodes = walk_all(body);
    let binds: Vec<(TsNode, &str, Vec<&str>)> = nodes.iter().filter_map(|n| (syn.binds)(*n, src).map(|(x, v)| (*n, x, v))).collect();
    let loops: Vec<(TsNode, &str, Option<&str>, TsNode)> =
        nodes.iter().filter_map(|n| (syn.loops)(*n, src).map(|(x, v, b)| (*n, x, v, b))).collect();
    let own = |s: TsNode| s.parent().into_iter().chain(s.parent().and_then(|p| p.parent())).any(|p| p.id() == body.id());
    let (mut name, mut at) = (name, at);
    // each step goes back to where a loop or a statement before `at` binds the name, so it ends
    'follow: loop {
        // the innermost loop around the change that binds the name, where its body binds it nowhere anew
        let around = loops
            .iter()
            .filter(|(_, x, _, b)| {
                *x == name
                    && b.byte_range().contains(&at)
                    && !binds.iter().any(|(s, y, _)| *y == name && b.byte_range().contains(&s.start_byte()))
            })
            .max_by_key(|(l, ..)| l.start_byte());
        if let Some((l, _, over, _)) = around {
            name = (*over)?;
            at = l.start_byte();
            continue;
        }
        let mut before: Vec<&(TsNode, &str, Vec<&str>)> =
            binds.iter().filter(|(s, x, _)| *x == name && own(*s) && s.end_byte() <= at).collect();
        before.sort_by_key(|(s, ..)| s.start_byte());
        for (s, _, held) in before.into_iter().rev() {
            if held.contains(&name) {
                continue;
            }
            name = *held.first()?;
            at = s.start_byte();
            continue 'follow;
        }
        return Some(name);
    }
}

/// What the names of a body hold, binding by binding: each binding is visible over a stretch of the
/// source, and a use takes the innermost binding visible where it stands. A name bound twice (a
/// loop's `b`, then `let b = Body::new()`) is two values, as two names would be. Where a binding is
/// visible is the language's to say.
pub struct Bindings<T> {
    all: Vec<(String, usize, usize, T)>,
}

impl<T> Default for Bindings<T> {
    fn default() -> Self {
        Bindings { all: vec![] }
    }
}

impl<T> Bindings<T> {
    /// A binding of `name` to `value`, visible over the bytes `from..to`.
    pub fn bind(&mut self, name: &str, from: usize, to: usize, value: T) {
        self.all.push((name.to_string(), from, to, value));
    }

    /// The index of the binding of `name` visible at `at`: of those that hold it, the one visible
    /// from the latest byte, which is the innermost.
    fn at(&self, name: &str, at: usize) -> Option<usize> {
        (0..self.all.len())
            .filter(|&i| self.all[i].0 == name && self.all[i].1 <= at && at < self.all[i].2)
            .max_by_key(|&i| (self.all[i].1, i))
    }

    /// Where the binding of `name` visible at `at` is visible from: which binding it is.
    pub fn which(&self, name: &str, at: usize) -> Option<usize> {
        self.at(name, at).map(|i| self.all[i].1)
    }

    /// What the binding of `name` visible at `at` holds.
    pub fn get(&self, name: &str, at: usize) -> Option<&T> {
        self.at(name, at).map(|i| &self.all[i].3)
    }

    /// The binding of `name` visible at `at`: the byte it is visible from, and what it holds.
    pub fn binding(&self, name: &str, at: usize) -> Option<(usize, &T)> {
        self.at(name, at).map(|i| (self.all[i].1, &self.all[i].3))
    }

    /// Whether `name` holds a value at `at`.
    pub fn holds(&self, name: &str, at: usize) -> bool {
        self.at(name, at).is_some()
    }

    /// What the binding of `name` visible at `at` holds, to change it.
    pub fn get_mut(&mut self, name: &str, at: usize) -> Option<&mut T> {
        self.at(name, at).map(|i| &mut self.all[i].3)
    }

    /// Every binding, by its name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &T)> {
        self.all.iter().map(|(n, _, _, v)| (n.as_str(), v))
    }

    /// Every binding, by its name and the byte it is visible from.
    pub fn all(&self) -> impl Iterator<Item = (&str, usize, &T)> {
        self.all.iter().map(|(n, from, _, v)| (n.as_str(), *from, v))
    }

    /// Every name visible at `at`, with what it holds there.
    pub fn visible(&self, at: usize) -> HashMap<String, T>
    where
        T: Clone,
    {
        let mut out = HashMap::new();
        for (name, ..) in &self.all {
            if let Some(v) = self.get(name, at) {
                out.entry(name.clone()).or_insert_with(|| v.clone());
            }
        }
        out
    }
}

fn line(c: TsNode) -> u32 {
    c.start_position().row as u32 + 1
}

/// Code as words on a picture: one line, at most 60 characters, without the parentheses a
/// language puts around a condition (`if (!existsSync(raw))`).
fn worded(code: &str) -> String {
    let mut one: String = code.split_whitespace().collect::<Vec<_>>().join(" ");
    while one.starts_with('(') && one.ends_with(')') && closes_at_end(&one) {
        one = one[1..one.len() - 1].trim().to_string();
    }
    if one.chars().count() <= 60 {
        one
    } else {
        one.chars().take(59).collect::<String>() + "…"
    }
}

fn span(c: TsNode) -> (u32, u32) {
    (c.start_byte() as u32, c.end_byte() as u32)
}

/// Whether the parenthesis that opens `s` is the one that closes it.
fn closes_at_end(s: &str) -> bool {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return i == s.len() - 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `fs::copy(a, b)`, `shutil.move(src, dst)`, `os.replace(a, b)` and `copyFile(a, b)` all read
    /// their first path and write their second.
    #[test]
    fn a_move_or_a_copy_reads_its_first_path_and_writes_its_second() {
        let both = Some([Crossing::read(World::Disk), Crossing::write(World::Disk)]);
        for op in ["copy", "copyfile", "copyFile", "cpSync", "rename", "renameSync", "move", "replace"] {
            assert_eq!(sides(op, 2), both, "{op}");
        }
        assert_eq!(sides("copy", 1), None, "one path is the operation's own way");
        assert_eq!(sides("write_text", 2), None);
    }

    /// The error a `Result` names is its second argument, at the top level of the brackets; a result
    /// that names none apart from its value (`io::Result<T>`) names none.
    #[test]
    fn the_error_a_result_names_is_its_second_argument() {
        assert_eq!(error_written("Result<Vec<Expense>, Error>"), Some("Error"));
        assert_eq!(error_written("Result<HashMap<String, f64>, crate::Error>"), Some("crate::Error"));
        assert_eq!(error_written("Result<(), Box<dyn Error>>"), Some("Box<dyn Error>"));
        assert_eq!(error_written("io::Result<()>"), None);
        assert_eq!(error_written("Option<u32>"), None);
    }

    /// A body's index answers as its tree does: the node each node stands in is tree-sitter's parent, the named nodes
    /// before it are its previous named siblings, and the names it keeps are the walk's, also by what each writes.
    #[test]
    fn the_index_of_a_body_answers_as_its_tree_does() {
        let ids = |ns: &[TsNode]| ns.iter().map(|n| n.id()).collect::<Vec<_>>();
        let names = ["identifier"];
        for (tree, code) in crate::tree::tests::trees() {
            let src = code.as_bytes();
            let mut bodies = vec![tree.root_node()];
            bodies.extend(walk_all(tree.root_node()).into_iter().filter(|n| n.child_by_field_name("body").is_some()));
            for body in bodies {
                let index = Index::new(body, src, &names);
                let (mut cursor, mut stack, mut under) = (body.walk(), vec![body], vec![]);
                while let Some(n) = stack.pop() {
                    let children: Vec<TsNode> = n.children(&mut cursor).collect();
                    under.extend(children.iter().copied());
                    stack.extend(children);
                }
                for n in under {
                    let (p, at) = index.parents[&n.id()];
                    assert_eq!(Some(p.id()), n.parent().map(|p| p.id()), "{code}: {}", n.kind());
                    let before: Vec<TsNode> = index.named.get(&p.id()).map_or(vec![], |all| all[..at].iter().rev().copied().collect());
                    let siblings: Vec<TsNode> = std::iter::successors(n.prev_named_sibling(), |s| s.prev_named_sibling()).collect();
                    assert_eq!(ids(&before), ids(&siblings), "{code}: {}", n.kind());
                }
                assert_eq!(ids(&index.names), ids(&walk(body, &names)), "{code}");
                for (name, at) in &index.written {
                    let of: Vec<TsNode> = walk(body, &names).into_iter().filter(|n| text(*n, src) == *name).collect();
                    assert_eq!(ids(at), ids(&of), "{code}: {name}");
                }
            }
        }
    }

    /// `for b in bases { resolve(b) }` then `let b = Body::new()`: the use in the loop takes the
    /// loop's `b`, a use after it the later binding, and a use before both neither.
    #[test]
    fn a_use_takes_the_innermost_binding_visible_where_it_stands() {
        let mut b = Bindings::default();
        b.bind("b", 10, 40, "String");
        b.bind("b", 50, 100, "Body");
        b.bind("b", 60, 70, "Leaf");
        assert_eq!(b.get("b", 5), None);
        assert_eq!(b.get("b", 20), Some(&"String"));
        assert_eq!(b.get("b", 45), None);
        assert_eq!(b.get("b", 55), Some(&"Body"));
        assert_eq!(b.get("b", 65), Some(&"Leaf"));
        assert_eq!(b.get("b", 80), Some(&"Body"));
        assert_eq!(b.visible(65).get("b"), Some(&"Leaf"));
    }
}
