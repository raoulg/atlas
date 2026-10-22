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
| `side PLACE SIDE` | where a place of the world stands, decided on the closed picture: `left`, `right`, `between` (also a place the code goes to without saying which way, in neither domain nor codomain), `both` |

**Occurrences.** A path is the route's name and the step labels below it: `run/tidy/strip`. A constructor is `Table()`,
a method `Class.method`, an override in a call into a family `Circle.draw` below the call. A choice is `?1`, `?2` in
order in its body, and its arms stand below it (`pick/?1/load_csv`). A second call of one name in one body is
`write_to#2`. In `F` every occurrence with calls is open. As the end of a line it is its own arrow, a point on its
border that keeps what the body itself takes and what it gives back, at the step's moment. A value the step's call is
handed crosses the frame to the step inside that takes it. A call into a library the program does not read is an
occurrence where the body names what it gives (`con = duckdb.connect(path)`) or where it changes the value it runs on
(`rows.append(row)`), labelled by the operation (`connect`, `append`); otherwise what it gives goes into the call around
it. What a library operation touches at a place is its caller's own touch, whether or not it is a step: naming its
result changes no line to the place (`design.md`, "An opened route is the fiber of its calls").

**Values.** A value is `TYPE·NAME@MOMENT`. The type is the code's name for it (`Table`, `Circle`); a value of no type of
the code's is `_`. The name is the one that holds it; a value no name holds has none. The moment is the occurrence that
made it, or the choice for the value a choice hands on that two of its arms make, or that one arm makes anew of a value
named before it (`pick/?1` in a body, `Store()/Store.get/?` for the overrides a call into a family may run, below the
call); what one arm makes and uses itself is its maker's. A value no step makes, from beyond, has no moment. A route's
parameters are values from its start, by their names: `Circle·c`. In the route of an exported class each public method
is called by the entry and runs on the caller's instance, one value from beyond (`Store`); what the constructor door
builds goes back to the caller. What a step reads back inside itself of what its own
writers wrote is `PLACE in STEP` (`File(cache.json) in main/refresh`), made by the writers and taken by the reader.
Two values the names do not tell apart are told apart by a count, `#2`. A call is handed the values the names in its
arguments hold: `f(x.name)` is handed `x` (a projection is no occurrence; it is glued into the call). A nested call that
is a step of its own takes its names itself, and the call around it is handed what that step gives. A projection bound to
a name first is still the projection (`width = settings.width; f(width)` hands `settings`). An error is a value: its
constructor makes it in the arm that raises it, and each border it crosses uncaught hands it on, up to the route.

**Places.** A place is drawn as Atlas names it: `File(data/out.csv)`, `Screen`, `Disk(bump.path)`, `entry` (where the
route starts).

**Channels.** `value` (a value made or taken), `own` (a step's own effect at a place), `reached` (an effect through a
step below, only where the plan is cut), `call` (the entry calls the route), `back` (a result of no type of the code's
goes on to the step that called it, where no other step of the caller takes it: one line, labelled in the picture by
the name that holds it, and no value `_·name` beside it).

## What Atlas gets wrong today

A fact Atlas gets wrong is written down as it should be and marked with the issue that tracks it:

```
known #100 line Store > Store()/Store.get/Shelf.get value          # a fact that does not hold
known #74 extra line run/Store() > File(data/store.txt) reached    # a fact Atlas has and should not
known #N fault Page@Doc()/Doc() stands in                           # a fault of the pictures, matched as part of its text
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
| `path-pattern` | a pattern below a directory is one place, for the listing and for what is read of what it yields (R3) |
| `check-after-fill` | a place read only to check it stays in the codomain, also where an arm filled it first (R4) |
| `read-logged`, `with-open` | a read handed to a call, or through `with .. as`, is used: the place stands between (R4) |
| `loop-reads-then-writes` | a read in a loop finds what a later round writes over: both (R4) |
| `callee-reads-back` | a callee that reads back what its caller wrote changes nothing the route found (R5) |
| `recursion-order-a`, `-b` | a recursive program draws the same whichever function is defined first (R5) |
| `readback-one-body`, `two-steps` | a place a step writes and reads back stays in its codomain; one a later step reads is between (R4) |
| `what-holds` | a place read and changed stands twice, a choice is a coproduct, a call into a family a choice (Z13) |
