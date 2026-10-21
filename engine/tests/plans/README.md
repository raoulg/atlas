# Plans: what the finest plan of a route must say

The page's oracle holds the pictures of a route to one another: each is a quotient of one finest plan `F`, and
opening a box refines it. Whether `F` says what the code does is a question of its own, and these files answer it
for small programs (`docs/reference/mathematics.md`, "One fine plan, every picture its image"; issue #52).

A `.plan` file lies beside the program it describes: `inside-a-step.plan` beside `inside-a-step/` here, and
`clean.plan` beside `clean/` in a pair under `../pairs`. It is written from the code and the mathematics, never from
what Atlas draws. `plans.rs` builds the plans in node with the viewer's own model (`viewer/plan.js`, through
`../plans.mjs`). Every fact the file writes down for a route must hold, and no other. Every route of every program
under `pairs` and `plans`, with or without a file, is also asked for its pictures: for every set of open steps, the
oracle and the quotient must find nothing.

```bash
cargo test -p atlas-engine --test plans -- --nocapture                              # every program
ATLAS_PLANS=pairs/pipeline cargo test -p atlas-engine --test plans -- --nocapture   # the programs under one path
```

## The facts

One fact per line; `#` starts a comment, at the start of a line or after a space.

| Fact | Says |
| --- | --- |
| `route ID` | the facts that follow are of the route of this entry (its node's id: `clean.run`) |
| `step PATH` | an occurrence: a call of the route, by the names on its path from the route |
| `choice PATH` | a choice in a body, a frame of its own with its arms inside |
| `line A > B CHANNEL` | a line of the plan, from what makes to what takes |
| `home VALUE FRAME` | the frame a value stands in with every step open (`outside` when no frame holds it) |
| `side PLACE SIDE` | where a place of the world stands, decided on the closed picture: `left`, `right`, `between`, `both` |

**Occurrences.** A path is the route's name and the step labels below it: `run/tidy/strip`. A constructor is `Table()`,
a method `Class.method`, an override in a call into a family `Circle.draw` below the call. A choice is `?1`, `?2` in
order in its body, and its arms stand below it (`pick/?1/load_csv`). A second call of one name in one body is
`write_to#2`. In `F` every occurrence with calls is open. As the end of a line it is its own arrow, a point on its
border that keeps what the body itself takes and what it gives back, at the step's moment. A value the step's call is
handed crosses the frame to the step inside that takes it. A call into a library the program does not read is no
occurrence: what it touches is its caller's touch.

**Values.** A value is `TYPE·NAME@MOMENT`. The type is the code's name for it (`Table`, `Circle`); a value of no type of
the code's is `_`. The name is the one that holds it; a value no name holds has none. The moment is the occurrence that
made it (or the choice, for a value either arm makes); a value no step makes, from beyond, has none. A route's
parameters are values from its start, by their names: `Circle·c`. What a step reads back inside itself of what its own
writers wrote is `PLACE in STEP` (`File(cache.json) in main/refresh`), made by the writers and taken by the reader.
Two values the names do not tell apart are told apart by a count, `#2`.

**Places.** A place is drawn as Atlas names it: `File(data/out.csv)`, `Screen`, `Disk(bump.path)`, `entry` (where the
route starts).

**Channels.** `value` (a value made or taken), `own` (a step's own effect at a place), `reached` (an effect through a
step below, only where the plan is cut), `call` (the entry calls the route), `back` (a result of no type of the code's
goes on to the step that called it).

## What Atlas gets wrong today

A fact Atlas gets wrong is written down as it should be and marked with the issue that tracks it:

```
known #100 line Shape·s > draw_all/Shape.draw/Circle.draw value    # a fact that does not hold
known #74 extra line run/Store() > File(data/store.txt) reached    # a fact Atlas has and should not
known #102 fault Page@Doc()/Doc() stands in                         # a fault of the pictures, matched as part of its text
```

The test reports each mark and passes. It fails once the fact holds, or the extra fact or the fault is gone, so the mark
goes with the fix. A `known #N fault` written before any `route` holds for every route of the program.

## The programs here

The review of 2026-10-20 ("Atlas tegen zijn wetten", `docs/reviews/2026-10-20-laws`) gave the programs, each the
smallest that showed one law:

| Program | What it shows |
| --- | --- |
| `inside-a-step` | what a step makes inside and hands back stands in its frame (Z8) |
| `readback-inside` | a read back inside a step is a value at a moment inside it; the place stays in the codomain (Z1) |
| `own-and-reached` | a step's own effect and what it reaches are two channels to one place (Z2) |
| `two-of-one-family` | two parameters of one family are two values (Z6) |
| `two-of-one-type` | the route's own parameters are values from its start (Z7) |
| `two-callers` | what a step reaches is the closure of its calls in the occurrences, not the functions (R1) |
| `builds-a-class` | building an object runs its constructor, not its methods (R2) |
| `path-hole` | a directory no code decides is a hole in the path (R3) |
| `readback-one-body`, `two-steps` | a place a step writes and reads back stays in its codomain; one a later step reads is between (R4) |
| `what-holds` | a place read and changed stands twice, a choice is a coproduct, a call into a family a choice (Z13) |
