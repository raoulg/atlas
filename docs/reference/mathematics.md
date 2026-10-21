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
handed to a call, kept in a field) through the names it is bound to, or it only decides a choice,
goes to an error or is dropped, and is a check. A place one step
writes and a later step reads and uses is the object between them, and stands between them. A
place the route reads and then writes on is in both: two projections of one place, `from X` on
the left and `into X` on the right. A place it writes only before it reads there, and only in an
arm (a file downloaded where it is missing, a cache filled on first use), it makes sure of: what
it reads there is its input, and the place stands between the step that fills it and the step
that reads it. A read is preceded by a write only when the write happens whenever the read does,
not in one branch of a choice the read is outside. Making a place's shape (a directory, a table's
schema) or testing it (whether a file is there, whether a value holds a key) puts and takes no
content. A connection that opens a database on a file names that file, so a directory made for the
file is made for the database. Where the code does not say which way a call goes, the place is in neither.
**Where.** A function's `reads_back` and `updates` (`flow.rs`); a read's `checks` (`Steps::mark`,
`body.rs`, with each language's `UseSyntax`); the viewer's `roleOf`.

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
**Where.** Principle `nothing`.

### Coproducts
**What.** A coproduct A + B: a value is one of the alternatives, and a function out of it is a
case for each.
**In Atlas.** A sum type (`Result`, `Option`, a union) is taken apart where it arises, and a
choice between alternatives is made once, in a place with a name. A choice in a body is a
coproduct of its arms, each injected by what decides it as the code writes it (`when not
raw.exists()`, a `match` pattern, `otherwise`). A choice of one arm is a coproduct with the
identity, `f + 1`: its steps run when the condition holds and nothing runs otherwise, so it is
drawn as a choice too.
**Where.** Principles `sums` and `choices`; `Node::whens` (`body.rs`), `Item::Choice` (`flow.rs`),
the MCP tool `choices`.

### Lenses
**What.** A lens on S has `get: S → A` and `put: A × S → S`, and a lawful one gets back what
it puts: `get(put(a, s)) = a`.
Lenses compose: a lens `S ⇄ B` followed by the projection of `B` onto one of its fields `B ⇄ A`
is a lens `S ⇄ A`. Through a variant of a sum the projection is partial (a prism), and the
composite gives `A` back when the value is that variant; a sum given a name is the same sum, so
the prism goes through the name. A family of lenses `S ⇄ A_k` indexed by
a key `k` (a row by its id, a tile by a point it holds) takes the key to choose the lens, not as
the value it puts.
**In Atlas.** A class that writes a place and reads it back is a lens on that place. Atlas asks
the law at the level of types: what goes in as a `Graph` comes back as a `Graph`, as it is or
inside what a get gives (`has` and `variant` edges). What a put takes is what reaches the place
through its body's steps, by what each is handed and gives; a parameter handed only to reads is
a key. A view is the `get` of a lens with no `put`: it only reads.
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
beneath it.
**Where.** `CLAUDE.md`, "Isomorphic is the same"; `Entry::options`.

### Paths as terms, directories as fibers
**What.** A path is a term in a small algebra over a root: a root (a literal, a parameter, an
option's default) closed under `/`-join, `parent`, `with_suffix` and a pattern below a
directory. A collection of paths is the set of terms its elements can be; iterating it yields
one of them. Substituting a parameter by the term a caller hands it is the term's evaluation, and
a part no code determines is a named hole. A pattern below a directory (`D.glob("*/IO-*")`) is
the fiber of `D` over the predicate "matches the pattern": the part of the directory the pattern
holds.
**In Atlas.** A place on the disk the code names only through a variable is drawn as the term it
evaluates to (`assets/regelingen/*/IO-*`), not as the variable (`m`). The root is traced to a
default or to a parameter wherever the code says it; what it cannot trace stays a hole in the
term, so the gap is visible. A file the term decides to the last part is a `File`; a pattern is
the fiber below its directory.
**Where.** `paths.rs` (`Seg`, `join`, `place`), `python/paths.rs` (`Eval::of`); the pairs
`python/paths`.

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
marks that step, never `smooth(raw)` after it. A place a caller makes whole is the caller's
arrow; a file named by a function's own parameter (`load.path`) is named so whoever calls the
function. A rename is an isomorphism of the typed arrows in and out: `list[float] → float` is
no `str → str`. `atlas changes` reads the same union along each route, so the picture and the
list hold the same marks.
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
   rows stands where its other end products stand.
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

**Where.** `homeOf`, `madeAt` and `addEdge` in `viewer/index.html`; `tools/pictures.py topology`
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
screen, at the zoom the reader chose.

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
| files fold into the directories that hold them, one box per side their files stand on; opening a directory refines the fold | the disk is a tree, and a fold is its quotient by directory, met with the side |
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
takes them by their names, and a step handed one by its name takes the same value. A closed family holds its kinds (a subclass stands in its base
until the family is opened); a part of a type is structure of its owner, a box of its own in the
owner's frame, and the values of the part's type stand on their own (#39).
**Where.** `plan`, `imageOf`, `refines` and the camera in `viewer/index.html`;
`tools/pictures.py topology`.

### One fine plan, every picture its image
**What.** A quotient is not computed from what is drawn of it. A map `q: G → G/π` is defined on the
fine graph `G`, and the coarse graph is its image: its objects are the blocks, its arrows the images
of the arrows of `G`, and an arrow whose two ends lie in one block is gone. Every notion the coarse
graph needs (where an object stands, which side it is on, when it was made) is a function of `G` and
`q`, so two coarse graphs of one `G` are images of one thing, and refining `q` can only split.
**In Atlas.** A route has one finest plan `F`, built once from the code and independent of what is
open:

- every call of the route is an *occurrence* (a step key, its path through the route), opened to
  all its calls; an occurrence with calls has a frame and its own arrow, one without is a pill;
- every value is an identity `(type, name, moment)`: the type it has, the name that holds it, and
  the occurrence (or choice) that made it; two values of one type at one moment are two; a route's
  parameters are values with their names at the route's start;
- every place of the world is at its concrete place (a path, a table, a command); what a step reads
  back of what it wrote there is a value at a moment inside the step (rule 4);
- a step's own arrow is its witness at its border: its signature and its own touches, the lines that
  cross its frame. An opened step draws it as a point on the frame's right edge, not as a step beside
  its steps: a value its call is handed crosses the frame to the step inside that takes it, and the
  point keeps what the body itself takes and what it gives back, at the step's moment.

What the reader has open is the map `q`, nothing else: an occurrence goes to its outermost closed
ancestor (or to itself), a type to the box its family stands in (`placeOfType`), a world place to its
group, its fold, or itself, a value's moment to the occurrence it goes to, and the steps that only
parse the command line to the entry. The closed picture is `F/q`:

- a closed step's domain is the places that the occurrences below it take and no occurrence below it
  makes; its codomain is what its own arrow hands on and what the occurrences below it write. This is
  the domain of the composite, "the parameters plus what the body consumes" (the second sentence),
  not the signature alone. A line comes from the step itself when its own arrow supplies it and
  reaches through it otherwise (the sixth sentence);
- a place made and taken inside one block is internal to it and is not drawn. What a step makes is never
  its input: the value the arms of a step that is itself a choice make, at the step's own moment, is
  what the step hands on, not what it takes;
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
  the caller has one, else the latest of its kind the body made. The cut changes how much of `F` is
  held, never its picture, and opening a cut step refines as opening any step does.

On the image the page then does only what is a quotient of `F` or a way of drawing it: the merge of
siblings, the storage quotient and the fold (above); a value with no name and no type of the code's
drawn as a wire from its maker to its takers, a vertex between two arrows contracted (a homotopy, as
in rule 6); a type no line touches left out, since it lies on no border; and a place the route
finds and changes drawn twice, `from X ⇄` on the left and `into X ⇄` on the right, one place with
the side `both`.

Each law is then a theorem of the construction, or it is not yet, and says which:

| Law | Why it holds | What checks it |
| --- | --- | --- |
| Incidence | `F/q′` maps onto `F/q` along the blocks | the oracle (`checkOpenings`), `pictures.py topology` |
| Containment, Moment | functions of `F` | the oracle, `topology` |
| Side | decided on `F/q₀` and carried by `q` | `topology` (a place that changes side) |
| Closing | `q` is a function of the set of open boxes | `topology` (closing draws the picture it was) |
| Reading | the own arrow of an opened step is its border | `topology` |
| Endpoints | every image of a place or an occurrence is a box | `pictures.py walk` |
| Across the cut | a cut step's arrow is the image of its body, made by the same construction (`cutArrow`) | `topology` with a small budget (`ATLAS_FINE_BUDGET=1`, `=3`), which cuts the small programs as a large one is cut |
| The overview | not built on a finest plan: the packages' pictures are drawn by their own construction, one box per type (#69) | `walk` only |

What none of these checks is whether `F` says what the code does: the oracle holds the pictures to
one another, not to the program. That is a test of its own, a program with the plan its code
implies, compared with the plan Atlas builds (#52).
**Where.** `plan`, `coarsen`, `cutArrow` and `checkOpenings` in `viewer/index.html`; `tools/pictures.py`.

### Saturation: when the order of a view does not matter
**What.** A set is saturated by an equivalence when it never cuts a class in two.
**In Atlas.** A filter and a quotient commute exactly when the filter's predicate is saturated by
the quotient's equivalence. Otherwise the order of a view is meaning, and Atlas takes the filter
first, so the reader decides what exists before the quotient decides how it groups.
**Where.** `design.md` §1, "When order matters".

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
each one type, in every language.
**Where.** `knowledge.rs`: `python_container`, `TYPESCRIPT_CONTAINERS`, `RUST_CONTAINERS`.

### Behavioural subtyping
**What.** A subtype can stand wherever its base stands (Liskov).
**In Atlas.** An override keeps the promise of its base: it takes what the base takes and gives
what the base gives. A subclass of a generic base stands where the base stands as the subclass
fills it in: `Boekwinkel(Winkel[Boek])` keeps the promise of `Winkel[Boek]`, the base's `R`
replaced by `Boek` wherever the promise names it.
**Where.** Principle `overrides`; for Python ty decides it (`pytypes.rs`).

### Signatures say the arrow
**What.** A function's type is its domain and its codomain.
**In Atlas.** A package names its doors, and each door says what goes in and what comes out. A
change to an input is part of the codomain (`T → T`) and shows in the signature. A function
whose value is used says what it gives back. A call that changes the value it is called on is
such an arrow inside a body, `T × A → T`: a step that makes its receiver anew from what it is
handed (`rows.append(row)`), so a value gathered into a container is followed to where the
container goes.
**Where.** Principles `doors`, `changes` and `gives`; the steps of a body (`body.rs`), with the
calls that change their receiver in `knowledge::MUTATORS`.

## Graph theory

### Reach
**What.** The transitive closure of a relation: everything a path leads to.
**In Atlas.** What a step reaches is the closure of its calls; it is drawn apart from the step's
own arrow.
**Where.** `flow.rs`; sentence 6.

### One direction
**What.** A directed graph without cycles.
**In Atlas.** The packages of a part depend on one another in one direction: no package reaches
back to one that depends on it.
**Where.** Principle `layers`.

### Layered drawing
**What.** A graph without cycles can be drawn in layers, every edge pointing one way.
**In Atlas.** A route is drawn in layers from its entry to the places it reaches.
**Where.** `design.md` §2, the viewer's layout (elkjs).
