//! A `Graph` under construction: what a language module fills while it parses, with the
//! scratch it needs for resolution. It never leaves `Language::parse`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use indexmap::IndexMap;

use crate::model::{
    Bucket, Dir, Edge, EdgeKind, Entry, EntryKind, Graph, Held, Lang, Node, NodeKind, Signature, Site, Slot, Stats, Touch, Unresolved,
    World,
};

/// Per-class facts that only matter during resolution.
#[derive(Default, Clone, Debug)]
pub struct ClassInfo {
    pub bases: Vec<Vec<String>>,
    pub resolved: Vec<String>,
    pub attr_types: BTreeMap<String, String>,
    /// The library types its fields hold, by field: `conn` holds `ext:rusqlite.Connection`.
    pub foreign: BTreeMap<String, String>,
    pub fields: Vec<(String, Vec<Vec<String>>)>,
    /// For a sum (a Rust `enum`): the types its variants carry.
    pub variants: Vec<Vec<String>>,
    /// For a Rust `enum`, each variant that carries data: its name, the types it names, and where
    /// it is written.
    pub arms: Vec<(String, Vec<Vec<String>>, Option<(u32, u32)>)>,
}

/// What the program's classes say of one another: the methods each method overrides, each with
/// the method's parameters named as the base's at the same place, and the classes each class
/// inherits from.
#[derive(Default)]
pub struct Family {
    pub overrides: HashMap<String, Vec<(String, Vec<(String, String)>)>>,
    pub bases: HashMap<String, Vec<String>>,
}

impl Family {
    /// Whether `class` is `of` or inherits from it.
    pub fn is_a(&self, class: &str, of: &str) -> bool {
        let (mut todo, mut seen) = (vec![class], HashSet::new());
        while let Some(c) = todo.pop() {
            if c == of {
                return true;
            }
            if seen.insert(c) {
                todo.extend(self.bases.get(c).into_iter().flatten().map(String::as_str));
            }
        }
        false
    }
}

/// A union a signature writes in place (`Fit | Failed`, `Union[Fit, Failed]`): the names of what
/// it may be, and how it is written.
#[derive(Default, Clone, Debug)]
pub struct Union {
    pub names: Vec<Vec<String>>,
    pub written: String,
}

#[derive(Default, Clone, Debug)]
pub struct SigInfo {
    pub ptypes: Vec<Vec<String>>,
    pub rtypes: Vec<Vec<String>>,
    /// The unions its parameters and its result write in place, read apart from `ptypes` and
    /// `rtypes`: each is a sum when it may be two or more known types.
    pub punions: Vec<Union>,
    pub runions: Vec<Union>,
}

impl SigInfo {
    /// The types of another signature of the same function as well.
    pub fn extend(&mut self, other: SigInfo) {
        let SigInfo { ptypes, rtypes, punions, runions } = other;
        self.ptypes.extend(ptypes);
        self.rtypes.extend(rtypes);
        self.punions.extend(punions);
        self.runions.extend(runions);
    }
}

/// What one declaration of a function's name is, among the declarations of that name in one
/// scope.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Declared {
    /// What runs: the body a call runs, or the getter a read of a property runs.
    Body,
    /// A signature its callers are checked against, apart from the body that runs: an overload.
    Overload,
    /// What runs when a property is assigned or deleted: a setter, a deleter.
    Writer,
}

/// The nodes and edges found so far, and the tables that resolve names into them.
#[derive(Default)]
pub struct GraphBuilder {
    pub nodes: IndexMap<String, Node>,
    pub edges: Vec<Edge>,
    /// The edges by source, target and kind, and for a change by the input it changes.
    pub edge_set: HashSet<(String, String, EdgeKind, Option<String>)>,
    pub classes: HashMap<String, ClassInfo>,
    pub sigs: HashMap<String, SigInfo>,
    /// The calls not followed: by the name each is written with, and by what that name is bound to.
    pub unresolved: HashMap<String, u32>,
    pub unresolved_by: Unresolved,
    pub files: BTreeMap<String, String>,
    /// Module id -> local name -> fully qualified target, kept for re-export lookups.
    pub bindings: HashMap<String, HashMap<String, String>>,
    /// Methods that change their receiver in place.
    pub mutating: HashSet<String>,
    /// Calls whose caller uses what they give back, as (caller, callee).
    pub used: HashSet<(String, String)>,
}

impl GraphBuilder {
    pub fn add_node(&mut self, mut n: Node) {
        if n.prophecy.is_none() {
            n.prophecy = crate::model::Prophecy::among(&n.decorators);
        }
        self.nodes.insert(n.id.clone(), n);
    }

    /// One declaration of the function `n` names, with the types its signature names. A name
    /// declared more than once in one scope is one node, the declaration that runs: a call's body,
    /// with the overloads declared with it as the signatures its callers see and the types they
    /// name as its own; a property's getter, which is what a reader of the property gets. Until
    /// that declaration comes, the first of the others stands for it. A body declared again
    /// replaces the one before, as the name is bound to the last.
    pub fn declare(&mut self, mut n: Node, sig: SigInfo, declared: Declared) {
        let signature = |n: &Node| Signature { line: n.span.map_or(1, |s| s.0), params: n.params.clone(), returns: n.returns.clone() };
        let Some(old) = self.nodes.get_mut(&n.id) else {
            if declared == Declared::Overload {
                n.overloads = vec![signature(&n)];
            }
            self.sigs.insert(n.id.clone(), sig);
            self.add_node(n);
            return;
        };
        match declared {
            Declared::Writer => {}
            Declared::Overload => {
                old.overloads.push(signature(&n));
                self.sigs.entry(n.id).or_default().extend(sig);
            }
            Declared::Body => {
                n.overloads = std::mem::take(&mut old.overloads);
                match self.sigs.get_mut(&n.id).filter(|_| !n.overloads.is_empty()) {
                    Some(theirs) => theirs.extend(sig),
                    None => {
                        self.sigs.insert(n.id.clone(), sig);
                    }
                }
                self.add_node(n);
            }
        }
    }

    pub fn add_edge(&mut self, src: &str, dst: &str, kind: EdgeKind, via: Option<String>) {
        self.add_call(src, dst, kind, via, vec![]);
    }

    /// The sum a union written in place is, of two or more resolved `variants`: a union of the
    /// same types is the same type however and wherever it is written, so it is the sum the
    /// program names for them when it names one (`Result = Fit | Failed`), else one node of its
    /// own, standing where `at` first writes it.
    pub fn written_sum(&mut self, variants: &[String], written: &str, at: &Node) -> String {
        let mut key = variants.to_vec();
        key.sort();
        key.dedup();
        let mut sums: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for e in self.edges.iter().filter(|e| e.kind == EdgeKind::Variant) {
            sums.entry(e.src.as_str()).or_default().push(e.dst.as_str());
        }
        for vs in sums.values_mut() {
            vs.sort();
        }
        let known = sums.iter().find(|(s, vs)| {
            self.nodes.get(**s).map_or(false, |n| n.kind == NodeKind::Class)
                && vs.len() == key.len()
                && vs.iter().zip(&key).all(|(v, k)| v == k)
        });
        if let Some((id, _)) = known {
            return id.to_string();
        }
        let id = format!("({})", key.join(" | "));
        self.add_node(Node {
            id: id.clone(),
            kind: NodeKind::Class,
            name: written.split_whitespace().collect::<Vec<_>>().join(" "),
            file: at.file.clone(),
            span: at.span,
            module: at.module.clone(),
            ..Default::default()
        });
        self.add_edge(&at.module, &id, EdgeKind::Contains, None);
        for v in &key {
            if v.starts_with("ext:") {
                self.ext_node(v);
            }
            self.add_edge(&id, v, EdgeKind::Variant, None);
        }
        id
    }

    /// An edge from a body, with the arms of the choices it sits in; the first call of a
    /// target is the one kept. A change is one edge per input it changes: changing `self` and
    /// changing `other`, both Graphs, are two.
    pub fn add_call(&mut self, src: &str, dst: &str, kind: EdgeKind, via: Option<String>, arms: Vec<(u32, u32)>) {
        if src == dst {
            return;
        }
        let input = via.clone().filter(|_| kind == EdgeKind::Mutates);
        let key = (src.to_string(), dst.to_string(), kind, input);
        if self.edge_set.insert(key) {
            self.edges.push(Edge { src: src.into(), dst: dst.into(), kind, via, weight: None, arms, uses: false });
        }
    }

    /// A name a package exports, as a route into it: a function or a class, by its label, unless
    /// it is already a route of another kind.
    pub fn export(&mut self, id: &str, label: String) {
        if let Some(n) = self.nodes.get_mut(id).filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Class) && n.entry.is_none()) {
            n.entry = Some(crate::model::Entry::new(crate::model::EntryKind::Export, label));
        }
    }

    /// A call the reader could not follow, by the name it is written with (its first two parts)
    /// and what the name it starts from is bound to.
    pub fn not_followed(&mut self, name: String, bucket: Bucket) {
        *self.unresolved.entry(name).or_insert(0) += 1;
        self.unresolved_by.count(bucket);
    }

    /// Record a boundary call on a node; the same call again counts up.
    /// A call as it was read, for the routes that reach none of the callers that make it whole.
    pub fn instead(&mut self, id: &str, t: Touch) {
        let t = self.owned(id, t);
        let Some(n) = self.nodes.get_mut(id) else { return };
        if !n.instead.iter().any(|x| x.kind == t.kind && x.what == t.what && x.op == t.op && x.line == t.line) {
            n.instead.push(Touch { n: 1, by: BTreeSet::new(), users_of: vec![], ..t });
        }
    }

    /// The classes the program's classes inherit from, and the methods each method overrides
    /// through them, in the order of its class's method resolution order, each with the method's
    /// parameters named as the base's at the same place.
    pub fn family(&self) -> Family {
        let mut bases: HashMap<String, Vec<String>> = HashMap::new();
        for e in self.edges.iter().filter(|e| e.kind == EdgeKind::Inherits) {
            bases.entry(e.src.clone()).or_default().push(e.dst.clone());
        }
        let mut overrides: HashMap<String, Vec<(String, Vec<(String, String)>)>> = HashMap::new();
        {
            let declared = |c: &str| -> Vec<&str> { bases.get(c).into_iter().flatten().map(String::as_str).collect() };
            let mut known = HashMap::new();
            for n in self.nodes.values().filter(|n| n.kind == NodeKind::Method) {
                let Some(cls) = n.cls.as_deref() else { continue };
                for b in crate::model::linearization(cls, &declared, &mut known).into_iter().skip(1) {
                    let base = format!("{b}.{}", n.name);
                    if let Some(m) = self.nodes.get(&base).filter(|m| m.kind == NodeKind::Method) {
                        let named = n.params.iter().zip(&m.params).map(|(own, theirs)| (own.name.clone(), theirs.name.clone())).collect();
                        overrides.entry(n.id.clone()).or_default().push((base, named));
                    }
                }
            }
        }
        Family { overrides, bases }
    }

    /// A request `id` makes whole that no route of its part serves.
    pub fn ask(&mut self, id: &str, request: String) {
        let Some(n) = self.nodes.get_mut(id) else { return };
        if !n.asks.contains(&request) {
            n.asks.push(request);
        }
    }

    /// The name a function goes by in the names of its values: a constructor's is its class's
    /// (`Store`), a method's its class's and its own (`Repo.read`), any other function's its own.
    pub fn owner(&self, id: &str) -> Option<String> {
        let n = self.nodes.get(id)?;
        let class = n.cls.as_deref().map(|c| c.rsplit('.').next().unwrap_or(c));
        Some(match (n.ctor, class) {
            (true, Some(c)) => c.to_string(),
            (false, Some(c)) => format!("{c}.{}", n.name),
            _ => n.name.clone(),
        })
    }

    /// A function's own touch of a file the code names only by a name of the function's
    /// (`fs::write(&file, ..)`), named with the function's (`Settings.keep.file`), as a parameter's
    /// value is: two functions' `file`s are two places.
    fn owned(&self, id: &str, t: Touch) -> Touch {
        // a name, also as a reference to it (`&file`); an expression names more than a value
        let name = t.what.trim_start_matches('&');
        let plain =
            name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') && name.chars().all(|c| c.is_alphanumeric() || c == '_');
        if t.kind != World::Disk || !t.artifact.is_empty() || !plain {
            return t;
        }
        match self.owner(id) {
            Some(owner) => Touch { what: format!("{owner}.{name}"), ..t },
            None => t,
        }
    }

    pub fn touch(&mut self, id: &str, t: Touch) {
        let t = self.owned(id, t);
        let Some(n) = self.nodes.get_mut(id) else { return };
        // only what is read can be a check
        let t = Touch { checks: t.checks && t.dir == Some(Dir::Read), ..t };
        match n
            .touches
            .iter_mut()
            .find(|x| x.kind == t.kind && x.what == t.what && x.op == t.op && x.artifact == t.artifact && x.dir == t.dir)
        {
            Some(x) => {
                x.n += 1;
                // the touch runs on the line of every call it stands for
                for l in std::iter::once(t.line).chain(t.also_at.iter().copied()) {
                    if l != x.line && !x.also_at.contains(&l) {
                        x.also_at.push(l);
                    }
                }
                // one call that uses what it reads makes the touch a use
                x.checks &= t.checks;
                // whole in its own function is whole in every route; else in the routes of each
                let whole_here = |y: &Touch| y.by.is_empty() && y.users_of.is_empty();
                let here = whole_here(x) || whole_here(&t);
                // a place is decided where any of the calls that touch it decides it
                x.fixed |= t.fixed;
                for f in t.fixed_by {
                    if !x.fixed_by.contains(&f) {
                        x.fixed_by.push(f);
                    }
                }
                if x.payload.is_empty() {
                    x.payload = t.payload;
                }
                if here {
                    x.by.clear();
                    x.users_of.clear();
                } else {
                    x.by.extend(t.by);
                    for e in t.users_of {
                        if !x.users_of.contains(&e) {
                            x.users_of.push(e);
                        }
                    }
                }
            }
            None => n.touches.push(Touch { n: 1, ..t }),
        }
    }

    /// The graph, once every file is parsed: the scratch is dropped, the names that did not
    /// resolve are counted.
    pub fn finish(self, root: String, project_root: Option<&std::path::Path>, language: Lang) -> Graph {
        let mut top: Vec<(String, u32)> = self.unresolved.iter().map(|(k, v)| (k.clone(), *v)).collect();
        let mut nodes = self.nodes;
        for (id, info) in &self.classes {
            if let Some(n) = nodes.get_mut(id).filter(|n| n.kind == NodeKind::Class) {
                for (f, _) in &info.fields {
                    if !n.fields.contains(f) {
                        n.fields.push(f.clone());
                    }
                }
                n.holds = info.attr_types.clone();
            }
        }
        serve_data(&mut nodes);
        settle_fails(&mut nodes, &self.edges);
        crate::flow::wire(&mut nodes, &self.edges);
        fill_nested(&mut nodes);
        settle_parameters(&mut nodes, &self.edges);
        mark_doors(&mut nodes, &self.edges);
        mark_options(&mut nodes);
        crate::prophecy::settle(&mut nodes, &self.edges);
        let mut edges = self.edges;
        for e in edges.iter_mut().filter(|e| e.kind == EdgeKind::Calls) {
            e.uses = self.used.contains(&(e.src.clone(), e.dst.clone()));
        }
        top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut g = Graph {
            root,
            language: language.into(),
            project_root: project_root.map(crate::tree::path_str).unwrap_or_default(),
            nodes: nodes.into_values().collect(),
            edges,
            files: self.files,
            stats: Stats {
                unresolved_calls_total: self.unresolved.values().sum(),
                unresolved_calls_top: top,
                unresolved_calls: self.unresolved_by,
                ..Default::default()
            },
            ..Default::default()
        };
        g.recount();
        g
    }

    /// The method `name` a value of class `cls` runs: the first along its method resolution order
    /// that defines it, the class's own before a base's (a trait's provided method, an inherited
    /// one).
    pub fn method_of(&self, cls: &str, name: &str) -> Option<String> {
        self.order(cls).into_iter().map(|c| format!("{c}.{name}")).find(|m| self.nodes.contains_key(m))
    }

    /// The initializer building a value of class `cls` runs (`model::initializer_along`): what a
    /// call of the class hands its arguments to.
    pub fn initializer_of(&self, cls: &str) -> Option<String> {
        crate::model::initializer_along(&self.order(cls), |id| self.nodes.get(id))
    }

    /// The method resolution order of `cls` along the program's own classes.
    fn order<'a>(&'a self, cls: &'a str) -> Vec<&'a str> {
        let bases = |c: &'a str| -> Vec<&'a str> {
            self.classes.get(c).into_iter().flat_map(|i| &i.resolved).map(String::as_str).filter(|b| !b.starts_with("ext:")).collect()
        };
        crate::model::linearization(cls, &bases, &mut HashMap::new())
    }

    /// Library code read as code: every node of a library module is the library's, and no route
    /// into the program. A name outside the code is no code of its own.
    pub fn mark_library(&mut self) {
        let library: HashSet<String> = self
            .nodes
            .values()
            .filter(|n| n.kind != NodeKind::External && (n.library || self.nodes.get(&n.module).map_or(false, |m| m.library)))
            .map(|n| n.id.clone())
            .collect();
        for id in &library {
            if let Some(n) = self.nodes.get_mut(id) {
                n.library = true;
                n.entry = None;
            }
        }
    }

    pub fn ext_node(&mut self, eid: &str) {
        if self.nodes.contains_key(eid) {
            return;
        }
        // a shaped type keeps its shape in the name: `jaxtyping` / `Float[Tensor, batch dim]`
        let full = &eid[4..];
        let cut = full.find('[').unwrap_or(full.len());
        let (module, name) = match full[..cut].rsplit_once('.') {
            Some((module, name)) => (module, format!("{name}{}", &full[cut..])),
            None => ("", full.to_string()),
        };
        let package = module.split(['.', ':']).next().unwrap_or("");
        self.add_node(Node {
            id: eid.into(),
            kind: NodeKind::External,
            name,
            module: module.into(),
            door: crate::effects::library_place(package),
            ..Default::default()
        });
    }
}

/// The parameters of no type of the code's that hold a value of the plan (`mathematics.md`, "One fine plan"): those a call
/// fills with a value, a step's result, a projection of one or a parameter of its caller's that holds one, in each
/// function the call may run (`flow::Runs`): a call into a family fills the parameters of every override it may run, and
/// a call through a class those of the method it names alone. A literal fills a parameter with none, and a function no
/// call of the program fills (an entry, a door) is handed none by what calls it from outside. A value a body hands or
/// reads by such a parameter's name (`Held::param`, `Use::param`) is the parameter: where it holds none, the body is
/// handed and reads nothing of it.
fn settle_parameters(nodes: &mut IndexMap<String, Node>, edges: &[Edge]) {
    let args = |n: &Node| -> Vec<String> { n.params.iter().filter(|p| !p.receiver).map(|p| p.name.clone()).collect() };
    let runs = crate::flow::Runs::new(nodes, edges);
    // the places a slot of a call fills among the callee's parameters after its receiver
    let places = |callee: &Node, slot: &Slot, written: &[Slot]| -> Vec<usize> {
        let ps: Vec<&crate::model::Param> = callee.params.iter().filter(|p| !p.receiver).collect();
        let at = |n: usize| ps.iter().position(|p| p.rest).filter(|r| *r <= n).or((n < ps.len()).then_some(n));
        let named = |k: &str| ps.iter().position(|p| p.name == k && !p.rest).or_else(|| ps.iter().rposition(|p| p.rest));
        let by: Vec<usize> = written
            .iter()
            .filter_map(|w| match w {
                Slot::At(n) => at(*n as usize),
                Slot::Name(k) => named(k),
                _ => None,
            })
            .collect();
        match slot {
            Slot::At(n) => at(*n as usize).into_iter().collect(),
            Slot::Name(k) => named(k).into_iter().collect(),
            Slot::Spread(n) => (*n as usize..ps.len()).filter(|i| !by.contains(i) || ps[*i].rest).collect(),
            Slot::Keywords => (0..ps.len()).filter(|i| !by.contains(i)).collect(),
            Slot::Receiver => vec![],
        }
    };
    // each call fills a parameter with a value, or with one of its caller's own parameters, which holds one where a call of
    // the caller fills it so: to the fixpoint
    let mut filled: HashSet<(String, usize)> = HashSet::new();
    loop {
        let before = filled.len();
        for n in nodes.values() {
            let mine = args(n);
            for s in &n.sites {
                for callee in runs.of(s).iter().filter_map(|c| nodes.get(c)) {
                    for h in &s.hands {
                        let theirs = h.param.then(|| mine.iter().position(|p| *p == h.name)).flatten();
                        if theirs.is_some_and(|i| !filled.contains(&(n.id.clone(), i))) {
                            continue;
                        }
                        for p in h.fills.iter().flat_map(|slot| places(callee, slot, &s.args)) {
                            filled.insert((callee.id.clone(), p));
                        }
                    }
                }
            }
        }
        if filled.len() == before {
            break;
        }
    }
    let holds = |id: &str, name: &str| -> bool {
        let Some(i) = nodes.get(id).map(&args).and_then(|ps| ps.iter().position(|p| p == name)) else { return true };
        filled.contains(&(id.to_string(), i))
    };
    let empty: HashMap<String, HashSet<String>> = nodes
        .values()
        .map(|n| {
            let names = n.sites.iter().flat_map(|s| &s.hands).filter(|h| h.param).map(|h| h.name.clone());
            let used = n.uses.iter().filter(|u| u.param).map(|u| u.name.clone());
            (n.id.clone(), names.chain(used).filter(|nm| !holds(&n.id, nm)).collect::<HashSet<String>>())
        })
        .filter(|(_, e)| !e.is_empty())
        .collect();
    for (id, names) in empty {
        let Some(n) = nodes.get_mut(&id) else { continue };
        // what a step makes of those parameters alone (a loop's element) holds no value either, and a library's operation
        // that makes nothing of nothing is no step
        n.sites.retain_mut(|s| {
            s.hands.retain(|h| !(h.param && names.contains(&h.name)));
            let made = s.gives.iter().any(|g| g.param);
            if s.hands.is_empty() {
                s.gives.retain(|g| !g.param);
            }
            !(made && s.gives.is_empty() && s.hands.is_empty() && s.to.is_empty())
        });
        n.uses.retain(|u| !(u.param && names.contains(&u.name)));
        for s in n.sets.iter_mut() {
            s.from.retain(|h| !(h.param && names.contains(&h.name)));
        }
    }
}

/// What a step written inside a call's argument gives fills that argument's parameter (`Held::fills`),
/// once the steps are wired and the call is handed what the step gives: `Trend::new(s).save(..)` hands
/// what `Trend.new` gives for the value `save` runs on, `f(g(y))` what `g` gives for `f`'s first. A
/// call that is no method of the program's runs on no value it is not handed: what is written before
/// the name of a function or a class is where it lies (`models.Store(..)`, `pd.read_csv(..)`).
fn fill_nested(nodes: &mut IndexMap<String, Node>) {
    let inside = |inner: (u32, u32), outer: (u32, u32)| inner != outer && inner != (0, 0) && outer.0 <= inner.0 && inner.1 <= outer.1;
    let methods: HashSet<String> = nodes.values().filter(|n| n.kind == NodeKind::Method).map(|n| n.id.clone()).collect();
    for n in nodes.values_mut() {
        let sites = &mut n.sites;
        // what a step in a function written in a call's arguments gives under a name the function binds is the function's:
        // the call is handed what the function gives back (`query_map(.., |row| { let amount = Money { .. }; Ok(..) })`)
        for o in 0..sites.len() {
            let span = sites[o].span;
            let theirs: Vec<Held> = sites
                .iter()
                .filter(|s| s.in_function && inside(s.span, span))
                .flat_map(|s| s.gives.iter().filter(|g| !g.name.is_empty() && !g.name.starts_with('#')).cloned())
                .collect();
            sites[o].hands.retain(|h| !theirs.contains(h));
        }
        for o in 0..sites.len() {
            for k in 0..sites[o].hands.len() {
                if !sites[o].hands[k].fills.is_empty() {
                    continue;
                }
                let (span, h) = (sites[o].span, &sites[o].hands[k]);
                let Some(at) = sites.iter().find(|s| inside(s.span, span) && s.gives.contains(h)).map(|s| s.span.0) else { continue };
                let slots: Vec<Slot> =
                    sites[o].written.iter().zip(&sites[o].args).filter(|((a, b), _)| *a <= at && at < *b).map(|(_, s)| s.clone()).collect();
                sites[o].hands[k].fills = slots;
            }
        }
        for s in sites.iter_mut().filter(|s| !methods.contains(&s.to) && !s.hands.iter().any(|h| h.fills.contains(&Slot::Receiver))) {
            if let Some(j) = s.args.iter().position(|a| *a == Slot::Receiver) {
                s.args.remove(j);
                s.written.remove(j);
            }
        }
    }
}

/// Mark the classes that are doors: one of their operations reaches a service, through what it
/// calls. An operation is a method called on a value of the class, not on the class: an
/// alternative constructor (`Config.from_yaml`) reads into a value and leaves it data.
fn mark_doors(nodes: &mut IndexMap<String, Node>, edges: &[Edge]) {
    let mut out: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for e in edges.iter().filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::Instantiates)) {
        out.entry(e.src.as_str()).or_default().push(e.dst.as_str());
    }
    let operation = |n: &Node| n.kind == NodeKind::Method && !n.on_class;
    let reaches = |start: &str| -> Option<World> {
        let mut seen = HashSet::from([start.to_string()]);
        let mut stack = vec![start.to_string()];
        while let Some(id) = stack.pop() {
            let Some(n) = nodes.get(&id) else { continue };
            if let Some(t) = n.touches.iter().find(|t| t.kind.is_service()) {
                return Some(t.kind);
            }
            let next = match n.kind {
                NodeKind::Class => n.initializer(|m| nodes.get(m)).into_iter().collect::<Vec<String>>(),
                _ => out.get(id.as_str()).into_iter().flatten().map(|d| d.to_string()).collect(),
            };
            for d in next {
                if seen.insert(d.clone()) {
                    stack.push(d);
                }
            }
        }
        None
    };
    let doors: Vec<(String, World)> = nodes
        .values()
        .filter(|n| n.kind == NodeKind::Class)
        .filter_map(|c| {
            let prefix = format!("{}.", c.id);
            nodes
                .values()
                .filter(|m| m.cls.as_deref() == Some(c.id.as_str()) || m.id.starts_with(&prefix) && m.kind == NodeKind::Method)
                .filter(|m| operation(m))
                .find_map(|m| reaches(&m.id))
                .map(|k| (c.id.clone(), k))
        })
        .collect();
    for (id, kind) in doors {
        if let Some(n) = nodes.get_mut(&id) {
            n.door = Some(kind);
        }
    }
}

/// An entry run from the command line (a command, a script) is handed what the human types: the
/// options its own declaration names (a command's parameters), else those of the parses of the
/// command line its route runs, in the order it runs them.
fn mark_options(nodes: &mut IndexMap<String, Node>) {
    let run = |e: &Entry| matches!(e.kind, EntryKind::Cli | EntryKind::Script) && e.options.is_empty();
    let mut found = vec![];
    for n in nodes.values().filter(|n| n.entry.as_ref().map_or(false, run)) {
        let mut options = vec![];
        parsed(nodes, &n.id, &mut HashSet::new(), &mut options);
        if !options.is_empty() {
            found.push((n.id.clone(), options));
        }
    }
    for (id, options) in found {
        if let Some(e) = nodes.get_mut(&id).and_then(|n| n.entry.as_mut()) {
            e.options = options;
        }
    }
}

/// The options of the parses of the command line a function runs, in its own steps or through
/// the functions they run, in the order they run; a class runs its initializer.
fn parsed(nodes: &IndexMap<String, Node>, id: &str, seen: &mut HashSet<String>, out: &mut Vec<String>) {
    if !seen.insert(id.to_string()) {
        return;
    }
    let Some(n) = nodes.get(id) else { return };
    if n.kind == NodeKind::Class {
        if let Some(i) = n.initializer(|m| nodes.get(m)) {
            parsed(nodes, &i, seen, out);
        }
        return;
    }
    for s in &n.sites {
        match &s.command_line {
            Some(options) => {
                for o in options {
                    if !out.contains(o) {
                        out.push(o.clone());
                    }
                }
            }
            None if !s.to.is_empty() => parsed(nodes, &s.to, seen, out),
            None => {}
        }
    }
}

/// The error each call hands on to its body's border where the language hands on every error a call
/// fails with (`Site::fails`), settled once every body is read, since it needs what the callee's body
/// does: what the callee's body ends with uncaught, as a `raise` propagates. A body ends with what it
/// raises, by the class of the error it builds, and with what its calls hand on, each where no handler
/// around it catches it. A handler catches the classes it names and those below them, and one that
/// names none catches whatever is raised. A call that `?` hands on fails as its reader said (`Steps`).
fn settle_fails(nodes: &mut IndexMap<String, Node>, edges: &[Edge]) {
    let bare = |id: &str| id.trim_start_matches("ext:").rsplit(['.', ':']).next().unwrap_or("").to_string();
    let mut bases: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in edges.iter().filter(|e| e.kind == EdgeKind::Inherits) {
        bases.entry(e.src.as_str()).or_default().push(e.dst.as_str());
    }
    // the classes an error is of, itself first: what a handler that names one of them catches
    let kinds = |error: &str| -> Vec<String> {
        let (mut out, mut todo) = (vec![], vec![error]);
        while let Some(k) = todo.pop() {
            if !out.contains(&bare(k)) {
                out.push(bare(k));
                todo.extend(bases.get(k).into_iter().flatten());
            }
        }
        out
    };
    let caught = |handlers: &[Vec<String>], error: &str| {
        let of = kinds(error);
        handlers.iter().any(|h| h.is_empty() || h.iter().any(|n| of.contains(&bare(n))))
    };
    // the functions a call may run, one of which runs (`flow::Runs`)
    let runs = crate::flow::Runs::new(nodes, edges);
    // the errors a call hands on: each that what it runs can end with, uncaught around the call
    let handed = |s: &Site, ends: &HashMap<String, Vec<String>>| -> Vec<String> {
        if !s.handed_on || !s.fails.is_empty() {
            return s.fails.clone();
        }
        let errors: BTreeSet<String> =
            runs.of(s).iter().flat_map(|f| ends.get(f).into_iter().flatten()).filter(|e| !caught(&s.caught, e)).cloned().collect();
        errors.into_iter().collect()
    };
    // what each function's body ends with uncaught, grown to the fixpoint: a call that hands on what
    // its callee ends with makes the caller end with it too
    let mut ends: HashMap<String, Vec<String>> = HashMap::new();
    loop {
        let mut grew = false;
        for n in nodes.values() {
            let mut out: Vec<String> = ends.get(&n.id).cloned().unwrap_or_default();
            let before = out.len();
            for s in &n.sites {
                let raised = s.gives.iter().filter(|g| s.raises && !caught(&s.caught, &g.ty)).map(|g| g.ty.clone());
                for e in raised.chain(handed(s, &ends)) {
                    if !out.contains(&e) {
                        out.push(e);
                    }
                }
            }
            if out.len() > before {
                ends.insert(n.id.clone(), out);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    let fails: Vec<(String, usize, Vec<String>)> = nodes
        .values()
        .flat_map(|n| n.sites.iter().enumerate().map(|(i, s)| (n.id.clone(), i, handed(s, &ends))).collect::<Vec<_>>())
        .collect();
    for (id, i, e) in fails {
        if let Some(s) = nodes.get_mut(&id).and_then(|n| n.sites.get_mut(i)) {
            s.fails = e;
        }
    }
}

/// A foreign type worth a place: a package's type that holds data. Not one of the language's
/// own, and not one reached through a type parameter (`D::Error`, `T.Item`).
pub fn foreign_type(id: &str, own: &crate::knowledge::OwnTypes) -> bool {
    let Some(full) = id.strip_prefix("ext:") else { return false };
    let head = full.split('.').next().unwrap_or("");
    let name = full.rsplit('.').next().unwrap_or("");
    let parameter = head.len() == 1 && head.chars().all(|c| c.is_ascii_uppercase());
    full.contains('.') && !own.packages.contains(&head) && !own.names.contains(&name) && !parameter
}

/// A touch that only prepares a place is part of what is done there. A directory made, or a path
/// checked, for a file the same function reads or writes is that file's (`to.parent.mkdir()`
/// before `to.write_bytes()`); a connection or a session opened on a database the same function
/// reads or writes is part of those statements, and so is a table's shape given where the same
/// function reads or writes the table. A directory made for the file a connection opens is that
/// database's. A table some statement makes temporary is the connection's working memory, no
/// place: its touches go.
fn serve_data(nodes: &mut IndexMap<String, Node>) {
    use crate::effects::{SCHEMA, SCRATCH, SHAPED};
    for t in nodes.values_mut().flat_map(|n| n.touches.iter_mut().chain(n.instead.iter_mut())) {
        if let Some(table) = t.artifact.strip_prefix(SHAPED) {
            (t.artifact, t.op) = (table.to_string(), SCHEMA.to_string());
        }
    }
    let table = |t: &Touch| (t.kind.name().to_string(), t.artifact.strip_prefix(SCRATCH).unwrap_or(&t.artifact).to_string());
    let scratch: HashSet<(String, String)> =
        nodes.values().flat_map(|n| &n.touches).filter(|t| t.artifact.starts_with(SCRATCH)).map(table).collect();
    for n in nodes.values_mut() {
        n.touches.retain(|t| !(matches!(t.kind, World::Db(_)) && scratch.contains(&table(t))));
        let all = n.touches.clone();
        let call = |t: &Touch| (t.line, t.op.clone(), t.kind);
        let folded: Vec<_> = all.iter().filter(|t| prepares(t, &all)).map(call).collect();
        n.touches.retain(|t| !prepares(t, &all));
        // the same call as it was read prepares the same place
        n.instead.retain(|t| !folded.contains(&call(t)));
    }
}

/// Whether touch `t` only prepares a place that another of `all` reads or writes, in every route
/// `t` is in: the other is in every route, or in the same ones.
fn prepares(t: &Touch, all: &[Touch]) -> bool {
    let along = |d: &Touch| (d.by.is_empty() && d.users_of.is_empty()) || (d.by == t.by && d.users_of == t.users_of);
    let data = |d: &&Touch| d.kind == t.kind && !d.shape() && along(d);
    match t.kind {
        World::Disk => {
            let within = |file: &str| file == t.artifact || (t.artifact.ends_with('/') && file.starts_with(&t.artifact));
            // a database's file is made for as a file is
            let opens = |d: &&Touch| matches!(d.kind, World::Db(_)) && !d.file.is_empty() && along(d);
            t.shape()
                && !t.artifact.is_empty()
                && (all.iter().filter(data).filter(|d| !d.artifact.is_empty()).any(|d| within(&d.artifact))
                    || all.iter().filter(opens).any(|d| within(&d.file)))
        }
        World::Db(_) | World::Global if t.shape() => {
            all.iter().filter(data).filter(|d| d.dir.is_some() && !d.artifact.is_empty()).any(|d| d.artifact == t.artifact)
        }
        World::Db(_) => t.dir.is_none() && all.iter().filter(data).any(|d| d.dir.is_some()),
        _ => false,
    }
}
