# Technote — engineering guardrails

    status   review checklist, 2026-07-17. Informative: it makes existing
             principles and requirements falsifiable at review time; it
             introduces no new binding rule of its own.
    scope    run at design reviews and at each v0.x slice sign-off. Every
             item anchors to a principle (P1–P5), a requirement (R1–R7), a
             target-hardware rule (R-T1–R-T5), or an open question (Q-1–Q-6).

These are guardrails the project imposes on itself because its own
requirements demand proof, not promises. Each item turns a stated principle
into something a reviewer can pass or fail. Run the list at every design
review and every slice sign-off: an unchecked item is scheduled work or a
recorded waiver, never silence.

The identifiers `G-1`–`G-23` name guardrails and are distinct from the goals
`G1`–`G3` in
[`01-goals-and-requirements.md`](../specification/01-goals-and-requirements.md).
Where a guardrail body cites `G2` or `G3` it means the goal. The hyphen
carries the distinction, as it does for `R` requirements versus `R-T`
target-hardware rules.

## Fidelity

- **G-1** — Every approximation constant in a painter or a lowering cites the
  golden test that pins its value. A constant without a fixture is a bug. (R6,
  G3)
- **G-2** — Font resolution has exactly two declared modes: (a) a bundled font
  file — a build input, content-hashed; (b) a platform-provided font, resolved
  from the target image at build time, content-hash pinned in the document,
  and baked through the same atlas pipeline — hash drift on the target is a
  named diagnostic. An unresolvable font is a compile error or a waiver. The
  platform text stack is never used at runtime. (R1, P2; the current single-
  font-per-charset rule is
  [font-fallback-deferred-past-v06.md](../decisions/font-fallback-deferred-past-v06.md))
- **G-3** — The Figma line-height and vertical-metrics mapping is specified in
  the typeset spec and corpus-tested with a mixed-font, mixed-size row. (R1)
- **G-4** — Corner smoothing (squircle) is detected and diagnosed by the
  importer from its first slice, even while it is triaged LATER. (R6)
- **G-5** — Full 2D transforms, including shear and skew, are representable in
  the schema and the rect table, and every painter honors the full matrix.
  (boundary B, P2)
- **G-6** — Rotated-child-inside-auto-layout semantics are defined — which box
  feeds the solver — and corpus-tested. (R2)
- **G-7** — The paint-and-text edge-case triage covers stroke-on-text,
  per-side strokes, dashed strokes, single-stop gradients, and mask scoping
  ([04-figma-vocabulary-profile.md](../specification/04-figma-vocabulary-profile.md),
  "Paint and text edge cases"). (R6)
- **G-8** — Letter-case transforms happen in the typesetter, pre-shaping. (P2)
- **G-9** — Image scale-mode edge semantics (fill / fit / tile / stretch;
  clamp versus decal) are specified in `dashpaint` and identical across
  painters — never inherited from a backend default. (G2, P2)
- **G-10** — The gradient interpolation color space is pinned and
  single-sourced into all painters; the angular-gradient angle convention is
  pinned by a golden. (R-T5)
- **G-11** — CI includes a design-source render oracle: a perceptual diff of
  the Skia reference painter's output against Figma's REST image export for
  every corpus frame, with per-rule tolerances. Fidelity is a measured number,
  not an asserted one. This guardrail is tracked as exit criterion E7, whose
  status the qualification file states
  ([05-qualification.md](../specification/05-qualification.md)). (R6, G3)

## Frame loop

- **G-12** — A boundary-B audit at every slice confirms that nothing at frame
  rate walks the document tree, resolves styles, or merges variants. Frame cost
  is dirty-range upload plus submit, nothing else. (R-T4, P3)
- **G-13** — Painters allocate nothing in steady state, asserted by an
  allocation counter in the bench harness. (R3)
- **G-14** — Variant and style merge at commit is incremental, over dirty
  subtrees; commit-time cost is profiled from v0.4 onward. (P3)
- **G-15** — Transitions never duplicate whole-tree resolution: FLIP
  interpolates measured deltas, and there is no second resolved tree during a
  transition. (R4)
- **G-16** — The saveLayer / render-target count is a validator-enforced budget
  with a measured value. The budget is a validator placeholder until Q-6 is
  measured on target hardware
  ([masks-and-group-opacity.md](../decisions/masks-and-group-opacity.md)).
  (Q-6, R-T1)

## Load

- **G-17** — The first frame of a declared root touches zero cold-section pages
  and decodes zero images on the CPU, asserted by a startup benchmark. (R5,
  R-T3)
- **G-18** — Every lookup structure needed at load — the node-id map, the
  variant map, the component index — is a compile-time artifact inside the
  `.dsb`, mmap-resident; nothing is rebuilt by a load-time walk over the tree.
  The `dashbuf` schema reserves these tables. (R5)
- **G-19** — Load-gate verification hashes hot sections only; its cost is
  measured and off the render path. (boundary A, R5)
  **Met, and measured.** It failed when this entry was written, and in **two**
  places rather than the one this entry first named, one per path.
  `dashbuf::open` resolved every asset entry through
  `Container::blob_by_hash`, which hash-verifies the whole payload — that was
  the **owning** path, the one a host takes when it holds bytes it cannot
  borrow from. `prefix::Plan::bind` hashed every fetched payload against the
  section table — that was the **mapped** path, after story #596 moved the
  native host onto the prefix reader, and so the one that mattered to this
  slice. So a load
  hashed every byte of every asset — cold sections included, 1 935 927 B to show
  a one-frame root out of a 65-frame document rather than the root's own
  197 387 B.

  Naming only `open` is the error issue #782 was filed against, and it is the
  same one PR #764 had already corrected in
  `docs/decisions/verification-moves-from-open-to-touch.md`: a claim can be true
  of the function it names and still be the wrong function, because a sibling
  story had moved the path. Fixing the guardrail's pass/fail state without
  fixing its attribution would have reproduced in a durable record the defect
  that record was corrected to remove.

  Story #597 moved both. `dashbuf::open` calls `Container::verify_hot`, hashes
  the hot region alone, and resolves each entry to where its payload lies
  without reading it; `dashbuf::residency::BlobResidency::touch` hashes a payload
  when a prefetch makes it resident, and the prefetch is the shown root's
  assets and nothing else. Story #598's re-run then moved the criterion onto
  the mapped path, which is what measures it:
  `goldens/tooling/tests/startup_scaling.rs`, macos aarch64, **197 387 B out of
  both documents — the shown root's own payload, and no copy at all.**

  The one thing this entry does not claim is a cold-cache number. The criterion
  counts bytes and asserts on no wall clock (D1), and the benchmark writes the
  documents it reads, so every fault it takes is a minor one. That is the same
  fact that deferred `madvise` to issue #767, and a cold-cache measurement is a
  hardware and harness question rather than a loading-path one.
- **G-20** — A scaling benchmark with a small-root document and a many-frame
  corpus document asserts that cold-start cost tracks the shown root, not the
  document size. This guardrail is tracked as the v1 startup-scaling exit
  criterion ([05-qualification.md](../specification/05-qualification.md)). (R5)

  **Met.** `goldens/tooling/tests/startup_scaling.rs` is that benchmark, and it
  is an ordinary `regression` test since story #598's re-run, so a regression in
  R5 fails a build. It was demonstrated failing first, at 9.81x against the
  pre-slice load path, which is what says the 1.00x it reports now was earned
  rather than assumed.

## Format and process

- **G-21** — The document version and the section hashes are a hard load gate.
  There is no warn-and-proceed compatibility mode, ever. (boundary A, R7)
- **G-22** — A vocabulary-coverage test enumerates the node, paint, and text
  properties from the pinned Figma REST spec and asserts each is supported,
  lowered, or a named diagnostic. The test fails if any property lands in no
  bucket. (R6)
- **G-23** — Golden tests diff against the reference painter and the
  design-source oracle (G-11), never only against the project's own previous
  output. A renderer that is its own oracle cannot see its own drift. (G3)
