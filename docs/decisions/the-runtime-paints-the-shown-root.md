# The runtime paints the shown root, and only when one is named

    status   accepted (2026-08-12), **as built** in story #838 (epic #833,
             slice v0.19); D4 and D6 amended 2026-08-15 by issues #980 and
             #943 (slice v0.20); D3 and D6 amended 2026-08-15 by issues
             #945 and #925, which made the report one statement and gave
             the mapped load a third caller
    scope    `Arena::dfs_order`, `dashscene-engine`'s solve, its glyph
             staging and its #322 baseline pass, `dashlang`'s per-commit
             caches, what each painter is handed, the browser's load
             bound, and R5's status. The default when no root is named.
    issue    #838, building #822 on D2 of
             `the-shown-root-bounds-the-load-not-the-paint.md`
    refs     #779, #798, #822, #836, #837, #937, #943, #980,
             `the-shown-root-bounds-the-load-not-the-paint.md`,
             `the-shown-root-is-named-by-ordinal.md`

## Context

D2 of `the-shown-root-bounds-the-load-not-the-paint.md` adopted the target:
confine the solve, the committed table and the paint to the root that is shown.
D3 named the two pieces the issue did not — a selection concept, built at story
#837, and `Arena::dfs_order` being the shared index space, which is this record.

Two costs were the reason. The browser load could not be bounded
unconditionally, because a payload it did not fetch has no bytes and the painter
still walked every root's rects. And the per-frame cost tracked the document
rather than what was shown: story #836 measured **65 Taffy layout computations
and 65 committed rect rows per frame against a one-root document's 1 and 1** —
65.00x on both — over the sixty-five-root document R5's criterion is stated
over.

## Decision

**D1 — the traversal follows the shown root, and `Arena::dfs_order` is where
that happens.** Its own documentation already said "change it here or nowhere".
`dashscene-engine`'s solve, its glyph staging and its #322 baseline pass follow
the same set, through `Arena::shown_roots`, so the four cannot disagree about
what "shown" covers. Each painter needed no change: they walk the committed
table, and the table is what changed.

**The baseline pass is the one the first cut missed, and it is the reason the
count is four rather than three.** It walked every root and re-solved over every
Taffy root, which does two wrong things at once on a document that carries text.
It reads `tree.layout()` for nodes no `compute_all` computed — a zeroed layout —
and shapes their text against it, inventing a cross-size floor for a row that
was never solved; and its re-solve then computes every artboard, restoring in
full the per-frame cost this record is about. **Neither is visible to the
per-frame band**, which runs `TaffySolver::new()` and returns from the pass
before any of it, so the band read 1.00x while every text document still paid
per artboard. `the_baseline_pass_solves_the_shown_root_and_not_the_document` is
what holds it, in `crates/dashscene-engine/tests/baseline.rs` rather than beside
the band, because it needs a font.

**D2 — nothing is confined until a root is named, and the default is every
root.** `Txn::show_root(None)` is the default and means every root.

This is the decision that is not in the issue, and it was taken on a
measurement. **69 tests in this workspace commit an arena with more than one
root**, and in the ones examined the extra roots are incidental — three sibling
roots as three independent nodes, in a test about paint interning. A default of
"root 0" would have re-topologised all of them for a reason unrelated to what
they assert.

Re-derive it rather than trusting the number: add
`assert!(arena.roots().len() <= 1)` above `let order = arena.dfs_order();` in
`Txn::commit_with`, then run

    cargo nextest run --workspace -P regression --no-fail-fast

The count is that run's failures — 69 across ten binaries, 60 of them
`dashscene-core`'s own. It is a **floor**: a `#[should_panic]` test that reaches
a multi-root commit swallows the probe and is not counted, and the figure is
over the regression tier rather than the whole suite.

**Assert rather than print, and that is the whole reason this says which.** It
was first recorded as 63, by an `eprintln!` in the same place, `--no-capture`,
and each probe line attributed to the next `PASS`/`FAIL` line after it. That
attribution is invalid under a concurrent runner: one test's stderr interleaves
with another's completion line, so every hit landed on whichever test finished
next and several tests' hits collapsed onto one neighbour. A failing assertion
makes nextest name the test itself, so the attribution is the runner's rather
than the reader's. The same error one layer up is reading `grep -c "FAIL \["` as
the count — nextest prints each failure inline **and** again in the summary
block, which doubles it to 138. The `Summary` line is the number.

It is also the more honest reading. A shown root is a property of a
**document**: `ShownRoot` is an ordinal over the roots a `.dsb` declares, and an
artboard is what a producer lowered. A scene built in code has no artboards to
choose between, and its roots are as often several independent pictures the
author wants drawn together as they are alternatives. So a document's host names
one — every host that loads a mapped document does, at the load — and an
authored scene keeps what it always had.

**D3 — a change of shown root is a renumbering event, and it is reported.**
`CommittedScene::renumbered` says the rect indices of this commit mean something
different from the previous commit's, every rect is dirty on such a commit, and
the index maps are rebuilt whether or not the table's length moved. **Every loop
that ticks a `LiveScene`** turns it into `Present::document_replaced` /
`SurfaceRenderer::document_replaced`, which `dashscene-gpu` answers by
forgetting what it uploaded — the three published hosts, and `demo-android`'s
showcase loop, which drives a renderer of its own without going through any of
them.

**It is reported once per commit, against the generation reported — and the rule
has one statement.** `renumbered` describes one commit, and an idle tick returns
from `LiveScene::tick` without committing, so the flag outlives the frame that
raised it and a loop reading it as a **level** reports the same renumbering on
every frame until the scene next moves. That is not a spare call:
`Renderer::forget_uploaded` drops every resident texture along with the uploaded
rows, and the browser loop runs a frame per `requestAnimationFrame` whether or
not anything moved.

**Amended at issue #945.** This originally read "each host holds
`renumber_reported`", and that was the defect rather than the design: the same
six lines sat in `dashscene-desktop` and `dashscene-web`, and `dashscene-ffi`
held no copy at all while AGENTS.md lists three integration surfaces. The gate
is now `LiveScene::take_renumbering`, the way story #810 moved the frame clamp
and the shown-generation gate. Two properties came with the move: the stamp is
still made only once the report **has been**, so a renumbering landing on a
frame with no presenter stays pending for the one that follows — **while the
scene stays idle across the gap**, which issue #1070 records as the limit it is;
and clearing it on a rebuild stopped being a step a host must remember, because
a rebuild makes a new `LiveScene` and the stamp starts clear with it.

The call is `take_renumbering`, which stamps as it answers. A separate
`mark_reported` would be a second call every host must remember, and forgetting
it is precisely the level-read defect above.

The arena stays a **parameter** of that call because every call site already
holds it: a host ticks and reports in the same frame. An earlier draft justified
it by claiming the load's `show_root` commit would need seeding at attach; that
is wrong, and the review caught it. `attach_live` commits once more to seed the
scene, and by that commit `previous_shown_root` already equals the arena's — so
the load's renumbering is **already cleared** before any host ticks, on every
host. A `tick`-recorded field would answer identically.

**That is worth stating on its own, because it is not what D3 implied.** The
tick-side report does not fire after a load anywhere. The load's own eager
`document_replaced` is the only notification on every path, not merely on the
authored-scene path — which makes issue #946's ruling that it must not be
removed stronger than the argument that ruling was made on.

**The painter is not the only consumer keyed on a rect index, and `dashlang` is
the other one.** `LiveScene` holds `cached_solve` and `cached_index`, both built
from the committed table, and before this story they could be built once: a
reflow changes geometry and never the committed order, so a `NodeId` kept its
row for the life of the scene. A renumbering breaks exactly that. Three things
follow, and each was a defect in this story's first cut:

- **A tick that stages a shown root must not take the idle return.** `Txn` has
  no `Drop` that reverts, so `show_root` leaves the arena changed and
  uncommitted the way `set_variant` used to — and a shown-root change moves no
  signal and starts no track, so the idle test would swallow it and the host
  would go on painting the artboard it was already showing. That is issue #617's
  shape on the state this story added.
- **It must commit through the real solver.** The cached arms replay
  `cached_solve`, which holds the rects of the root that _was_ shown, so the
  newly shown subtree would reach `commit_with` with no rect for any of its
  nodes — refused by name, which is the loud version.
- **A node with no committed row is not an error to the binding flush.**
  `attach_live` binds every node the document declares, including those under
  unshown roots, and a contained rect write is `WriteClass::Patch`. The write is
  staged — intent is intent, and it applies if that root is shown — and only the
  patch is skipped, because a patch is an overlay on a solved rect and such a
  node has none.

**A length check is not sufficient and that is the whole hazard.** Two roots of
the same shape give a table of the same length whose rows name different nodes,
so a bit compare finds them equal and reports a clean frame while every pixel
moves. That is issue #798's failure — a comparison reading clean because what
changed lived outside the compared structure — and
`a_change_of_shown_root_renumbers_and_reports_every_rect_dirty` is the test that
fails if the commit is treated as an ordinary one.

**It became that test by being mutated, and it is worth recording that it did
not start as one.** Its first fixture gave the two roots a distinct fill each,
so the entry at row 0 moved when the shown root did and the comparison marked it
dirty on its own — the assertion passed whether or not the renumbering
contributed anything. Removing `dirty_set.extend(0..rects.len())` was measured
against the whole sanity tier: **1 784 tests, all passing, with the mechanism
gone.** The fixture is now two **bit-identical** roots — same geometry, same
fill, so the same interned paint index — which is the one arrangement where the
comparison and the contract disagree, and the assertion reads `[]` against `[0]`
under the same mutation.

**What that does not mean is that a painter draws wrong pixels without it.** A
`RectEntry` is self-contained: two bit-equal entries index the same interned
paint and resolve the same clip, so a row this reports dirty draws today what it
drew anyway. The contract is what is asserted — `dirty` names every row whose
_meaning_ moved, because "did the bits at index i change" is the right question
only for a table whose indices mean the same thing on both sides. What depends
on it is every consumer caching something else against a rect index, which is
what `renumbered` and `Present::document_replaced` are for.

**D4 — a node under an unshown root has no rect index.** `rect_index` names
`NO_RECT` for it and `CommittedScene::rect_index_of` answers `None`, which is
the answer it already gave for a node added after the commit and for the same
reason: this scene resolved no rect for it. The zero default it replaces would
have answered "row 0" — the shown root's own rect — for every unshown node.

**Amended 2026-08-15 (issue #980): the same holds for the transient slot table
the commit builds, which kept the zero default this decision removed.**
`Txn::commit_with` builds a `rect_of_slot` spanning every arena slot but writes
it only for the slots the shown roots' DFS reaches, and it was allocated
`vec![0; n]` — so the reasoning above applied to it word for word and had not
been applied. `LayoutSolver` is a **public trait** and `stage_text` is handed
the whole arena, so a stager walking `Arena::roots()` rather than
`Arena::shown_roots()` reached an unwritten slot, and its run was stamped
`rect = 0` and drawn anchored on the shown root's own box — a wrong picture with
no diagnostic, which is what P4 forbids. The table is now allocated with
`NO_RECT` and `rect_of_slot_checked` refuses the untouched entry by name, at
both of the stager's call sites.

**The rule is narrower than "every slot-keyed table".** What earns a sentinel is
not the key. It is the pair of questions **which slots does the walk write, and
which slots does the read reach** — a table whose reads stay inside what the
walk wrote needs no sentinel, whatever it is keyed by, and `Txn::commit_with`
holds several of both kinds.

`rect_of_slot` is the one that changed answer. It has three reads: two for a
node a stager named, through `rect_of_slot_checked`, and one raw index by a
node's **parent** in the `subtree_end` pass. Before story #838 the walk wrote
every slot, so no read of any kind could reach an unwritten one and a plain
default was never a lie. Confining the walk to the shown roots' subtrees
separated the two sets for the stager-facing reads, and the default did not move
with the walk. **That is the whole defect, and it is a defect of the pair rather
than of either half** — which is why no rule stated over the key alone would
have caught it, and why the check to apply when confining any walk is to re-ask
the second question of every table the walk fills.

The parent read stayed safe across that change, and for a reason that belongs to
the walk rather than to the table: every non-root node the walk visits has its
parent in the same walk. That rests on **two** guarantees, and both are load
bearing. `Txn::add_node` pushes to `Arena::roots` only when the parent is
`None`, and nothing reparents — so a root has no parent. And `Txn::commit_with`
refuses a shown root that is not a member of `Arena::roots`, so the walk cannot
be seeded at an interior node whose parent lies outside it.

**The second is the one a later change would remove.** "Confine the traversal to
a subtree" — showing an interior node rather than a root — is the natural next
step from this record, and it is exactly what turns the parent read into a read
of an unwritten slot: the group-opacity pass would take the sentinel for a row
index and panic with a bare out-of-bounds message naming nothing. Whoever
relaxes that assertion owes that read a sentinel check, and the paragraph above
is the rule that says so. The bindings are deliberately not named here — they
are locals in a private function, and this record has already been wrong twice
by describing that function's internals rather than its behaviour.

This record deliberately does not enumerate the other tables. Two attempts to do
so during review were both wrong, in different ways, about which of them carry
`Option` and which are read only for a parent — an inventory in a normative
record is a claim that goes stale silently and was already false when written.
Read the function.

**D5 — the engine keeps the whole tree and computes part of it.** The retained
Taffy tree is built over every root; only the shown ones are computed and read
back. Building it is a load-time cost, and keeping it whole is what makes a
later change of shown root cheap: no rebuild, because the newly shown nodes are
already in the tree. That change is not an ordinary frame, and the incremental
path says so twice — it does not take the paint-only fast path, and it reads the
new subtree back in **full** rather than pruned, because "pruned" means "what
moved" and nothing about a root that was never shown has moved.

**D6 — the host names the root in a commit of its own, rather than through the
loader.** Every host that loads a mapped document calls `show_root` after
`load_document_mapped` returns. One extra commit per load, against a signature
change on three public loaders and every call site; the load has already
committed by then, so there is no ordering to get wrong.

**Amended at issue #925**, which read "both integration crates" and became wrong
when `dashscene-ffi` gained `ds_runtime_load_document_mapped`: there are three
callers now, and none of them writes the step itself. It is
`dashscene_core::show_appended_root`, which also carries the ordinal correction
the amendment below records.

**Amended 2026-08-15 (issue #943): that commit names the root by `NodeId`, and
the loader is what converts.** `Txn::show_root` took the `ShownRoot` the host
passed and `Arena::shown_roots` indexed `Arena::roots()` with it, which reads a
_document_ ordinal as an _arena_ ordinal. Those agree only when the arena was
empty before the load, and `dashscene_core::load_document` documents the
opposite as supported — the document's nodes are appended to whatever the arena
already holds. An embedder loading into a populated arena therefore had the
prefetch read one artboard's payloads while the traversal followed another. Each
loader now takes the arena's root count before the load and resolves its own
ordinal against the roots that load appended, which is correct because the
loader is the one place holding both the document and the arena. See
`the-shown-root-is-named-by-ordinal.md` D4 for what this does and does not
change about the type itself.

**D7 — the browser's widening is deleted, not made rare, and `Bound` goes with
it.** `shown::layout` returns the shown root's own set for every document, and
`Bound`, `Bound::EveryRoot` and `assets_of_every_root` are all deleted with the
branch that constructed them. The issue's own text expected the variant to
survive as a reportable fact; the compiler disagreed, on both targets, and an
unconstructible variant behind an `allow` would be a claim that the widening can
still happen.

Keeping `Bound` with its one remaining variant was the first answer here and is
rejected, because a type with one inhabitant reports nothing: every
`assert_eq!(layout.bound, Bound::ShownRoot)` in this module's tests becomes an
assertion that cannot fail, which is the near-side reading
`docs/technotes/measured-verification.md` names. D4 of the earlier record asked
a host to say **which** bound it took, and that question has one answer now.
What those tests assert instead is the fetched set and the byte count — both of
which move with the ordinal since this story, and neither of which did before.

**What the widening was protecting is real and outlives it, so it is stated in
prose where it bites rather than carried as a variant.** A browser load fetches
nothing for the roots it did not show, so **the root named at load is the only
root that can be shown** on that target: naming another afterwards points the
paint at image-table rows that name no bytes, and `dashscene_gpu::residency`'s
`decode_png` is handed an empty slice. A mapped desktop load binds a real range
for every entry and draws it instead — handing the painter bytes nothing hashed,
which is the narrow remainder of debt #779. No host offers the switch today;
`Txn::show_root`'s own documentation and `crates/dashscene-web/src/shown.rs` are
where the first one that wants it will find what it costs.

## Consequences

- **The per-frame band reads 1.00x on both terms**, from 65.00x. The
  before-number is still measured on every run:
  `the_confinement_is_what_makes_the_number_one` clears the shown root and
  reports 65 again. That guard replaced story #836's scaling guard, which ran
  the same frames over a seventeen-root document — a test that worked only while
  the terms tracked the document's size, and had to be retired because they no
  longer do.
- **R5's document-shape qualification is gone.**
  `docs/specification/05-qualification.md` stated the criterion as holding "on
  native for any document, and on the web for a document whose unshown roots
  draw no asset". It now holds on both targets for any document. The
  **mechanism** still differs by target, and that half of the note stands.
- **Debt #779 closes.** A mapped load still binds one image-table row per asset
  entry and still hashes only the shown root's, so unshown rows are still
  unverified — but nothing can reach them, which was the risk. The coupling is
  recorded where the prefetch is: the rows are safe **because** the traversal is
  confined, and a load naming no root would bind the same rows and paint them.
- **Issue #937 closes as obsolete.** It recorded that the browser's widened
  bound was O(roots x nodes) with an allocation per root; the function is
  deleted.
- **Issue #825's payload gate is unblocked**, which `docs/roadmap.md` records as
  waiting on this.

## Alternatives considered

**Default to root 0, so every arena is confined.** The most literal reading of
"the runtime paints the shown root". Rejected on the measurement under D2: 69
tests build sibling roots as a way to hold independent nodes, and
re-topologising them would change fixtures for a reason unrelated to what they
assert — while delivering nothing the opt-in does not, because every path R5
concerns loads a document and names a root.

**A parameter on the loaders rather than a commit of its own.** One commit per
load instead of two, and "a loaded document has a shown root" as a type-level
fact. Rejected for its blast radius: three public functions in `dashscene-core`
and every call site in the goldens, the FFI and the tests, for a saving of one
commit that happens once per load rather than per frame.

**Detect the renumbering from the table's length.** No new field, and it is
already computed for the structural check. Rejected because it is wrong for the
case that matters: two roots of the same shape.

**Keep `Bound::EveryRoot` behind an `allow(dead_code)`, or keep `Bound` with the
one variant that survives.** Either honours issue #838's expectation that the
type stays in the API. Both rejected under D7, and for the same reason from
opposite ends: the first claims a widening that can no longer happen, and the
second turns every assertion over it into one that cannot fail.

## What this does not decide

- **Whether a host may show a set of roots.**
  `the-shown-root-is-named-by-ordinal.md` D1 settled one at a time, and nothing
  here reopens it. `show_root(None)` is not a set — it is the absence of a
  choice, and it means every root.
- **What a host should do when the shown root changes mid-scene.** The contract
  is here and every host honours it; no host in this repository changes the
  shown root after the load, so nothing has exercised the transition outside the
  tests. The first host that wants to is where the frame-pacing question gets
  asked.

  Issue #945 narrowed that gap without closing it: the report is now one gate
  every host reads rather than a copy per host, so the transition has one
  implementation to exercise instead of three. The C ABI is the sharpest case —
  it names the root once, inside the load, and offers no symbol to change it —
  so the gate is correct there and unreachable, deliberately.
