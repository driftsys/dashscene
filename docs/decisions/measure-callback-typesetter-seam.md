# The measure callback borrows one Typesetter; text drives hug sizing through it

    status   accepted (story #29, 2026-07-15)
    scope    dashscene-engine TaffySolver; binds #30 (the hug-sizing
             text golden) and #164 (the v0.4 retained Taffy tree)

## Context

Story #29 wires text into the Taffy solve: a hug-sized text node must lay out to
its shaped width and height, and — the essential constraint — layout and paint
must read the same shaped-run cache so they cannot disagree about a glyph's
size. The shaped-run cache lives on `dashscene-typeset`'s `Typesetter`
(`docs/decisions/shaped-run-cache-font-units.md`); the solve runs inside
`dashscene-core`'s `commit_with` (`docs/decisions/layout-solver-seam.md`). The
measure callback is a seam two later stories rebase onto — #30's painter reads
the same cache, and #164's retained Taffy tree must invalidate a cached
measurement when text changes — so the callback signature is a design decision,
not just wiring.

## Options

How the `Typesetter` reaches the measure callback:

1. The caller owns one `Typesetter` and lends it to the solver by mutable borrow
   (`TaffySolver::with_typesetter(&mut Typesetter)`); `TaffySolver::new()` keeps
   the text-free path with no typesetter.
2. `TaffySolver` owns a `Typesetter` it constructs or is handed by value.
3. The `Typesetter` is shared through `Rc<RefCell<…>>` (or `Arc<Mutex<…>>`) so
   the solver and the painter each hold a handle.

How a node is recognized as text to measure:

- A node becomes a measured Taffy leaf (`new_leaf_with_context`) only when it
  carries both text content and a text style; a node missing either is a plain
  leaf whose measure is a no-op.

## Choice

Option 1. `TaffySolver` holds `Option<&mut Typesetter>`: `with_typesetter` sets
it, `new()` leaves it `None`. During the solve the measure callback reads the
borrowed typesetter; a text leaf carries a `TextContext { text, size }`, and
`measure_text` lays it out through `Typesetter::layout`, returning the shaped
box. The wrap width is the width Taffy already fixed if any, else a definite
available width, else none (a min/max-content probe imposes no wrap), so an
unconstrained hug node hugs its natural one-line width and a width-constrained
node grows taller as the text wraps. A known axis is returned unchanged.

## Why

- The borrow is the single-source discipline the story demands. The caller keeps
  one `Typesetter` for the whole runtime; it lends it here for the solve and
  lends the same instance to the painter at paint time (#30). One cache, so
  layout and paint cannot disagree about a glyph's size (P2 — one typesetter).
- Option 2 (owned) would trap the cache inside the solver, which is built fresh
  per commit (`commit_with(&mut TaffySolver::…)`); the painter could not reach
  it, and each commit would start from a cold cache. It contradicts the
  single-source constraint.
- Option 3 (shared interior mutability) reaches one cache but adds a runtime
  borrow discipline and a refcount for a sharing pattern the borrow already
  expresses: the solve and the paint are sequential phases of one owner, not
  concurrent holders. `Rc<RefCell<…>>` is reserved for a real concurrency need,
  not this.
- Keeping `new()` typesetter-free keeps every non-text solve — and the
  fixed-commit equivalence tests — unchanged: a text-free scene needs no font,
  so it constructs none. A text node solved with `new()` is simply not measured
  (it has no font to shape with) and sizes as an empty leaf; text scenes call
  `with_typesetter`.
- Requiring both text and style before attaching a measure context keeps the
  seam honest: a text node with no style has no size to shape at, and the engine
  is not the diagnostic surface for that (the validator is, P4).

## Consequences

- The public signature #164 rebases onto is fixed:
  `TaffySolver::with_typesetter(&mut Typesetter)` and the `&mut self` solve.
  When #164 retains the Taffy tree across commits, it keeps the
  borrowed-typesetter contract and adds cache invalidation keyed on a node's
  text/style change; nothing about the borrow needs to move.
- The `TextContext` owns its text so the tree can outlive the arena borrow.
  Shaping is not repeated per solve (the cache sits in front of it), but the
  tree itself is still rebuilt every solve — the retained tree is #164's work,
  not this story's.
- A definite-available-width measurement (a `Fill` or fixed-width text node)
  wraps correctly today, though v0.5's own scenes are hug text; the branch is
  exercised by the width-constrained wrap test and stands ready for #30/#43.
- The seam widens by adding measure inputs to `TextContext`, never by changing
  how the typesetter is reached. Later stories added the shaping axes and then
  the node's CSS weight (story #368), each populated from the same
  `Arena::text_style` read and each carried into the typesetter call — the
  borrow, the recognition rule, and the known-axis rule are untouched. The
  as-built field list is in `docs/design/dashscene-engine.md` (Measure
  callback); why weight is a measure input rather than a paint-only one is in
  `docs/decisions/weight-selection-in-the-cascade.md`.
- **The borrow gained an alternative at story #863, and the reason it had to is
  worth stating: the seam is fine, the lifetime is not.** Lending assumes a
  caller that outlives the solve. A host loading a `.dsb` is not one —
  `dashlang::attach_live` takes a `Box<dyn LayoutSolver>` and keeps it for the
  life of the scene, so the solver in that box is `'static` and nothing local
  can be lent into it. Every `.dsb` load path therefore built
  `TaffySolver::new()`, and a loaded document containing text drew no glyphs and
  measured its text nodes as empty leaves.

  `TaffySolver::owning(TextResources)` holds the typesetter instead. **Lending
  stays the default and this record's Choice is unchanged** — the showcase, the
  goldens and all but one test still lend, because they can. The exception is
  the test that holds on purpose,
  `an_owning_solver_measures_through_its_\
typesetter_and_retains_its_tree`,
  which exists to cover the held arm. What the held variant is _not_ is the
  shape the showcase carried until issue #950: a wrapper owning a typesetter and
  building a fresh `TaffySolver` inside every call, which starts each solve with
  no retained tree and so rebuilds it per frame. It was recorded there as a cost
  rather than hidden, and it was tolerable only in a demonstration — on the
  product path it pays back issue #164's whole saving every frame. **The
  showcase now holds too**, one `owning` solver per scene, so no such wrapper is
  left in the tree. What that move cost is stated in
  `one-solver-per-live-scene.md`: a retained tree is correct only while one
  solver sees every commit into the arena, in order.

  The reason for lending — one typesetter for the runtime, so layout and paint
  cannot disagree about a glyph's size — is satisfied by the held variant rather
  than waived. The held solver _is_ the runtime's only text producer: it
  measures and it stages, both at commit, and a painter reads the runs out of
  the committed table rather than out of a cache it holds separately.
