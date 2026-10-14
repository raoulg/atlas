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
choice between alternatives is made once, in a place with a name.
**Where.** Principles `sums` and `choices`.

### Lenses
**What.** A lens on S has `get: S → A` and `put: A × S → S`, and a lawful one gets back what
it puts: `get(put(a, s)) = a`.
**In Atlas.** A class that writes a place and reads it back is a lens on that place. Atlas asks
the law at the level of types: what goes in as a `Graph` comes back as a `Graph`. A view is the
`get` of a lens with no `put`: it only reads.
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

### Yoneda: what changed
**What.** An object is determined up to isomorphism by the arrows into it, `Hom(−, A)`, and
dually by the arrows out of it.
**In Atlas.** Between two revisions, a place or a type is changed when its direct arrows differ,
even when its name stays. A function is changed when its own arrow differs. An object that
disappears and one that appears with isomorphic direct relations are one object renamed, when
the match is unique. In a picture, Yoneda is applied to the quotient it draws: a closed box (a
part, a package, a family, a place that stands for others) is one object, and its own arrows are
those that cross its border. It is changed when they differ; what changes inside it without
reaching the border is counted beside it, not marked on it.
**Where.** `atlas history`, `atlas changes A..B`; the diff; `design.md` §4, L5.

## Topology

### Gluing: zooming out
**What.** A quotient space glues the points an equivalence relates into one.
**In Atlas.** Zooming out glues each family of types into one point, one box.
**Where.** `design.md` §0, "Zoom is a quotient on types".

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
what the base gives.
**Where.** Principle `overrides`.

### Signatures say the arrow
**What.** A function's type is its domain and its codomain.
**In Atlas.** A package names its doors, and each door says what goes in and what comes out. A
change to an input is part of the codomain (`T → T`) and shows in the signature. A function
whose value is used says what it gives back.
**Where.** Principles `doors`, `changes` and `gives`.

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
