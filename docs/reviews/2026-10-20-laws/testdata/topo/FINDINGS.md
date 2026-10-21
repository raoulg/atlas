# Topology findings (viewer), Atlas 0.68.0 (8c5cc49)

Harness: probe.py (render, open, dump atlasPicture/atlasPlan, screenshots), evalpage.py, topo1.py (pictures.py TOPOLOGY on one box).
Atlas's own `tools/pictures.py topology` on m1..m6: all green (0 faults, 0 more).

T1 m1-readback-inside — rule 4 (reading back) claimed as theorem; code keeps world places unversioned and EXEMPTS the line
   (imageOf drop "back"; refines skips gone==="back"; pictures.py `wrote`). Closed: refresh → File (mid, nothing out).
   Open: save → File → load (line crosses frame inward, file moves to left). Known: zoom-quotient comment "rule 4 back-read exemption stays".
   Side "mid" although mathematics.md: read back → stays in codomain. roleOf: reads&&writes, !found, used → mid.
T2 m2-own-and-reached — closed export: ONE solid line ×2 to out.csv (own append + reached write_header). Sentence 6 / rule 3:
   channels stay separate. imageOf add(): m.get(id) && r ; addEdge key a>b, e.reach &&= info.reach. #49 closed claiming "channels stay separate" (4b83e02 kept &&=).
T3 m3-dotted-packages — dotted "calls, no data" defined with a negation (joinedIn). Closed app: no app→lib dotted (Rec joins).
   Open app: app.a → lib dotted appears = line the closed picture lacks (law 1). Package views: no oracle (plan() returns planBase(false)); topology tool checks routes only.
T4 m3 view 2 — open lib: make → Rec@out (dead end inside lib), b takes t:Rec from lib's ghost own pill. Unit pills unversioned (planBase skips "unit:"),
   route pills inside versioned (at = "out"). One value drawn as two boxes; producer and consumer disconnected.
   Also: own pill "app"/"lib" inside an opened package frame duplicates every crossing line (Rec→b and Rec⇢app).
T5 m4-family-splits-siblings — closed family Shape: merged pill "show_circle, show_square". Open family: two pills. A box that does not lie in the
   opened family is split ("only the box opened is split"). Merged pill has no ×2 (Cn/Dn not summed in sibling merge).
T6 m4 — route report(c: Circle, q: Square): own arrow takes only Circle once Shape is open. oneOfKind() in planBase: two unnamed values of one kind
   (kindKey = kindOf family) collapse; F itself loses the line Square → report. Oracle cannot see (wrong F, consistent quotient).
T7 m5-two-of-one-type — combine(left: Table, right: Table): join's inputs folded as "join · +2 types" (rule 6) though doc: "the route's own values are
   not of the block"; route's own pill takes t:Table@join (the value its body made): own pill versioned after its children → latestGet gives body output.
T8 m6-own-input-moment — open tidy: value strip makes inside tidy drawn OUTSIDE tidy's frame (userFrame(own pill) = parent frame). Decision table:
   "memory made and used inside a step lives inside its frame". Lines cross frame out and back in.
T9 m6 — closed strip block Dn=2 for one handed value: own pill (signature) + body step both count. Multiplicity is not the count of uses in the code.
T10 self (atlas.main) — FINE_DEPTH=6, fineCut=true, 4524 pills; opening atlas.main/11/0/0/0/0/0 grows F to 4537. mathematics.md: "one finest plan F,
   built once from the code and independent of what is open" — false beyond the budget (disclosed in #36 comment (4); #48 closed). Sample opening refined OK.
T11 m7-folds — dir fold data/ (8 files: raw/* read, report/* written) side "mid": roleOf ORs member flags (found from raw, writes from report).
   Picture: publish → File(data/) → gather, i.e. publish drawn BEFORE gather against call order (fake path, rule 5's own example).
   Opening the dir changes sides (data/raw/ in, data/report/ out). Same for SQLite group (users read, runs written) → mid.
   Not checked by any tool (targets(): steps and families only).
T12 m7 vs m7p — flat single module: [project.scripts] main is "Library main" (export marked first; mark_scripts entries.rs:112 skips n.entry.is_some()),
   argparse steps drawn, no usage line. Package layout: CLI `harvest --out`, parse folded. Isomorphic forms, different pictures.
T13 No CI / cargo test runs the viewer quotient or oracle; only manual tools/pictures.py with ATLAS_ORACLE. "more" lines never fail; `wrote` exempt.
   CLAUDE.md still says more is tracked in #38 (closed).
HOLDS m8-holds — twin ⇄ for read-then-write (bump), coproduct of arms (pick: one Table~t at ~c0), override dispatch "one of" (draw_all).
