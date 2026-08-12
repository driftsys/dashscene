# The shown root bounds the load; the paint follows it in v0.19

    status   accepted
    date     2026-08-08
    scope    `Arena::dfs_order`, `dashscene-engine`'s solve, every painter's
             walk of the committed table, and the load bound in both
             integration crates; R5's status on each target
    issue    #822, ruled at the close of slice v0.17 (epic #793), story #796
    refs     #594, #598, #779, #792, #821, #825,
             `startup-scaling-is-measured-by-a-counter.md`,
             `verification-moves-from-open-to-touch.md`,
             `the-integration-surface-is-two-published-crates.md`

Story #792 bounded the browser load by the root that is shown and found that the
bound cannot be unconditional, because nothing below the loader knows which root
is shown. That finding was filed as issue #822. This record rules on it rather
than carrying it as debt: what the runtime does today is deliberate, what it
should do instead is named, and R5's status is restated in the form that is true
on both targets.

## Context

**The runtime paints every root**, verified at three sites rather than assumed:

- `crates/dashscene-engine/src/lib.rs:372` — the layout solve runs
  `for &root in arena.roots()`, inside `rebuild`, which `solve` calls. The
  module's own documentation calls roots "independent coordinate islands" (line
  7). Glyph-run staging does the same at line 212, in `stage_text`, and the
  count matters: `arena.roots()` is iterated at six sites in this file, so
  "every root" is the engine's habit rather than one loop.
- `crates/dashscene-core/src/arena.rs:975` — `Arena::dfs_order` seeds its stack
  from **all** roots, so `CommittedScene::rects` holds every root's subtree in
  one table with no filter.
- `crates/dashscene-gpu/src/render.rs:2152` — `Renderer::resolve_frame` iterates
  `buffer.instances()`, every instance, with no notion of which root is shown.
  `dashscene-skia` walks the same table.

**And nothing selects a root** — the state this record was written against, on
2026-08-08. Both integration crates called `dashbuf::prefetch::first_root`, so
"the shown root" meant "root 0" everywhere it was used. It was a bound on the
load and a synonym for the first root; it was not a choice any host made and not
a value anything below the loader read.

**Settled by story #837 (2026-08-12), which is the first half of D3.** A host
now names a `dashbuf::prefetch::ShownRoot` and both integration crates take one
—
[`the-shown-root-is-named-by-ordinal.md`](the-shown-root-is-named-by-ordinal.md).
Everything else in this section still holds unchanged: the selection bounds the
**load**, and the three sites above still cover every root. That is D2, and it
is story #838.

## What v0.17 measured, and the part that is easy to misread

The native host survives painting every root because it maps the file: it binds
a real byte range for every asset entry and only _hashes_ the shown root's, so
an unread row still decodes. It is merely unverified, which is debt #779.

A browser has no such free addressability — a payload that was not fetched has
no bytes at all — so story #792 ships a guard. `dashscene_web::shown::layout`
reads the shown root's assets only when no other root draws one, and otherwise
reads the union over every root, reporting which through `Bound`.

**The criterion's own document is in the widened class, and this is the fact the
close turns on.** `goldens/tooling/tests/startup_scaling.rs` builds its
many-frame document as sixty-five root frames each drawing a distinct tile —
`frame()` sets `parent: None`, and its doc comment says why: "a document with n
of these is a document with n top-level frames — the shape a Figma file with
many artboards compiles to, and the shape the criterion is stated over." On the
web that document takes `Bound::EveryRoot` and reads all 1 935 927 B of it.

The web fixture that passes is `many_frames(64, false)`: sixty-four asset
entries in the file, one of them drawn. That is a real falsification of cost
scaling with file size — the file carries 262 144 B of payloads and the load
reads 4 096 B, and it was demonstrated failing at 64x first. It is not, however,
the document epic #594 measured. The shape native passes over is precisely the
shape web widens on.

## Decision

**D1 — painting every root is the architecture as designed, and is recorded as
such.** Roots are independent coordinate islands and a multi-root document is
what a Figma file with several artboards compiles to. Nothing about the current
behaviour is a defect against a rule this project wrote down, and #822 is
therefore a change of intent rather than a bug fix. Reading it as a bug is what
would let it be applied without the two consequences under D3.

**D2 — the intended end state is that the runtime paints the shown root, not
every root.** #822's direction is adopted as the target: confine the solve, the
committed table and the paint to the root that is shown. It is adopted because
the alternative leaves two costs proportional to file size rather than to what
is shown — the browser load, which R5 names, and the per-frame solve and
committed table, which no criterion currently measures at all.

**D3 — it is two changes and a renumbering, not one traversal edit.** #822
describes the traversal. Two further pieces are load-bearing and are recorded
here so that the estimate is not taken from the issue alone:

- **A selection concept has to exist first.** No host could say which root it
  showed; both hardcoded `first_root`. "Confine the paint to the shown root" is
  meaningless until something can name a different one. **Done — story #837**,
  which is the whole of this bullet and none of the next:
  [`the-shown-root-is-named-by-ordinal.md`](the-shown-root-is-named-by-ordinal.md).
- **`Arena::dfs_order` is the shared index space.** Its own documentation says
  it is "the one traversal both the rect table and the solvers agree on — change
  it here or nowhere", and rect-table index order is what glyph runs, instance
  rows and clip regions are keyed on. Root-scoping it makes a change of shown
  root a renumbering event, which the dirty-set contract has to treat the way it
  treats `document_replaced` — a dirty set is only meaningful across consecutive
  commits of one index space.

**D4 — until then, a host widens rather than crashes, and reports which it
did.** `Bound::EveryRoot` is correct and stays. It is the widest _safe_ set
rather than the whole table: an asset no root draws is still not read. Reporting
it is part of the rule, because "read everything" and "the shown root happens to
draw everything" produce the same byte count and are not the same fact.

**D5 — R5's status is stated per target and per document shape, not per target
alone.** The claim that holds is: cold-start cost tracks the shown root on
native for any document, and on the web for a document whose unshown roots draw
no asset. `docs/specification/05-qualification.md` already carries a note that
the _mechanism_ differs by target; it gains the second half, that the _document
shape_ does too, with D2 named as what removes the condition.

**D6 — placeholder activation is not the answer to this, and D2 shrinks it.**
`docs/roadmap.md` currently routes the unshown roots' unverified rows to the v1
placeholder item ("drawing a not-ready payload needs the placeholder field that
has no producer"). Under D2 that case stops existing, because a row nothing
paints is a row nothing can ask for. Placeholder activation stays in v1 for what
it was actually deferred for — streaming a payload the **shown** root draws and
that is not resident yet — which is a producer question about where the
placeholder colour comes from, and P1 forbids inventing one at compile time.

## Consequences

- **Epic #793's definition of done is not met on one line, and the close says
  so.** It asks that R5 hold on the web "measured the way epic #594 measured it
  on native". It holds over a document whose unshown roots draw nothing, which
  is not the document #594 measured. Recording the line as met would assert a
  mechanism into existence, which is the failure
  `docs/specification/05-qualification.md` keeps its own section about.
- **Debt #779 becomes fixable rather than permanent**, conditional on D2. It is
  permanent while every root paints, because a row the load skipped is a row the
  painter may still reach.
- **A second cost is named and is not yet measured.** Because the engine solves
  every root and `dfs_order` walks all of them into one table, a document with
  sixty-five artboards costs sixty-five artboards of solve and committed table
  **per frame** while one is shown. R5 and its benchmark bound the load only.
  Whether this needs its own criterion is a v0.19 planning question, not one
  this record settles.

  **Answered, and measured (story #836, 2026-08-12).** v0.19's planning made it
  a criterion, and `goldens/tooling/tests/per_frame_scaling.rs` is it. Over this
  record's own sixty-five-root document, on macos aarch64: a layout frame runs
  65 Taffy layout computations against a one-root document's 1, and the
  committed rect table holds 65 rows against 1 — 65.00x on both, on every frame.
  A paint-only frame solves nothing in either, so the retained tree's fast path
  (issue #164) is not what D2 removes. The counts are equalities in the
  `regression` tier, so D2 cannot land without moving them, and story #838 is
  what moves them to 1 and 1.
- **Issue #825's payload gate waits on this**, as `docs/roadmap.md` records:
  what an embedder links changes if the runtime learns to skip roots.

## Alternatives considered

**Ratify the load-time bound as permanent** — keep the runtime multi-root, keep
`Bound::EveryRoot`, and amend R5 to mean "not proportional to file size" rather
than "proportional to what is shown". Rejected. It costs nothing today, but it
makes the criterion's own benchmark document a shape one target can never
satisfy, and it leaves the per-frame cost proportional to the whole document
with no criterion covering it. A requirement measured over a document one target
cannot deliver is not a requirement.

**Make non-residency representable at paint time** — let a painter draw a
placeholder for a payload that has no bytes, so the browser could bound the
fetch unconditionally while still painting every root. Rejected as _the_ answer,
and kept for what it was deferred for (D6). It is blocked on a producer
supplying the placeholder colour, which is why it sits in v1; and it would leave
the solve and the paint proportional to the document even once unblocked, so it
answers the crash without answering the cost.

**Fetch the missing payloads lazily, when the painter asks** — bound the initial
load by the shown root and satisfy an unshown root's row on demand. Rejected. It
puts a blocking wait in the frame path on the one target that cannot take one:
`just wasm-painter` exists as a gate precisely to keep a blocking wait off the
web path, where it deadlocks. It would also make the byte cost of a load unknown
until the load had already happened, which is the property `Layout` was built to
state in advance.

## What this does not decide

- **Which slice builds it.** `docs/roadmap.md` holds #822 against v0.19 with the
  rest of that slice's inputs; the sequencing is that entry's, not this
  record's.
- **Whether a host may show more than one root at a time**, and what the
  selection surface looks like. D3 says a selection concept is a prerequisite;
  its shape is the story's to settle.
