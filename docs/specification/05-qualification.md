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
| E4 dirty Figma file → report      | R6       | open — v0.7 (epic #36)                    |
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

E6 was scheduled for v0.7 in the original plan; the fixture guard landed early,
as v0.3 debt (issue #64).
