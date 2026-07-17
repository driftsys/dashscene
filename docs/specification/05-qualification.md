# Qualification

    status  as-built, gardened 2026-07-14
    source  docs/archive/2026-07-14-design-1-seed.md §11

A requirement with no proof is indistinguishable from a requirement with one.
This file is the chain that closes that gap:

    requirement (R1)
      → criterion  (E2)                    this file
        → case     (an RTL corpus scene)   corpus/
          → proof  (a golden test)         goldens/ or a crate test

Criteria whose slice has not landed are listed as **open**, not omitted — a
missing proof must be visible.

## v0 exit criteria

| Criterion                         | Verifies | Status                                    |
| --------------------------------- | -------- | ----------------------------------------- |
| E1 same screen authored both ways | G1       | open — v0.9 (epic #47)                    |
| E2 Arabic golden-stable           | R1       | **met**                                   |
| E3 stress corpus green            | R2       | partial — v0.8 (epic #42, issue #46 open) |
| E4 dirty Figma file → report      | R6       | **met**                                   |
| E5 variant switch via FLIP        | R4       | **met**                                   |
| E6 byte-identical `.dsb`          | R7       | **met**                                   |

The file carries no version in its name. "v0 exit criteria" is a heading
inside it; v1's criteria will be a second heading, not a second file.

### E2 — met

R1 requires the runtime to render Arabic text correctly. The golden
`goldens/tooling/tests/v06_arabic.rs` (`arabic_screen_matches_its_golden`)
proves it end to end: a pure Arabic-plus-numerals screen — no Latin, so
one Arabic font suffices (font fallback is out of v0.6,
`docs/decisions/font-fallback-deferred-past-v06.md`) — is authored in
`dashscene-core`, measured and solved by `dashscene-engine` against the
one `Typesetter`, staged as positioned glyph runs at boundary B, and
rendered through the Skia reference painter as MSDF atlas quads, then
compared against the checked-in `goldens/images/v06-text-arabic.png`.

The screen exercises what E2 names, one node per feature:

- a fixed-width banner whose right-to-left greeting (`السلام عليكم`) sits
  flush-right in a box wider than the text, carrying a lam-alef ligature
  and joining-context forms (#32 bidi/RTL, #33 shaping);
- a hug-sized word (`مَرْحَبًا`) whose harakat are GPOS-stacked above the
  letters, its box sized to the shaped extent by the measure callback
  (#29);
- a hug-sized speed chip whose authored European digits (`120`) render as
  Arabic-Indic shapes because their context is Arabic (#33 mixed
  numerals).

The pixel golden is a coarse full-frame check; the text ink is only
3.95 % of the canvas, too sparse for a pixel budget to resolve a shaping
change. A companion test,
`the_arabic_screen_is_laid_out_and_shaped_as_the_golden_expects`, pins
each E2 feature at glyph-id level, machine-independent and exact, so a
regression fails with a specific message: the banner carries the
seen-joined lam-alef ligature and no isolated lam (a lam-alef splitting
to isolated forms fails); the speed word shapes to contextual, not
isolated, forms; the harakat word's four marks carry nonzero GPOS
y-offsets (marks dropping to the baseline fails); the banner is
flush-right in its box; and the authored European digits shape to the
Arabic-Indic glyphs. A third test,
`every_scene_glyph_is_covered_by_the_committed_atlas`, asserts every
glyph the scene's strings shape to is in the committed atlas, catching a
missed post-GSUB form before it reaches the golden.

Golden-stable across machines: the reference painter is CPU raster at a
pinned skia version, and the atlas is the committed, R7-reproducible
Arabic fixture (`corpus/atlas/arabic`) — no `msdf-atlas-gen` at render
time; the fixture is byte-reproduced on the CI atlas-repro runner
(`committed_arabic_fixture_is_reproducible`). MSDF resolve is
anti-aliased at every glyph edge, so the pixel comparison is
tolerance-based, not bit-exact. Because the inked text is sparse, the
golden uses an absolute 1,000-px differing-pixel budget rather than a
canvas fraction (a fraction wide enough to clear the anti-aliasing
jitter would exceed the whole inked footprint, so a text-erasing
regression would pass): the budget is a few times the scene's
anti-aliased edge count, well below the 2,818-px text-erase and 4,633-px
form-isolation breaks it must catch
(`docs/decisions/golden-comparison-space.md`, "Text goldens").

### E3 — partial

The stress-corpus generator itself (`dashlang`-driven, story/issue #46) has
not landed — epic #42 (v0.8 — fidelity) is open. Two of the six named cases
are already proven independently of the generator, each by a hand-written
case plus an executable test in the crate that owns the construct:

- `negative-gap` (story #10) — `crates/dashscene-engine/tests/solve.rs`.
- `hug-in-fill` (story #11) — `goldens/tooling/tests/v02_flex.rs`.

`wrap`, `grid spans`, `baseline`, and `variant topology change` have no test
yet. See `corpus/dsl-generated/README.md` for the case-by-case status.

### E4 — met

R6 requires a deliberately dirty Figma file to produce a full diagnostic
report and no document. `crates/dashc/tests/figma_lowering.rs`
(`the_reject_fixture_is_refused_rather_than_emitted`) proves it end to end:
the diagnostic fixture `corpus/figma-fixtures/effects-2025.json` — a frame
carrying a noise effect, a texture effect, and a progressive blur, every one
on `docs/specification/04-figma-vocabulary-profile.md`'s REJECT list — is
compiled through `dashc::compile_figma`, which lowers it, runs the import
and load gates, and returns `CompileError::Diagnostics` rather than bytes.
The report names each construct as an error (`profile.noise-or-texture-effect`,
`profile.progressive-blur`), and `compile_figma` emits no `.dsb`: an error
from either gate blocks emission (`crates/dashc/src/lib.rs`, R6). Each
diagnostic points at its own node, pinned separately by
`each_diagnostic_points_at_its_own_node`. Both tests run in the workspace CI
job (`just build`).

The report is backed by the complete named-rule set the validator delivers
at v0.7 (story #41): the import gate's out-of-profile bands (including
variable-width stroke, #145), the load gate's referential-integrity, enum,
`TextStyle.weight` (#129), and corner-radius rules, and the paint gate's
geometry-extent (#128) and budget rules — each independently tested in
`crates/dashscene-validator/tests/`. P4 holds throughout: every out-of-scope
construct is a named diagnostic, never a silent drop
(`docs/decisions/waivers-and-diagnostic-completion.md`).

The validator also delivers the strict-mode release gate as a tested library
contract: `Report::strict` refuses even a warning unless a declared waiver
records the exception for one specific target, and an out-of-scope waiver is
itself diagnosed (`crates/dashscene-validator/tests/waiver.rs`). It is not
yet wired into any compile or import path — no producer calls it and there is
no waiver-file format — so it does not tighten E4 today; that wiring is a
named later importer step (`docs/decisions/waivers-and-diagnostic-completion.md`).
E4's proof does not depend on it: the literal criterion is the dirty file
producing a report and no document, which the emit-gate above already
enforces.

### E5 — met

R4 requires animation to be reproducible in tests. `goldens/tooling/tests/v04_flip.rs`
(`variant_transition_goldens_at_t_0_half_and_1`) proves it end to end: a
`set_variant` switch that moves and grows one node is solved before and after by
the retained `TaffySolver` (issue #164), a `VariantFlip` binds the declared
`VariantTransition` onto `dashcue`'s scheduler (issue #22), and a fixed-step
`advance` then `sample` reads the animated geometry at t = 0, t = 0.5, and t = 1.
Each sample is composed into a full rect set and committed through a fixed-rect
`LayoutSolver` (the `CachedSolver` pattern of `crates/dashlang/src/reactive.rs`),
then rendered through the Skia reference painter and compared against the
checked-in goldens `goldens/images/v04-flip-t000.png`, `v04-flip-t050.png`, and
`v04-flip-t100.png`.

Determinism: the 1-second linear tween lands t = 0.5 on the exact midpoint, and
every authored coordinate and every midpoint is an integer, so the solid fills
are integer-aligned and the three goldens compare exactly — no anti-aliasing
tolerance, the same bit-stable comparison the v0.2 flex goldens use
(`docs/decisions/golden-comparison-space.md`). `dashcue`'s IEEE-754 fixed-step
advance is bit-identical on re-run (`crates/dashscene-engine/tests/flip.rs`
proves a spring FLIP replays bit-for-bit).

### E6 — met

Cross-machine byte-identity is proven by the committed fixture verified in CI.
`goldens/dsb/README.md` states: "Two suites pin it, in two CI jobs that never
meet: `crates/dashc/tests/figma_lowering.rs` (the native library call) and
`importers/figma/src/wasm_test.ts` (the same compile through the wasm ABI, from
Deno). That is what makes story #17's byte-identical to dashc-native output
checkable: each side asserts against the same committed bytes, so identity is
transitive." Each suite runs in a separate CI job on separate machines (GitHub
Actions runners); `crates/dashc/tests/abi.rs:92`'s `the_fixture_compiles_to_the_golden_dsb()`
asserts that freshly compiled output matches the committed `goldens/dsb/v03-paint.dsb`.

Schema-evolution safety is a second layer: a field-id shift or reordered union
would break byte-identity for every previously emitted `.dsb` without failing
the transitive proof above, because both sides build and decode with freshly
generated bindings. `docs/decisions/dsb-frozen-fixture-r7-guard.md` (issue #64,
closed in v0.3) closes that gap with a frozen `.dsb` byte fixture decoded by
today's bindings with value assertions.

The v0.3 proof pins the bytes at the `dashc` boundary. Story #40 (v0.7) extends
it across the importer path that runs in front of `dashc`: trim → export closure
→ token-sidecar derivation → the wasm codec → the written artifacts. The named
per-artifact tests in `importers/figma/src/determinism_test.ts` cover each output
artifact with two layers — a same-process double run that catches per-call
nondeterminism (a clock read, an RNG, or a within-instance hash-map seed that
advances between calls), and an independent anchor that catches
deterministic-but-wrong output:

- the `.dsb` and the `<out>.vars.json` token sidecar run the **whole importer
  path** (`importFigmaFile`) end to end; the `.dsb` is anchored to the committed
  golden and the sidecar to its exact expected binding;
- the many-binding sidecar case is **partial-path** — `variables-bound.json`
  cannot compile whole (a refused Fill-on-hug-axis child), so it derives the
  sidecar exactly as `import.ts` does (trim → closure → derive → format, no
  compile) and is anchored to the exact document-ordered binding sequence;
- the `<name>.receipt.json` receipt is a capture-side artifact, not an
  `importFigmaFile` output, so it is covered at the **unit level** over its two
  producers (`figmaImageRefs`, `formatReceipt`), anchored to the refs coming back
  sorted.

Two separate wasm instantiations would add nothing and are deliberately not used:
the `dashc` module imports nothing from the host, so each instance is a
deterministic clone with identical initial state, and comparing two clones cannot
stand in for two machines. Cross-machine byte-identity is instead the golden's
job, pinned from two CI jobs (`goldens/dsb/README.md`). The determinism holds
because each ordering the path depends on is pinned, not incidental: the paint,
string, and text-style pools intern in first-use DFS order and the image pool is
filled in first-use order rather than by hash-map iteration
(`crates/dashc/src/emit.rs`, `crates/dashc/src/figma/mod.rs`); the closure sorts
its image refs and keeps document order; the sidecar walks nodes in document
order; and the receipt's refs come back sorted from a `BTreeSet`. The native
emitter is locked in isolation by `crates/dashc/tests/figma_lowering.rs`
(`emission_from_the_fixture_is_byte_reproducible`).

E6 was scheduled for v0.7 in the original plan; the fixture guard landed early,
as v0.3 debt (issue #64), and story #40 completed the end-to-end importer proof
on schedule.
