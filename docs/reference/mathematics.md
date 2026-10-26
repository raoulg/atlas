# Atlas — the mathematics

Atlas draws a small piece of mathematics. This page names every notion it uses, once: what the
notion is, what it is in Atlas, and where it lives. The design (`design.md`) is the engineering
of these notions, the principles (`principles.md`) are the laws `atlas check` asks of code. A
notion Atlas starts to use is added here first.

Each entry says **what** the notion is, what it is **in Atlas**, and **where** it lives: a section
of the docs, a file of the engine, or a principle by its id.

## Category theory

### A category of places and functions
**What.** A category: objects, arrows between them, and composition of arrows.
**In Atlas.** The objects are places where data rests: Memory(type), File(path), Database(table),
Network(url), Process(command), Screen(human), Library(what a dependency produces),
Global(a value a module keeps). The arrows are functions. Composition is call order, and a route
is a composite from an entry point to the places it reaches.
**Where.** `design.md` §0; `flow.rs`.

### Effects belong to the arrow
**What.** In a type-and-effect system, what a function does to the world is part of its type.
**In Atlas.** A function's domain is its parameters plus what its body consumes; its codomain is
what it returns plus what it writes, sends or runs. A file written is as much a part of the
arrow as a value returned.
**Where.** `design.md` §0; `effects.rs`, a node's touches.

### A route's domain and codomain
**What.** An arrow `f: A → B` has a domain and a codomain; in a composite `g ∘ f: A → C` the
object `B` between them is neither. A place whose state an arrow uses and changes is in both:
`S → S′`.
**In Atlas.** A route's places are drawn by what the route does with their content. A place stands
left, in the domain, when the route uses what was there for what it makes. It stands right, in the
codomain, when the route leaves something in it. What the route reads back of what it wrote, or
reads only to check or count, it does not use: that place stays in the codomain. Whether a read is
used is a fact of the step's body: what the call gives reaches the step's codomain (it is returned,
handed to a call, kept in a field) through the names it is bound to and the values a call on it
changes (`f.read_to_string(&mut s)` makes `s` from `f`), or it only decides a choice,
goes to an error or is dropped, and is a check. A place one step
writes and a later step reads and uses is the object between them, and stands between them. A
place the route reads and then writes on is in both: two projections of one place, `from X` on
the left and `into X` on the right. A place it writes only before it reads there, and only in an
arm (a file downloaded where it is missing, a cache filled on first use), it makes sure of: what
it reads there is its input, and the place stands between the step that fills it and the step
that reads it. A read is preceded by a write only when the write happens whenever the read does,
not in one branch of a choice the read is outside. Two arms of one choice never run one after the
other: a place one arm writes and another reads finds what was there and is changed, in both. A
loop runs its body again: a read in a loop finds what a later round writes over, whichever stands
first in the loop. A call that may run one of several bodies (the overrides of a call by name, the
next method of a `super()` call) does at a place what one of them does, as the arms of a choice do; a
call that builds a class does there what the initializer it runs does, at the call's place in the order.
What a step a body calls finds there is what the body left, where the body wrote it before the call:
the step changes nothing the body found. The
route of a class is its doors, each called by the entry with no order between them: a place one door
finds and changes, the route finds and changes. Making a place's shape (a directory, a table's
schema) or testing it (whether a file is there, whether a value holds a key) puts and takes no
content: a test whose answer only decides a choice is that choice's condition, as any condition is,
and the place has no box and no line of its own unless the route reads or writes there. A connection that opens a database on a file names that file, so a directory made for the
file is made for the database. A request goes the way its method says, as a statement goes by its
verb: a `GET` takes what the place holds, a `POST`, `PUT`, `PATCH` or `DELETE` leaves something
there, and what such a request answers is what the place gives back of what it left, read back.
Where the code does not say which way a call goes (a file's mode or a request's method that is a
value), the place is in neither.
**Where.** A function's `reads_back` and `updates` (`flow.rs`); a read's `checks` (`Steps::mark`,
`body.rs`, with each language's `UseSyntax`); a request's way, `knowledge::REQUEST_WAYS`, and
`Touch::side` (`flow.rs`); `sideOfRoles` in `viewer/plan.js`.

### The effect functor
**What.** A functor maps objects to objects and arrows to arrows, keeping composition.
**In Atlas.** `F: Graph → W` sends each function to the arrow from what it takes in to what it
hands on, in the small category W of resting places. The overview of a codebase is the image of
F; refining W refines the picture.
**Where.** `design.md` §1, "The plaat is a functor image"; `flow.rs`.

### Generators and composites
**What.** A graph of arrows (a quiver) generates a free category, whose arrows are its paths.
**In Atlas.** A step's own arrow is a generator; what it reaches is a composite. The two are
different quantities and are never merged (the sixth sentence). Whatever compares two pictures,
compares generators.
**Where.** `CLAUDE.md`, sentence 6; `design.md` §0.

### The terminal object
**What.** A terminal object 1 has exactly one arrow into it from every object.
**In Atlas.** A function that gives nothing back and has no effect is that unique arrow `A → 1`:
it tells its caller nothing. Either it is dead, or it changes something the code does not show.
The graph cannot tell that arrow from one whose effect Atlas does not model: where a body runs a
call no table describes (a library's call, a callback in a field, a method nothing implements),
the finding names that call as what Atlas cannot see, and asks whether it is dead only where it
sees everything the function runs.
**Where.** Principle `nothing`; `Node::unseen`, `Node::declared` (`body.rs`, `Steps::dropped`).

### Coproducts
**What.** A coproduct A + B: a value is one of the alternatives, and a function out of it is a
case for each.
**In Atlas.** A sum type (`Result`, an enum, a union) is taken apart where it arises, and a
choice between alternatives is made once, in a place with a name. A union written in place in a
signature is the coproduct a named one is, one object however and wherever it is written. The
copairing has to stand somewhere: one function that takes the sum apart is where it stands, and
`sums` asks only when two or more take it. `1 + T`, a value that may be absent (`T | None`,
`Option<T>`), is not asked by `sums`: its consumers ask whether it is there, not which outcome it
is. By naturality, `h ∘ [f, g] = [h ∘ f, h ∘ g]`: arms that each make their value by the same step
are that step with a choice of what it is handed, no choice of where the value comes from, and
`choices` asks only of arms whose steps differ. A choice in a body is a
coproduct of its arms, each injected by what decides it as the code writes it (`when not
raw.exists()`, a `match` pattern, `otherwise`). A choice of one arm is a coproduct with the
identity, `f + 1`: its steps run when the condition holds and nothing runs otherwise, so it is
drawn as a choice too. In `x ?? y`, `x || y` and `x && y` (Python's `or` and `and`, Rust's
`x.unwrap_or_else(..)`) the left always runs and its value goes on where it holds, so it is no arm; the right is
the one arm. What goes on past them is the left's value where the arm does not run and the arm's where it does, as
in `match x { Some(v) => v, None => y }`: a body that gives back `x ?? y` gives back both. A guard, a choice none of whose arms runs on past its end and that has no arm for when none
is chosen (`if not xs: return Failed(..)`), is that choice with what follows it in its block as its last arm: `return
Fit(..)` after it runs otherwise, as an `else` would write it. A repository's graph is the coproduct of its parts' graphs, glued only
along the libraries and places of the world they share (a pushout over those): two parts whose code
holds one name stay two, each name injected under where its part lies (`orders/app`, `billing/app`).
A call into a family runs one of its overrides, decided at run time, so its codomain is the coproduct of theirs: it
hands on each error any of them can end with, as a call hands on each its one callee can end with, never one chosen
for it (#213).
**Where.** Principles `sums` and `choices`; `Node::whens` (`body.rs`), `arms_of` and `guarded_by` (`tree.rs`), `Item::Choice` (`flow.rs`),
the MCP tool `choices`; `parts::apart`, `Graph::rename`; `Site::fails` and `settle_fails` (`builder.rs`).

### Lenses
**What.** A lens on S has `get: S → A` and `put: A × S → S`, and a lawful one gets back what
it puts: `get(put(a, s)) = a`.
Lenses compose: a lens `S ⇄ B` followed by the projection of `B` onto one of its fields `B ⇄ A`
is a lens `S ⇄ A`. Through a variant of a sum the projection is partial (a prism), and the
composite gives `A` back when the value is that variant; a sum given a name is the same sum, so
the prism goes through the name. A family of lenses `S ⇄ A_k` indexed by
a key `k` (a row by its id, a tile by a point it holds) takes the key to choose the lens, not as
the value it puts.
**In Atlas.** A class that writes a place and gives back a value from it is a lens on that
place, the place being what the code names: a table, a file by its path or by the value that holds
it, never a database as a whole or a file known only by its format. Atlas asks the law at the
level of types, a necessary condition of PutGet where the focus is right: what goes in as a
`Graph` comes back as a `Graph`, as it is or inside what a get gives (`has` and `variant` edges,
and a subclass, a variant of a sum left open). A constructor that reads the place is a get of its
class. What a put takes is what reaches the place through its body's steps, by what each is
handed and gives; a parameter handed only to reads, or only to a statement's key (`WHERE`), is a
key. A view is the `get` of a lens with no `put`: it only reads.
**Where.** Principle `lens`; `design.md` §1.

### Pullback: a filter
**What.** The pullback of `true` along a predicate `p: G → Bool` is the part of G where p holds.
**In Atlas.** A filter (a route, an entry point, an edge kind, one part) keeps that part: its
nodes and the edges between them.
**Where.** `design.md` §1, "Fibers".

### Coequalizer: a quotient
**What.** A coequalizer identifies what an equivalence relates.
**In Atlas.** Zooming out identifies the nodes of a class (by base class, by package, by
composition) into one box.
**Where.** `design.md` §1, "Quotients".

### Pushout: forms that say the same thing
**What.** A pushout glues two objects along what they share.
**In Atlas.** Forms that say the same thing are one construct, in every language: typer's
parameter default, clap's `default_value` and parseArgs' `default:` are each an option the
caller can override. The pushout is drawn, as the entry's usage line; the language's form lies
beneath it. An environment variable with a fallback (`os.environ.get("X", "d")`,
`process.env.X ?? "d"`, `env::var("X").unwrap_or("d")`) is that construct too, and so is any value
nothing in the program hands, with a fallback: one input, one place, drawn as its default and
decided by no code, since a run reads or writes one file whatever the input says. A type written
through an alias (`Order as Parcel`, `use tree_sitter::Node as TsNode`) is the type it stands for: a
parameter holds it by that type's name, as the type it takes is named (#216).
**Where.** `CLAUDE.md`, "Isomorphic is the same"; `Entry::options`; `paths::or_else`; `model::resolve_held`.

### Paths as terms, directories as fibers
**What.** A path is a term in a small algebra over a root: a root (a literal, a parameter, an
input with a default, where the package's code lies) closed under `/`-join, `parent`, `with_suffix` and a pattern below a
directory. A collection of paths is the set of terms its elements can be; iterating it yields
one of them. Substituting a parameter by the term a caller hands it is the term's evaluation, and
a part no code determines is a named hole. A pattern below a directory (`D.glob("*/IO-*")`) is
the fiber of `D` over the predicate "matches the pattern": the part of the directory the pattern
holds.
**In Atlas.** A place on the disk the code names only through a variable is drawn as the term it
evaluates to (`assets/regelingen/*/IO-*`), not as the variable (`m`). The root is traced to a
default or to a parameter wherever the code says it; what it cannot trace stays a hole in the
term, so the gap is visible. An input with a default (a parameter default, an environment
variable with a fallback, a command-line option) is the caller's to choose; a file found from
where the package's code lies ships with it. The code decides a place in the frame of the
function where the root becomes something it holds, a literal or a constant, which is where
`places` asks. A file the term decides to the last part is a `File`; one whose name it decides
below a directory it does not is that name under that directory, the hole kept in the term by
the name the code gives it, wherever in the term it stands: `File({$DATA_DIR}/report.txt)`. Two
holes the code names differently are two directories, since nothing in the code says they are one;
a hole the code gives no name (an expression Atlas does not follow) is `{}`, never `*`, which is
a wildcard the code writes. A pattern is the fiber below its directory, and what is read of what
it yields is the same place. A place the program names a place below is a directory (`D/*/IO-*/`,
which `glob` lists and `io_dir / "Metadata.xml"` reads in), so the listing and what is read in it
fold into one folder. The place is the term; its label says the term's patterns in words, from the
file up (`every *.xml in OW-bestanden/ in each folder in {base_dir}/`), and the term is under the
pointer. Words are a label, never an identity: two places are one when their terms are.
**Where.** `paths.rs` (`Seg`, `join`, `place`, `folders`, `Callers`), `python/paths.rs`
(`Eval::of`), `plan.js` (`pathWords`); the pairs `python/paths` and `places`.

### Yoneda: what changed
**What.** An object is determined up to isomorphism by the arrows into it, `Hom(−, A)`, and
dually by the arrows out of it.
**In Atlas.** Between two revisions, a place or a type is changed when its direct arrows differ,
even when its name stays. A function is changed when its own arrow differs. An object that
disappears and one that appears with isomorphic direct relations are one object renamed, when
the match is unique. In a picture, Yoneda is applied to the quotient it draws: a closed box (a
part, a package, a family, a place that stands for others) is one object, and its own arrows are
those that cross its border. It is changed when they differ; what changes inside it without
reaching the border is counted beside it, not marked on it. A bundle is one line with its count
(rule 3), so a line both readings draw is changed when its count differs (`×2 → ×3`), and so are
the boxes at its ends.
A step of a body is the same in both when it makes the same call on values of the same types,
in the same order. Which step made a value is the arrow of that step, not of the one it goes to,
so a change before a step leaves it the same step: `raw = load(p)` becoming `raw = fetch(p)`
marks that step, never `smooth(raw)` after it, and `raw` is one value whose arrows in differ. A
step put in before another (`double(add_one(x))` where A had `double(x)`) is the only step
marked: the step after it is handed in each reading what that reading gives it. A place a caller
makes whole is the caller's arrow; a file named by a function's own parameter (`load.path`) is
named so whoever calls the function. A rename is an isomorphism of the typed arrows in and out:
`list[float] → float` is no `str → str`. `atlas changes` reads the same union along each route,
so the picture and the list hold the same marks.
**Where.** `atlas history`, `atlas changes A..B`, a vision beside the working tree (`atlas
mcp`'s `vision`); `diff::union`, `diff::reading`, `changes::between`; `design.md` §4, L5.

### Partial morphisms: a prophecy
**What.** A partial map is defined on part of its domain; a graph with some objects and arrows
marked "not built" is a graph together with a subgraph of what exists.
**In Atlas.** A prophecy is sketch code kept in the tree under the real ids, marked in each
language's form (Python's `@prophecy(..)`, Rust's `#[doc = "prophecy: .."]`, TypeScript's
`@prophecy` JSDoc tag): one construct, a flag on the node with its `supersedes`, `veiled` and
`sealed`. The graph holds the built part and the prophetized part in one picture; the arrows
from the built into the prophetized are the work still to do, and `supersedes` is an arrow from
the new object to the one it replaces. A prophecy is fulfilled when the marker is gone.
**Where.** `engine/src/prophecy.rs`, `Node::prophecy`; issue #26.

## Topology

### Gluing: zooming out
**What.** A quotient space glues the points an equivalence relates into one. Gluing is a pushout:
collapsing a part R of G onto a point b is the pushout of `G ← R → {b}`.
**In Atlas.** Zooming out glues each family of types into one point, one box. Of a library, what
lies beyond the fiber the project reaches is glued into the function at the fiber's border: that
function is one arrow, whose effects are all its code reaches inside the library.
**Where.** `design.md` §0, "Zoom is a quotient on types"; `view::reached`.

### Fibers: one route
**What.** The fiber of a map over a point is everything that lies above that point.
**In Atlas.** Keeping one route, or one class, is the fiber over it of the map that groups;
opening a box puts its fiber in its place.
**Where.** `design.md` §0, "Filters are fibers".

### Fine and coarse
**What.** Topologies on a set are ordered from fine to coarse. A partition gives one: its open
sets are the unions of its blocks.
**In Atlas.** A zoom level divides the nodes into boxes, so each level is a topology on them.
Zooming in moves to a finer one and zooming out to a coarser one, so the picture stays the same
picture. The code's hierarchies say which levels exist: parts into packages into modules, a
route's steps into their bodies, families into their members. A zoom is the set of boxes that
are open, closed downwards; the boxes reached but not opened are the blocks of its partition.
Zooms in two hierarchies meet in their common refinement, so any combination of zooms is one
picture.
**Where.** `design.md` §0, and §1, "A view is data".

### The closed picture is the pushforward of the fine one
**What.** A graph with directed lines, a quiver `G`, and a partition `π` of its vertices with
quotient map `q`. The quotient quiver `G/π` has the blocks as vertices and, for each line of `G`,
a line from the block of its source to the block of its target; a line with both ends in one block
becomes a loop on that point. It is the coequalizer in reflexive graphs, so `q` is a morphism and
every morphism that is constant on the blocks factors through it once. Two lines of `G` with the same
ends in `G/π` are one line with multiplicity two, a bundle. The fiber of `q` over a block is the
part of `G` that collapsed into it, and opening the block puts back the fiber with its boundary:
each line between the fiber and the rest is joined again to the block of its end in the fiber.
**In Atlas.** The vertices of the finest picture are not places but values at a moment: a place
as one step leaves it, a value as the step that makes it hands it on, a call occurrence. The lines
run from the step that makes a value to the value and from the value to the steps that take it.
Every picture is `G/π` for the open boxes of its zoom. Six rules follow, and they decide what a
line or a box that appears or disappears means:

1. **A line without an image is a fault of the picture that lacks it.** The closed picture shows at
   least the image of every fine line that crosses its border. A line that the open box draws across its
   frame and the closed box did not is the closed box showing less than the pushforward, and the
   fault is in the closed picture. Which fine lines exist is the code's: the domain of a step is its
   parameters and what its body consumes (CLAUDE.md, sentence 2).
2. **A line inside a block is no line.** A line with both ends in one block is a loop, and the closed
   picture does not draw it. What the hidden elements of one box do to each other is not noise to be
   filtered but a fact of the fiber, shown when the box opens.
3. **Lines between two blocks are a bundle.** Fine lines with the same two blocks and the same
   channel (an effect of the step's own, a reached one, a value handed on, a call and nothing else)
   are one line that says how many it stands for, and opening splits it into the fine lines, each to
   the block of its end. Different channels between the same two boxes stay different lines. No line
   is left out because another joins the same boxes: a picture shows fewer lines only inside a block
   (rule 2) or through a filter the reader chooses and the picture names (by channel, or by box, the
   filter first).
4. **Reading back is no exception.** What a step makes and reads in the same box is a value inside
   the fiber, a vertex of its own; the place keeps the value as the box leaves it, in the codomain.
   The line from the place into the box that wrote it does not exist at a vertex that is a value at a
   moment. In the plan, a read inside a step of what that step's own writers wrote there is the value
   `X · in S`, made by those writers and taken by the reader, standing in the step's frame; closed, it
   is internal to the step. A closed step's reads of what it wrote itself are no read of the place, so
   the place keeps the side of what the step leaves there: a store that writes a table and counts its
   rows stands where its other end products stand. What it wrote is what it wrote at the places the route
   touches (rule 5): a helper that writes the path its caller makes whole writes that caller's file, not
   the file it names alone, so a read of the latter is no read back. A step's arrow stands in every context
   of callers that make a path it reaches whole, and reads back what it reads back in any one of them (#217).
5. **Reach is computed on the fine graph.** Quotienting identifies vertices and can make a path that
   no fine path has (`a → b`, `c → d`, `b ≡ c` gives `[a] → [d]`). What a step reaches is the closure
   of its calls in the fine graph, drawn apart from its own arrow (sentence 6), and never read off a
   quotient. The fine graph is the graph of occurrences, not of functions: a place a caller makes whole
   (a path it hands down) is touched under a step only where that caller is on the calls that lead to
   the step, or below it. A helper that writes the path its caller hands writes, under each caller,
   that caller's file.
6. **Places with the same incidence are one block.** The rule for lines holds for the vertices too: values
   that one closed step takes, that no step makes and that join nothing else, have the same neighbours
   (that step) and the same way (in), so they are one block of the quotient with a multiplicity `n`, drawn
   as a count on the step (`graph_of · +25 types`). Contracting a vertex of degree one into its neighbour
   leaves the picture the same up to homotopy, and the border of the step stays whole: each line is still
   counted, and opening the step splits the block into the values, each with its line. A value that
   another step takes or makes, a family that can be opened and the route's own values are not of the block.

**Where.** `homeOf` and `madeAt` in `viewer/plan.js`, `addEdge` in `viewer/index.html`; `tools/pictures.py topology`
holds the open picture to the closed one.

### Refinement keeps what it does not refine
**What.** A finer partition refines a coarser one: each block of the finer lies in one block of the
coarser. The map `q` that sends a point to its block is the quotient map, and the coarse graph is
the image of the fine one under `q`: its points are the blocks, its lines the images of the fine
lines that join two different blocks. A line inside one block is gone from the coarse graph; no
other line is.
**In Atlas.** Opening a box takes the picture from `G/π` to `G/π′` with `π′` finer, and only the
box opened is split. Seven laws follow, the first six statements about two pictures, the one before and the
one after:

1. **Incidence.** Every line of the closed picture is the image under `q` of a line of the open
   one, and every line of the open one that leaves the box has an image that the closed picture draws
   (the pushforward above): opening takes no line away, and a line it adds across the frame is one the
   closed picture should have drawn.
2. **Containment.** A box that does not lie in the opened box stays in the box that held it. What
   lies inside the opened box is what `q` sends to it: the frame is the fiber of the closed box.
3. **Side.** A place of the world stands left, right or between by the coarsest picture that shows
   it. What the steps inside an opened step do at the place is that step's doing, and opening the
   step does not change the side.
4. **Moment.** A value stands at the moment its maker made it. The arrow of an opened step hands on
   at the moment the closed step did; the values made between its own steps are at moments inside
   it.
5. **Closing.** The picture is a function of the set of open boxes, not of the order they were
   opened in: closing the box draws the picture it was, and two openings commute.
6. **Reading.** A box's own arrow is the lines that cross its border. An opened step keeps them at
   its frame: what its call was handed and gave, and the result that goes on to its caller.
7. **Endpoints.** Every line the picture plans ends at a box it draws. A folded directory is that
   directory, whatever it is itself folded into, and a line planned to a box that is not drawn is a
   fault, not a smaller picture.

What the picture does not claim is where a box lies on the page: the layering (`elkjs`), the camera,
and which lines are pinned are presentation. The camera keeps the box opened where it stood on the
screen, at the zoom the reader chose. What is pinned is a set of boxes, and zoom carries it by `q`
like everything else: opening a pinned box pins its frame, the fiber of the box, so the lines that
crossed the box stay lit; closing the frame around a pinned box pins the box it closes into, its
image.

| Law | What it asks of the viewer | Where |
| --- | --- | --- |
| Incidence | a closed step is the block of the occurrences below it: its lines are the images of the fine lines that cross its border; the frame of an opened step has the id of the block, its own arrow (`o:`) is a point on the frame's border | `imageOf` |
| Containment | a box stands where the finest plan puts it: a value in the smallest frame that holds its users and its maker, a place of the world outside every step | `imageOf` (`homeOf`) |
| Side | the roles of a place are counted on the coarsest picture, where only the route is open | `coarsen` |
| Moment | a value has the moment of the occurrence that made it; a closed block hands on at its own | `madeAt`, `imageOf` |
| Closing | the picture is the image of the finest plan under the map the sets in `V.open` define, and nothing else | `plan`, `coarsen` |
| Reading | a call's hands and gives are applied to the step's own arrow as to its closed box | `stepMorphism` |
| Endpoints | `addEdge` records each line it cannot end (`atlasPicture.dropped`); the walk reports it | `addEdge`, `pictures.py walk` |
| Refinement | for every box that can be opened, the picture with it open is held to the one drawn (`atlasPicture.oracle`) | `refines`, `checkOpenings` |

| Decision of the picture | The principle behind it |
| --- | --- |
| a frame has the id of the closed box | a block of the finer partition is the fiber over a point of the coarser |
| the steps of a body stand in their frame, in call order | composition is call order; a route is a composite |
| a place of the world is no part of the code's hierarchy and stays outside a package, part or step that shows it | a place is an object, a package is a block of the partition of functions; `q` moves functions only |
| memory made and used inside a step lives inside its frame | an object between two arrows of a composite is internal to it |
| what a step takes from beyond it stands outside its frame | the domain of a composite is the domain of its first arrow |
| a choice is a frame of its own, its branches inside | a coproduct is one object whose arms are its injections |
| files fold into the directories that hold them, one box per side their files stand on; directories nested in one another that hold the same files are one box, drawn as the deepest, and opening it opens them all; opening a directory refines the fold | the disk is a tree, and a fold is its quotient by directory, met with the side; directories with the same files are one block of that quotient, so the box is one class and opens as one (`foldDirs`, held by `plans.mjs` and `pictures.py topology`) |
| a dotted line joins the packages whose code calls each other, whatever data also joins them; opened, it refines to what inside is called, down to the function's own box | the calls are a relation, and a quotient of a relation keeps its image; a line drawn only where another is absent is no relation and would come back on opening |
| a type with parts is a family frame; its parts stand inside it opened | zoom by composition is a quotient by the relation "is a part of" |
| the overview draws a type once, the box that what makes it and what takes it share, at every zoom of the packages | the routes of an overview have no order between them, so a value has no moment there: its identity is its type |
| the layers of a route run left to right, entry to places reached | a graph without cycles can be drawn in layers (presentation, no claim) |

A closed step takes what its body takes from beyond it (a field of the receiver, a library's
value) as well as the types of its signature: the domain of a composite is the domain of its
first arrows, so opening the step draws no line across its frame that the closed box did not. A
value no step makes is its type from beyond; two of one kind that one step takes (`consume(a, b)`,
`report(c: Circle, q: Square)`) are two values, each by the name that holds it, also once their
family closes them into one box, and a value a step made keeps its name in its moment and in its
family's frame. A route's parameters are such values, there at the route's start: its own arrow
takes them by their names, and a step handed one by its name takes the same value. The route of an
exported class is its doors, its public methods, with no order between them: each runs on the
caller's instance, one value of the class from beyond (no name, no moment), and what the constructor
door builds goes back to the caller. A door takes its parameters as a route takes its own, by their names, and the
parameters of two doors are two values: each door is called by itself. A closed family holds its kinds (a subclass stands in its base
until the family is opened); a part of a type is structure of its owner, a box of its own in the
owner's frame, and the values of the part's type stand on their own (#39).
**Where.** `plan`, `imageOf` and `refines` in `viewer/plan.js`, the camera in `viewer/index.html`;
`tools/pictures.py topology`; `engine/tests/plans.rs`.

### One fine plan, every picture its image
**What.** A quotient is not computed from what is drawn of it. A map `q: G → G/π` is defined on the
fine graph `G`, and the coarse graph is its image: its objects are the blocks, its arrows the images
of the arrows of `G`, and an arrow whose two ends lie in one block is gone. Every notion the coarse
graph needs (where an object stands, which side it is on, when it was made) is a function of `G` and
`q`, so two coarse graphs of one `G` are images of one thing, and refining `q` can only split.
**In Atlas.** A route has one finest plan `F`, built once from the code and independent of what is
open:

- every call of the route is an *occurrence* (a step key, its path through the route), opened to
  all its calls; an occurrence with calls has a frame and its own arrow, one without is a pill. Building
  a class runs the initializer along its order (Method resolution order), so the step that builds it
  opens into that body and never into the class's methods, and building the class is no call of it,
  also inside the route of that class. An initializer another one calls (`super(text)`) builds nothing:
  it fills in the value its caller builds. An override in a dispatch runs on the value its call runs on
  and takes what the call hands it, at the place its own signature has it: what the dispatch takes
  crosses its frame to the override, as to any step inside;
- every value is an identity `(type, name, moment)`: the type it has, the name that holds it, and
  the occurrence (or choice) that made it; two values of one type at one moment are two; a route's
  parameters are values with their names at the route's start. A name holds one value: one that
  holds a sum holds the sum, whichever variants the reader lists of it (Coproducts), one a
  constructor fills holds what it builds, and a name handed on holds what the body last gave it. A
  name is read by the binding visible where it is used: a name bound again (shadowed, in a block,
  in an arm) holds another value, as a second name would, and a name bound to another through what
  hands a value on unchanged (`let record: Record = result?`) holds that one's value;
- every place of the world is at its concrete place (a path, a table, a command); what a step reads
  back of what it wrote there is a value at a moment inside the step (rule 4). A place named by a
  value an object holds is one place wherever it is reached: a method runs on an instance its class's
  constructor built, so `self.path` in a method and `path` in a helper it hands it to are both
  `Disk(Store.path)`. A module's global holds what rests in its place, `Global(m.df)`; a step its body
  hands it to takes it there, no value from beyond, until a step of the body makes it anew. A figure a
  library draws on rests in that figure (`Figure`) until something saves or shows it, and a notebook shows
  each figure its cells make on the screen (`Figure 1`). A figure rests in what holds it, as a place named
  by a value does, through the fields that hold it: one a method keeps in a field of the value it runs on
  (the fields the binding that makes it sets, `self.fig, self.ax = plt.subplots()`) is that value's, so
  every step that runs on the value draws on the one figure it holds, whichever of them makes it
  (`create_figure` only where `self.fig is None`); one a call makes and no value beyond it holds is that
  call's own, and two calls make two, also two in one body (`fig, ax = plt.subplots(); other, bx = plt.subplots()`). A value a body sets such a field of to what another value holds
  there (`other.fig = self.fig`, or `self.fig = fig` in a method its call hands the other's figure to) is
  that one's put, a lens's: from then on it holds the figure the other holds, and so does every value made
  anew of it. The code names the field it sets (`Node::sets`), so a field that holds no figure lends none
  (`other.title = self.title`), and the field each argument of a call reads of the value it hands (`Held::through`), so a
  parameter filled through one is that projection: `self.fig = fig` in a `take` its call hands `self.title` sets the field
  to the title, and lends none. A method called on a part of a value (`self.left.line(..)`) runs on the value through the
  field it reads: a figure it keeps in the value it runs on rests in that part, so two parts hold two figures
  (`board.left`, `board.right`), and a change made through the part makes the value anew. A part holds a figure as a value
  does, by the class its owner's field holds (`Node::holds`: a board's `right` holds a Chart, which keeps its figure in `fig`
  and `ax`): `self.right.take(self.left.fig, self.left.ax)` sets the right part's fields to the left part's, so from then on
  the right part holds the figure the left one holds, and so does a parameter filled through a part (`other` of
  `self.left.lend(self.right)` is the board's `right`, and `other.take(self.fig, self.ax)` lends the same). A step that changes
  such a parameter changes the part's owner, which its call gives anew, and no value of the parameter's type of its own:
  `lender.lend(self.left)` makes the board anew, its left part holding the lender's figure, also beyond the cut. Each binding that
  makes a figure is a figure the value holds: a class whose methods keep figures under two bindings (`self.fig, self.ax =
  plt.subplots()` and `self.res_fig, self.res_ax = plt.subplots()`) holds two, each named by the first field its binding sets
  (`fit.fig`, `fit.res_fig`), and a class that keeps one names none. A figure made in an arm that a condition decides by a
  field of the slot it is made in is that slot's figure made sure of: the slot keeps what it holds, its own or one lent to it
  (`if self.ax is None: self.create_figure()`, the condition in the step's body or in a body that calls it on the same
  value). One made otherwise is made anew, the slot's own, which the slot holds from then on, put over one lent to it: under
  `if self.ax is None`, `self.res_fig, self.res_ax = plt.subplots()` makes the residuals' figure whatever `res_ax` holds. The
  condition names the field it tests (`Node::tests`); without a lend the two readings name the one figure the slot holds.
  A part set to a whole value (`self.left = left`) is that
  value, a lens's put (`get_left(put_left(c, b)) = c`): what the value holds the part holds, and a figure lent to either is
  lent to both, so a chart that holds a figure, set into a board's part, draws there on the figure it made. A figure is one
  figure wherever what holds it is reached, and is named where it is made, by the value that holds it there and the part
  (`Chart · left` made by `left.line(..)`, `Board · board.left` made by the board's `self.left.line(..)`);
- a step's own arrow is its witness at its border: its signature and its own touches, the lines that
  cross its frame. An opened step draws it as a point on the frame's right edge, not as a step beside
  its steps: a value its call is handed crosses the frame to the step inside that takes it (the value
  its signature takes by no name is the parameter a step inside takes by its name), and the
  point keeps what the body itself takes and what it gives back, at the step's moment. What the body's
  own code uses outside its steps (a print, a condition, a format, what it returns, what it keeps in a
  field of a value, a lens's put: `self.left = Chart("left")` in an initializer), of a value a step
  inside made or one it is handed, is a line from that value to the point, also where a step inside
  takes the value too: the line lies inside the frame, so it is internal to the closed step (rule 2)
  and the value stands in the step's frame. A value no line takes says that the body drops it; the
  map draws fewer facts by zoom, never by leaving one out. Of what the
  body's steps make, what it gives back is what no step of the code among them takes and is of a type
  its signature gives as its language reads the type (its result, or the error of `T + E`): a value built only to be handed into another
  step (`Row(Reading(line))`) is that step's, and a body that gives back a number gives back none of the
  values its steps make, which it uses, if at all, by its own code, nor does one that gives back a collection of pairs
  (`Vec<(String, Vec<Where>)>` gives back pairs, no `Where`). A value handed into a
  library's operation goes on in what the operation gives, and is the operation's only where that
  is; what a function written in place gives back, as its expression or by a `return` of its own (`(t) => { return
  t.drawn(); }` is `(t) => t.drawn()`), goes where its result goes, so handed to such an operation it goes on in
  what the operation gives (`.map(|t| t.drawn()).collect()`). A result of no type of the code's goes back to the step that called it unless another step of
  the caller takes it, which it then reaches as a value. Going back, it is one line labelled by the
  name that holds it, and no value beside it: the caller's own arrow is a point on its border, no
  step, and `done = finished(..); if done:` is `if finished(..):` with a name. A call is
  handed the values the names in its arguments hold: `f(x.name)` is handed `x`, since the projection
  `.name` is no occurrence but glued into the call, whose arrow is `f ∘ π_name` with domain `X`; beside
  `x` the call keeps `π_name`, the field the argument reads, since `f ∘ π_name` and `f ∘ π_fig` are two
  arrows (`Held::through`, through the fields it is reached by, as `Set::of` names one), and
  `f(self.path)` is handed the instance the method runs on; a nested call that is a step of its own
  takes its names itself, and the call around it is handed what it gives. A library's operation that
  is a step carries the value it gives, by the name or the names the body binds it to (`with X as con`
  as `con = X`, `a, b = f(..)` each part), of whatever type; what it touches at a place stays its
  caller's own touch, so naming its result changes no line to the place. A copy makes its value anew
  (`board = copy.copy(board)`), and a method called on a library's value is that library's operation
  also where its types do not list it. A projection bound to a name first is still the projection
  (`width = settings.width; f(width)` hands `settings` through its field `width`, as `f(settings.width)` does, and
  `f = self.fig; other.fig = f` sets the field to `fig` of the instance; names bound in turn compose, `a = self.size; w =
  a.width` is `size.width`, and a binding written as as many parts as its value binds each name to its part, `fig, ax =
  self.fig, self.ax`), a lookup by a key among them (`row = table[k]`
  hands `table`, the lens the key chooses), and so is any value a body binds without a step: a
  pattern that takes a value apart (`const [year, month] = fields` hands `fields`), a name a
  `case` pattern binds (a part of what the `match` takes apart, through the variant the case matches, a
  prism), an element of a loop that is no step (`for s, t in rows`), an element a method hands the
  closure or the function it is given (`g.nodes.iter().map(|n| ..)` hands `g`, and inside it
  `n.touches.iter().chain(&n.instead).map(Touch::drawn)` hands `g` as well: what `chain` goes on with
  is part of what it is handed, an element of either; the closure's name is that element, of the class
  a loop's name over the same value is, so `.map(|t| t.drawn())` is `Touch::drawn` as `for t in ..`
  makes it, and `touches.map((t) => t.drawn())` hands `touches`), what operators compute of names
  (`bottom = height - margin`), what each arm of a choice sets a name to (`let unit;
  if (..) unit = KELVIN; else unit = CELSIUS`). A change made through one changes its owner, a lens's
  put. A parameter is such a name: inside an opened step it holds the value its call hands it, not a
  caller's value of its name or the latest of its type, and where the plan cannot say which, it is still
  a value of that body, in its frame, never one from beyond the route: what comes from beyond the route
  is what its caller holds, its parameters and the instance of a class route. The call hands each
  value to the parameter its argument fills, as the call writes it, whatever its type: the value it runs
  on to the receiver (also the first argument of a method called through its class or path), an
  argument by its place to the parameter at that place after the receiver, one by its name to the
  parameter of that name; a spread to each parameter from its place on, a spread of names to each one
  the call writes no argument for. A parameter holds every value its argument hands (`f(a.x + b.y)`),
  and none where its argument hands no value (a literal, a constant) or the call writes none for it,
  where it holds its default. A parameter the call fills through a projection (`finished(job,
  settings.mode)`) is still the projection, so the steps inside are handed `settings`, and a
  signature's type stands for what a parameter holds only where the call does not say what fills it:
  never for one it writes an argument for. A value
  of no type of the code's is one where a step makes it, so a parameter of no type of the code's
  (`path: &Path`) holds one only where its call fills it with one, a value a step made or a projection
  of one: a literal fills it with none, and the route's own, which no code of the program fills, hold
  none. What an opened step changes of a value it was handed goes out at its border as what its steps
  made of it;
- a step that can fail has the codomain `T + E`: the error is a value its constructor makes in the arm
  that raises it, the step's border hands it on with its result, and a call that does not catch it
  hands it on to its own caller. Python's propagation and Rust's `?` are one form, no choice. An error
  is a value of the plan where the code says it is one, whatever library its type is of: where the
  code builds it to raise (`raise RuntimeError(..)`, `throw new Error(..)`, `Err(Error::Empty)`), where
  a `?` hands on what a call fails with (converted to the function's error type, glued into the step
  as a projection is), and where a handler takes it. A `?` written in the body's own code, in no step
  (`Ok(fs::read_to_string(p)?.lines()..)`), is handed on by the body's own arrow, at the step's moment.
  An error a library may raise that the code neither builds, hands on nor catches, the code does not
  say, and it is no value. The errors the arms of one choice hand on are one value at the choice's
  moment, and an opened step takes each error its steps hand on that none of them takes. A call whose
  signature gives its error in the sum it gives (`Result<T, E>`) and that hands no error on gives the
  sum, at the step's moment, also in an arm: what takes its result takes its error with it
  (`store.save_rates(base, rates::fetch(..))`), and a body it is handed to takes the sum apart, its
  steps a variant (`Ok(body) =>`), its own code the error (`Err(e) =>`).

What the reader has open is the map `q`, nothing else: an occurrence goes to its outermost closed
ancestor (or to itself), a type to the box its family stands in (`placeOfType`), a world place to its
group, its fold, or itself, a value's moment to the occurrence it goes to, and the steps that only
parse the command line to the entry. The closed picture is `F/q`:

- a closed step's domain is the places that the occurrences below it take and no occurrence below it
  makes; its codomain is what its own arrow hands on and what the occurrences below it write. This is
  the domain of the composite, "the parameters plus what the body consumes" (the second sentence),
  not the signature alone. A line comes from the step itself when its own arrow supplies it and
  reaches through it otherwise (the sixth sentence);
- a place made and taken inside one block is internal to it and is not drawn. The arms of a choice make
  one value at the choice's moment, the value it hands on past it: what two of its arms make, by a
  step or by the body's own code (`let out = match f { "csv" => String::from(..), _ => format!(..) }`;
  the variants of a sum are one value of it), or what one arm makes anew of a value named before the
  choice, which the other arms keep. What one arm makes and uses itself is its maker's, and no step of
  another arm takes it: a run goes through one arm. A step that is itself a choice (a call into a family, which runs
  one of the overrides) makes it at a moment inside the step, and hands on a value of its own at its
  own moment, so what its overrides make is internal to it and what it hands on is not;
- the families are taken on the route's fiber, the types its plan holds (saturation: the fiber first,
  then the quotient): a type stands in its kind where the plan holds another type of that kind, so a
  route draws the same whatever other routes do, and an opened family holds, for a value, the kinds
  that value can be;
- fine lines of one channel between the same two boxes are one line that says how many it stands for
  (`×n` on the line, its stroke thicker); opening the box splits it into the fine lines. A channel is
  what the line says: data the step moves itself, data it reaches through a step it calls, a call, a
  result handed back. A closed step that writes a place itself and through a step it calls draws two
  lines to it, one solid and one dashed;
- sibling steps that draw the same lines on `F` (the same places, versions and channels) are one pill,
  and each of its lines stands for the lines of all of them. The merge is decided on `F`, not on the
  picture, so opening a family, a group or a directory never splits a pill it did not open;
- where a box stands is decided on `F`: a value stands in the smallest frame that holds all its users
  and its maker (a choice or the route frame when no step makes it), a place of the world stands
  outside the steps that show it. The block is its image, so no box moves into or out of a frame by
  being opened;
- which side a place of the world stands on is decided on the coarsest picture, `F/q₀` with only the
  route open, and carried by `q` to every finer one;
- the quotient of the storage (the disk by its files, a database by its tables) and the fold of the
  disk by directory are taken on the places of `F`, so they are the same at every zoom of the steps.
  They keep the side: places of one group or directory that stand on different sides are different
  classes, `from X` on the left and `into X` on the right. The quotient is the meet of the grouping
  with the side, saturated by it, so a group never stands between steps that its places do not stand
  between, and opening it never moves a place to another side;
- the plan is unfolded as deep as its budget allows (`FINE_BUDGET`, some thousands of occurrences). An
  occurrence beyond the cut stands for the image of its whole body: the body unfolded one level, each
  of its calls cut in turn, planned and closed by the same construction as every closed step. Closing
  is a composite of closings (closing a step's calls and then the step is closing the step), so this
  is the image of the body unfolded to its leaves, made without unfolding it, once for each function,
  call and set of callers that make its paths whole. Planned by itself, a body does not know its
  caller's values: a value it takes by a name it did not make is the caller's value of that name where
  the caller has one, else the latest of its kind the body made, else one its body binds out of the
  plan's sight, which the caller does not see either; a name its function does not declare (a module's
  constant) is what the module holds, from beyond, as it is with the step open. What its border hands
  on is what it hands on open: an error as an error, a result of no type back to its caller, a value
  of no type by its name; and the types its body holds are the route's fiber, as they are open. The cut
  changes how much of `F` is held, never its picture, and opening a cut step refines as opening any step
  does: the picture it opens to, planned anew, maps onto the one it was cut in.

On the image the page then does only what is a quotient of `F` or a way of drawing it: the merge of
siblings, the storage quotient and the fold (above); a value with no name and no type of the code's
drawn as a wire from its maker to its takers, a vertex between two arrows contracted (a homotopy, as
in rule 6); a type no line touches left out, since it lies on no border; and a place the route
finds and changes drawn twice, `from X ⇄` on the left and `into X ⇄` on the right, one place with
the side `both`.

Each law is then a theorem of the construction, or it is not yet, and says which:

| Law | Why it holds | What checks it |
| --- | --- | --- |
| Incidence | `F/q′` maps onto `F/q` along the blocks | the oracle (`checkOpenings`), `pictures.py topology`; `engine/tests/plans.rs`, for every set of open steps: each line counts the fine lines that cross a block's border, by channel |
| Containment, Moment | functions of `F` | the oracle, `topology`; `plans.rs`: each box stands where it stands with every step open, and a closed family opens into a frame with the id of its box for each value it stands for (`valueBox`, the walk's rule) |
| Side | decided on `F/q₀` and carried by `q` | `topology` (a place that changes side) |
| Closing | `q` is a function of the set of open boxes | `topology` (closing draws the picture it was) |
| Reading | the own arrow of an opened step is its border | `topology` |
| Endpoints | every image of a place or an occurrence is a box | `pictures.py walk` |
| Across the cut | a cut step's arrow is the image of its body, made by the same construction (`cutArrow`) | `plans.rs` at the budgets 1, 2 and 3 (`ATLAS_FINE_BUDGET`), which cut the small programs as a large one is cut: each line of a cut step counts the fine lines its body draws there, and each cut step of the closed picture, opened and planned anew, refines the picture it was cut in (`cutOpenings` in `plans.mjs`); `topology` with a small budget |
| The overview | not built on a finest plan: the packages' pictures are drawn by their own construction, one box per type (#69) | `walk` only |

The oracle holds the pictures to one another, not to the program. Whether `F` says what the code
does is a test of its own, `engine/tests/plans.rs`: beside a small program a `.plan` file writes down
the plan its code implies, derived from the code and this page, never from what Atlas draws, and every
fact of it must hold of the plan Atlas builds, and no other. What Atlas gets wrong today is marked
with the issue that tracks it, and the mark goes with the fix (`engine/tests/plans/README.md`). The
test runs the viewer's own model in node, the function every picture is drawn from, on every route of
the pairs and of its programs.
**Where.** `plan`, `sitesOf`, `figureOf`, `FIGURE_FIELDS`, `FIGURE_SLOTS` and `figurePart`, `setsAt` and `resolveWhole`, `throughOf` and `partIn`,
`changedThrough` and `changesOwner`,
`coarsen`, `cutArrow` and `typesOf`, `handTo` and `checkOpenings` in `viewer/plan.js`, `cutOpenings` in `engine/tests/plans.mjs`;
the field an argument reads, `Steps::fills` and `Steps::read_of` (`body.rs`); the class each field of a class holds, `Node::holds`;
which parameters of no type of the code's hold a value, `builder::settle_parameters`; the fields a condition tests,
`Steps::tests` (`body.rs`), and whether a figure is made sure of, `guarded` in `viewer/plan.js`; `tools/pictures.py`;
`engine/tests/plans.rs`.

### Saturation: when the order of a view does not matter
**What.** A set is saturated by an equivalence when it never cuts a class in two.
**In Atlas.** A filter and a quotient commute exactly when the filter's predicate is saturated by
the quotient's equivalence. Otherwise the order of a view is meaning, and Atlas takes the filter
first, so the reader decides what exists before the quotient decides how it groups.
**Where.** `design.md` §1, "When order matters"; `engine/tests/laws.rs` holds both orders to each
other on graphs `proptest` draws, and shows a filter that cuts a class in two.

## Type theory

### Types are objects
**What.** A type is a set of values with the operations on them.
**In Atlas.** Each type is an object, Memory(type), and every place carries a *what*: the type,
the path, the table, the command. A module, a class and a function are code, not values: what a
library's module holds as data is the place (`Global(os.environ)`, not `Global(os)`).
**Where.** `design.md` §0; `model.rs`, `World`.

### Containers and type parameters
**What.** A container is a functor on types: `list[T]` holds values of T. A class with type
parameters is one type, whatever its parameters.
**In Atlas.** A generic's arguments are values only where the generic holds or stands for them
(`Vec<Month>`, `Option<T>`, `Promise<Regel[]>`). `DrawingArea<DB, Shift>` and `ndarray[..]` are
each one type, in every language. A type a trait leaves to its implementations (`Self::Item`) is the one each
implementation fixes (`type Item = Book`): in the implementation that type, in the trait any of them. A tuple is the
values it pairs where it stands for one value (`-> (Config, Store)`); a collection of them holds pairs, of a type the
code does not name (`Vec<(NaiveDate, f64)>` holds no `NaiveDate`). A tuple of any length (`tuple[Station, ...]`) is a
collection itself. A map holds its values by its keys, a family of lenses indexed by the key (Lenses): `dict[Mode, Plan]`,
`HashMap<Mode, Plan>` and `Record<Mode, Plan>` are one value that holds Plans, the key choosing the Plan a lookup gives
as an index chooses an element of a list, no value the map holds. What a parameter, a local or a value its module keeps
holds is what its type holds as its language reads it, never a name its type merely writes: the module's `PLANS: Record<Mode,
Plan>` is a value of Plans from beyond, as `PLANS: dict[Mode, Plan]` is, and `const out: Where[] = []` holds Wheres. So is
what a function gives back: one that returns `Vec<(String, Vec<Where>)>` gives back pairs, and none of the Wheres its steps
make. A value declared as the language's container is one, a field as a name (`n.touches: Touch[]` is an array), so a method
called on it is the language's operation.
**Where.** `knowledge.rs`: `python_container`, `TYPESCRIPT_CONTAINERS`, `RUST_CONTAINERS`, the maps `PYTHON_MAPS`,
`TYPESCRIPT_MAPS`, `RUST_MAPS`; `Param::holds`, read by `writesType` in `viewer/plan.js` and by the principles, and
`Returns::holds`, read by `flow::made_from`; the Rust reader's
signatures (`rust/passes.rs`, `fixed_by_implementations`; `rust/module.rs`, `type_names`, with `RUST_COLLECTIONS`); `typescript/module.rs`, `type_names`, with `TYPESCRIPT_COLLECTIONS`, and `typescript/body.rs`, `typed` and `world`; `python/module.rs`, `type_names`, and `pytypes::outer_classes`, with `python_collection`.

### Narrowing
**What.** A test of a value's type refines its type in the code that runs only where the test
holds: there the value lies in the fiber of the test over `true` (flow-sensitive typing).
**In Atlas.** A guard of the language's own (`Array.isArray(x)`) narrows `x` to the language's
container where it holds: on the right of `guard && ..` and of `!guard || ..`, in the arm of an
`if` or a `?:` the guard decides so, and after an `if (!guard)` whose arm leaves. A method on the
narrowed value is the language's own operation. A function written inside is a scope the guard does
not reach. Python's narrowing is ty's.
**Where.** `knowledge::TYPESCRIPT_GUARDS`; `Body::narrowed` (`typescript/body.rs`).

### Wrappers are identities
**What.** An identity arrow `1_A: A → A` hands on its value unchanged; composing with it changes
nothing.
**In Atlas.** An expression that hands on the value inside it is an identity on what Atlas reads of
the value: parentheses, a cast or an assertion (`as`, `satisfies`, `!`), a reference (`&x`), and an
unwrapping (`await`, `x?`, `x.await`), since a promise, a coroutine, an `Option` and a `Result` are read as what
they hold. A value read through any of them is the value inside, so a change, a place or a call
written through a wrapper is the one written without it. Moving a value into a container that holds
one and is read as it (`RefCell::new(x)`, `Box::new(x)`, `Rc::new(x)`) hands on that value: no new
one, as a copy makes (`copy.copy(x)`), so it is no step. A cast also names a type, and where the
question is the class a method runs on (`(x as Store).f()`), that type answers it.
**Where.** `tree.rs`: `Wrappers`, `beneath`, `outermost`; each reader's `WRAPPERS`.

### Behavioural subtyping
**What.** A subtype can stand wherever its base stands (Liskov).
**In Atlas.** An override keeps the promise of its base: it takes at least what the base takes
(parameters contravariant) and gives what the base gives (results covariant), and it is held to
each base along the class's method resolution order, the C3 linearization of its bases in their
declared order. A subclass of a generic base stands where the base stands as the subclass fills
it in: `Boekwinkel(Winkel[Boek])` keeps the promise of `Winkel[Boek]`, the base's `R` replaced by
`Boek` wherever the promise names it. Where a language types structurally (TypeScript), a type
stands where another does when it has every member the other declares.
**Where.** Principle `overrides`; `model::linearization`; for Python ty decides it (`pytypes.rs`).

### Method resolution order
**What.** A linear extension of the order of inheritance: one sequence of a class and its
ancestors in which every class comes before its bases and each class's bases keep the order it
declares them in (the C3 linearization).
**In Atlas.** The method a value of a class runs for a name is the first definition along its
class's order, and building it runs the first initializer along it: what building it takes, and
what a call of the class hands its arguments to (`super(..)` in a constructor hands them to the
one its base runs). The picture's step, the paths a call completes and the rules' comparison read
the same order: in a diamond `D(B, C)` over `A`, a `D` runs the `f` of `C` before the `f` of `A`
that `B` inherits, and `D(..)` runs the initializer of `C` where `D` and `B` have none. A call
through a class (`C.f(x)`, `C.prototype.f.call(x)`) runs the `f` along `C`'s order, on the value it
is handed first, and no override of it. A `super()` call written in `K` runs the next `f` after `K`
along the order of the value's class, not of `K`: in `D(B, C)`, a `super().f()` in `B` runs `C.f` on
a `D`. Where the value's class is not known, it may be `K` or any class below, and the call runs
the next `f` after `K` along each of their orders: one of them, on that class. A call by name on a value
whose class is not known may run, for each class the value may be (the class of the method it reaches or
one below it), the `f` that class runs along its order, on the values that run it: in `D(B, C)`, a call
of `C.f` may run `B.f` on a `D`, though `B` is not below `C`, and there its `super().f()` runs `C.f`. The
alternative is that method on that class, a body as any step on a known class runs it, named by the
class it runs on (`D.f`); one of a class below the method's own runs on any value of its class, whose
class is not known. A call through a class, and a `super()` call on a value whose class is known, run
the one their call names.
**Where.** `model::linearization`, `model::initializer_along`; `principles.rs` (`Index::mro`),
`builder.rs` (`family`, `method_of`, `initializer_of`), `flow.rs` (`method_on`, `method_after`,
`built_from`, `Step::named`, `Ix::overrides`).

### Signatures say the arrow
**What.** A function's type is its domain and its codomain.
**In Atlas.** A package names its doors, and each door says what goes in and what comes out. The
unchecked type (`Any`, `any`) says no more than no type; a door's overloads are the signatures its
callers are checked against. A change to an input, of whatever type, is part of the codomain
(`T → T`) and shows in the signature. A function
whose value is used says what it gives back. A call that changes the value it is called on is
such an arrow inside a body, `T × A → T`: a step that makes its receiver anew from what it is
handed (`rows.append(row)`), so a value gathered into a container is followed to where the
container goes. The value it makes anew is the one it was handed, under the same name and of the
same type, one value at the step's moment; a result of no type of the code's it gives besides goes
back to its caller as any such result does. Taking one out is a step too: a loop takes what it goes over and gives each
element under the name it binds (`for n in g.nodes` is `Graph → Node`), so the element is a value
the body makes, not one from beyond.
**Where.** Principles `doors`, `changes` and `gives`; the steps of a body (`body.rs`), with the
calls that change their receiver in `knowledge::MUTATORS`; a loop's step in each reader (`each`).

## Graph theory

### Reach
**What.** The transitive closure of a relation: everything a path leads to.
**In Atlas.** What a step reaches is the closure of its calls; it is drawn apart from the step's
own arrow.
**Where.** `flow.rs`; sentence 6.

### One direction
**What.** A directed graph without cycles.
**In Atlas.** The packages of a part depend on one another in one direction: no package reaches
back to one that depends on it. The blocks are the quotient the picture draws (`flow::units`): the
packages below one package and that package's own module beside them, and the top-level packages
of a part. A dependence is what the code calls, builds, imports and extends and the types it takes,
gives and holds. A test is a way in from outside, not a layer.
**Where.** Principle `layers`; `Index::test` (`principles.rs`).

### Layered drawing
**What.** A graph without cycles can be drawn in layers, every edge pointing one way.
**In Atlas.** A route is drawn in layers from its entry to the places it reaches.
**Where.** `design.md` §2, the viewer's layout (elkjs).
