# Technote — measured verification — the pattern behind the goldens, the oracles and the guards

Informative. This note names a pattern the repository already follows and
records why each part of it exists. It **binds nothing**: every rule described
here is already normative somewhere else, and this note links to that record
rather than restating the ruling. Its purpose is to let a contributor see the
parts as one method instead of as unrelated conventions, and to give the parts
names so a review can ask for one by name.

The pattern applies wherever a test compares an **approximate** result. Where a
comparison is exact — a byte-identical `.dsb`, a hash, a counter — none of this
is needed, and the first rule below is to prefer that case.

## Why the pattern exists

Two failures produced it, both measured on this repository rather than
anticipated.

**A test can compare against the wrong reference.** A golden image diffs the
painter's output against the project's own earlier output. That comparison
cannot see the painter drifting away from what a design actually looks like,
because both sides move together. Guardrail `G-23`
([`engineering-guardrails.md`](engineering-guardrails.md)) states the
consequence: "a renderer that is its own oracle cannot see its own drift". Two
real defects were invisible to the whole golden suite and were found by a
second, independent comparison — a line height taken from the cascade's primary
font rather than the shaping font (story #314), and a text leaf aligned on its
box bottom rather than its glyph baseline because Taffy reports no baseline for
a leaf (issue #272).

**A test can carry a tolerance that no defect can breach.** The `blur-falloff`
band was chosen in advance at a 12 % area budget. Removing the drop shadow from
the scene **outright** measured 4.351 %, removing the inner shadow measured
3.570 %, and both passed. Issue #422 recorded the general form: a budget chosen
in advance and never exercised is not a gate. The remedy was not a tighter
budget but a second term on a different axis, described under "the two-axis
gate" below.

Both failures share a shape. The test ran, the test passed, and the passing
carried no information. Everything below exists to make a pass mean something.

## The six named parts

Nine names appear across the six parts below. Five are already in use in the
repository; four are new labels introduced by this note for rules that already
exist without one.

| name                         | status                                                                                                                                 |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| corpus, golden, band, oracle | in use                                                                                                                                 |
| sensitivity guard            | in use — [`goldens.md`](../design/goldens.md) uses the term directly, and calls the practice the "demonstrated-sensitivity discipline" |
| the oracle triad             | new label here; the three classes are standard terms from the software-testing literature                                              |
| two-bound calibration        | new label here; the practice is story #671's, recorded in [`golden-comparison-space.md`](../decisions/golden-comparison-space.md)      |
| kind-assigned band           | new label here; the rule is stated in [`tolerance-band-coverage.md`](tolerance-band-coverage.md)                                       |
| the two-axis gate            | new label here; the construct is issue #422's ruling                                                                                   |

### 1. The corpus and the expectations are separate artifacts

[`corpus/`](../../corpus/) holds inputs — scenes, documents, payloads, fonts,
atlases — and carries no expected output. [`goldens/`](../../goldens/) holds
expected outputs and the diff tooling. They change for different reasons and at
different rates: adding an input must not invalidate an expectation, and
re-baselining an expectation must not quietly alter an input.

Four rules the corpus carries, each with its own reason:

- **Small and focused, not one large file.** From
  [`corpus/figma-fixtures/README.md`](../../corpus/figma-fixtures/README.md): "a
  failure should implicate one construct, not 'the fixture'".
- **Regenerable, not hand-built.** Every Figma fixture is one menu command of
  the `fixture-author` plugin.
- **Committed, so tests are hermetic.** The atlases exist as committed artifacts
  so a test can load one without `msdf-atlas-gen` at test time; the render
  oracle runs in the ordinary `test` job because fixture, export and compile are
  all local.
- **Provenance recorded.** [`corpus/photo/README.md`](../../corpus/photo/README.md)
  carries a licence audit table, and
  [`figma-corpus-self-authored-only.md`](../decisions/figma-corpus-self-authored-only.md)
  keeps third-party captures out.

The corpus is also where a coverage gap lives: it is one of the four artifacts
a surviving mutant can indict, and the one the worked example under "what a
surviving mutant tells you" turned out to implicate.

### 2. The oracle triad

An oracle is whatever decides that an output is correct. This repository runs
three classes over the same painter, and each covers a blind spot of the others.

| class        | what it compares against                                     | instance here                                                             | blind to                                                                           |
| ------------ | ------------------------------------------------------------ | ------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| regression   | the project's own previously committed output                | the goldens in [`goldens/images/`](../../goldens/images/)                 | drift — both sides move together (`G-23`)                                          |
| differential | an independent authority's output                            | the design-source oracle: Figma's REST export (`G-11`, exit criterion E7) | anything the independent authority also gets wrong, and anything no fixture covers |
| metamorphic  | another run of this system with exactly one variable changed | the profile-preview oracle: RAW against HiFi and LoFi                     | absolute correctness — it proves a relation, not a truth                           |

The metamorphic case needs no external reference at all, which is what makes it
the cheapest of the three to add — and it is the one this project built last,
at story #435. [`goldens.md`](../design/goldens.md) notes
that both arms are "the same painter, the same solver, the same typesetter and
the same canvas, so the only variable is which bytes the asset entries resolve
to". That isolation makes it a purer measurement than any comparison against an
export, which must absorb rasteriser, resampling and gamma disagreement as well.

Two rules govern the differential class:

- **A result is a measured number, never a bare pass or fail** (`G-11`). The
  number carries the trend that predicts the next failure; a boolean does not.
- **A reference is never fabricated.** An uncaptured frame's `designSource`
  stays `null` and is marked `pending-265`. A reference the project generated is
  its own output presented as an independent one, which is the exact failure
  `G-23` names.

### 3. Exactness first — a tolerance is earned, not assumed

A tolerance is a hole in a test. The repository takes the exact comparison
wherever the scene can be made to allow it: the v0.2 flex goldens are
"dimensioned so every solved rect lands on an integer", so all four compare
bit-exactly, and the v0.13 negative-margin frame does the same.

The same instinct now governs measurements that have nothing to do with pixels.
[`startup-scaling-is-measured-by-a-counter.md`](../decisions/startup-scaling-is-measured-by-a-counter.md)
chooses a byte count over a stopwatch because it is "exact, identical on every
machine, and either right or wrong with no tolerance to argue about", and its
D4 makes the assertion an **equality** rather than a ratio under a threshold.

Reach for a tolerance only when the output is genuinely approximate, and then
calibrate it.

### 4. Two-bound calibration

A tolerance has two bounds, and only one of them is usually considered.

- **The floor** — the irreducible noise of the environment. The tolerance must
  sit above it or the test fails without a defect behind it.
- **The ceiling** — the smallest defect the test must still catch. The tolerance
  must sit below it or the test passes with a defect behind it.

A tolerance is **calibrated** when both bounds are measured numbers recorded
next to it. Story #671 did this for all seven text goldens and the result
replaced seven budgets with one constant, `goldens::CROSS_ARCH_BUDGET_PX` = 32:

- The floor came from six CI runners and showed **zero variance** — every golden
  returned an identical count on all six, between 0 and 4 pixels.
- The floor **does not scale with the scene**: the densest scene measured 0 and a
  sparse Arabic one measured 4. What a budget absorbs is machine jitter, not the
  picture, which is why a per-scene budget models something that is not
  per-scene.
- The ceiling came from each golden's own sensitivity guard, between 484 and
  3 193 pixels.

The seven numbers replaced were not seven calibrations; they were seven
multipliers of the same anchor. The full table is in
[`golden-comparison-space.md`](../decisions/golden-comparison-space.md).

If the floor ever meets the ceiling, the answer is a different assertion, not a
wider budget.

### 5. The sensitivity guard

A **sensitivity guard** is a committed defect injection that is re-executed on
every run and asserted to **fail** the gate it guards. It is the ceiling
measurement of the previous section, kept rather than discarded.

It takes three forms here:

- **A twin scene beside a golden.** The shadow goldens render the same scene with
  the shadow removed and assert the difference far exceeds the budget — 1 159
  pixels for the drop shadow and 748 for the inner. `v013-mask-effect-bleed`
  builds the rejected reading of the G-7 mask-bounds ruling and measures it at
  1 280 pixels of 18 432.
- **A row in a manifest.** Each row of
  [`goldens/oracle/profile-manifest.json`](../../goldens/oracle/profile-manifest.json)
  carries the measured defect that breaches its band, and
  `profile_preview_oracle.rs` re-measures it every run.
  `every_band_is_exercised_by_at_least_one_scene` closes the loophole of a row
  claiming no mutation is available; `profile-photo` under LoFi is the one row
  with none, and it states the reason.
- **Structurally, for free.** Two of the three v0.13 frames size their canvas
  from the solved root, so reverting the fix renders a differently sized image
  and fails the dimension check before any budget applies.

Three properties make it a guard rather than an anecdote: it is **committed**,
it is **re-executed** rather than measured once, and it asserts a **failure**.
A measurement taken during review and written into a commit message is none of
these, and decays into a claim.

The discipline is now self-applying. When story #586 asked whether the lean
painter needed its own bands, the answer did not stop at a table of passing
frames:
[`one-band-set-serves-both-painters.md`](../decisions/one-band-set-serves-both-painters.md)
states that "'every frame passes' is not evidence on its own, and this project
already knows that", notes that every mutation behind the existing numbers had
been run through the **reference** painter, and injects three fresh defects into
`dashscene-gpu`.

### 6. Kind-assigned bands, and the two-axis gate

**A residual is classified by its mechanism, not by its magnitude.** A hard rect
edge, a blurred shadow's falloff and an MSDF glyph edge disagree with a
design-source export for different reasons, so one global budget would either
reject a correct blur or accept a broken edge. Three bands are pinned and
asserted distinct by `the_three_rule_bands_are_pinned_and_distinct`, and
[`tolerance-band-coverage.md`](tolerance-band-coverage.md)
states the assignment rule: "a frame is assigned the band whose _kind_ of
residual it carries, not the one its magnitude happens to fit", because
`v08-baseline` was predicted into one band and measured into another.

Two band families exist — the design-source bands and the profile bands — and
`the_two_band_families_do_not_share_a_name_space` keeps them apart, because one
name space would let a frame be graded against a codec band that fails
everything, or a scene against `blur-falloff`, which at a threshold of 24
"passes anything".

**The two-axis gate** is what issue #422 produced. `blur-falloff` now carries
two terms over the same comparison:

| term     | per-pixel threshold | area budget | catches                                               |
| -------- | ------------------- | ----------- | ----------------------------------------------------- |
| residual | 24                  | 12 %        | many pixels slightly wrong — a falloff approximation  |
| gate     | 40                  | 1 %         | few pixels grossly wrong — a removed or broken effect |

Neither is redundant, and a frame passes only inside both. The amplitude
mutation fails the residual at 23.559 % while measuring 0.422 % at the gate's
threshold; the removal mutations do the opposite. The general lesson is that an
area budget alone cannot see a small number of pixels going badly wrong, so an
amplitude term has to be stated separately.

## What a surviving mutant tells you

A mutation that passes the gate is a finding about one of four artifacts, and
naming which one is the whole value of the exercise.

| the survivor indicts | symptom                                    | example                                                                                                                                                                                                                                                                                                                                                                                            |
| -------------------- | ------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| the **corpus**       | the input never exercises the property     | removing antialiasing from `dashscene-gpu` was caught by **nothing**, because every geometry value in `lowering-wrap` is an integer — 64 of 64 checked — so no pixel has partial coverage to lose. The worst channel delta did move — on `v08-wrap` from 8 to 35, on `v08-drop-shadow` from 16 to 96 — so it is not that nothing changed; almost no pixel crossed a threshold. Filed as issue #753 |
| the **band**         | the tolerance is wider than the defect     | issue #422: a removed shadow at 4.351 % against a 12 % budget                                                                                                                                                                                                                                                                                                                                      |
| the **metric**       | the comparison cannot express the defect   | an area budget cannot see a few grossly wrong pixels, which is why the gate exists                                                                                                                                                                                                                                                                                                                 |
| the **assertion**    | it reads the near side of a transformation | asserting authored intent rather than committed output — the producer's `text()` rather than the painter's `committed().glyphs()`                                                                                                                                                                                                                                                                  |

The antialiasing result is the instructive one, because the honest conclusion
was not "the band is fine". It was that "a band named `aa-edge` is currently
stated over frames that barely exercise antialiasing" — a coverage gap that no
passing run could have revealed, and that no change to the band would fix.

One practical caution: a mutation that fails to apply, fails to compile into the
path under test, or is reverted by the harness looks exactly like a mutation the
gate survived. Confirm the injected defect reached the code being measured
before concluding anything from a pass.

## The pattern off the pixel path

None of the six parts is specific to rendering. Inside this repository the same
discipline already governs measurements that produce no image:

- **Startup cost** — a byte counter rather than a stopwatch, with an equality
  assertion rather than a threshold, and the benchmark landed together with a
  demonstration of it failing
  ([`startup-scaling-is-measured-by-a-counter.md`](../decisions/startup-scaling-is-measured-by-a-counter.md),
  story #598).
- **Invisible costs** — `Residency::decodes` and `Renderer::allocations` are
  counters for costs with no visible symptom, and the same decision states the
  rule: a cost with no visible symptom needs a counter, not a stopwatch.
- **Honest guardrail status** — `G-19` in
  [`engineering-guardrails.md`](engineering-guardrails.md) records its own
  history rather than only its current state. It was marked as failing while it
  failed, with the cost measured beside it — 1 935 927 bytes hashed to show a
  one-frame root out of a 65-frame document, against the root's own 197 387 —
  and story #597 then moved the verification off the open path. It now reads
  "met, and measured", and still carries the numbers from when it did not. A
  guardrail that flipped silently to satisfied would leave no trace that it had
  ever been false.
- **Test tiers** — [`test-tiers.md`](../decisions/test-tiers.md) requires the tier
  actually run to be named in the PR body, and records that a green aggregate
  `ci` job no longer means the suite ran.

## Prior art

These are established techniques with established names, and none of them
originates here. The names are recorded so a reader can follow the literature
rather than take this note's word for anything.

| part                | established name                                                            | source                                                                                                                                                                                                                                         |
| ------------------- | --------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| golden              | golden master, snapshot testing, approval testing, characterization testing | Feathers, _Working Effectively with Legacy Code_, 2004; the `-update` testdata convention in the Go ecosystem                                                                                                                                  |
| corpus              | test corpus, seed corpus, fixtures                                          | fuzzing practice (AFL, libFuzzer); compiler test suites                                                                                                                                                                                        |
| oracle              | test oracle; the oracle problem                                             | Howden, late 1970s; Barr, Harman, McMinn, Shahbaz & Yoo, "The Oracle Problem in Software Testing: A Survey", IEEE TSE, 2015                                                                                                                    |
| differential oracle | differential testing                                                        | McKeeman, "Differential Testing for Software", 1998                                                                                                                                                                                            |
| metamorphic oracle  | metamorphic testing                                                         | Chen, Cheung & Yiu, 1998                                                                                                                                                                                                                       |
| sensitivity guard   | mutation testing; fault injection                                           | Hamlet 1977; DeMillo, Lipton & Sayward, "Hints on Test Data Selection", 1978; tools such as PIT, Stryker and `cargo-mutants`. Fault injection is also a recommended verification method in ISO 26262 for software unit and integration testing |
| tolerance bands     | perceptual image metrics                                                    | SSIM (Wang et al., 2004), FLIP (Andersson et al., 2020) and SSIMULACRA2 — the latter two already used by `perceptual_calibration.rs`                                                                                                           |

What differs from the textbook treatment is narrow and worth stating plainly.
Mutation testing normally runs a broad automated sweep and reports a score, with
the mutants discarded afterwards; here a small number of deliberately chosen
mutants are **committed and re-executed as standing assertions**, and no score
is computed. Tolerance thresholds are normally chosen by judgement; here they
are required to sit between two measured bounds, with both numbers recorded.

## What this note does not do

It settles nothing. The decision records that bind are:

- [`golden-comparison-space.md`](../decisions/golden-comparison-space.md) — the
  comparison space, the three comparison functions, the text-budget calibration.
- [`render-oracle-tolerance-and-gating.md`](../decisions/render-oracle-tolerance-and-gating.md)
  — the design-source bands and the real-export-only rule.
- [`asset-quality-profile-bands.md`](../decisions/asset-quality-profile-bands.md)
  — the profile bands and the mutation each ships with.
- [`one-band-set-serves-both-painters.md`](../decisions/one-band-set-serves-both-painters.md)
  — one band set across both painters, and the three defects injected to
  establish it.
- [`startup-scaling-is-measured-by-a-counter.md`](../decisions/startup-scaling-is-measured-by-a-counter.md)
  — a counter rather than a stopwatch, and equality rather than a threshold.
  Two further records carry most of the detail, and neither of them binds either.
  [`engineering-guardrails.md`](engineering-guardrails.md) states `G-11` and
  `G-23`, which the whole oracle triad exists to satisfy — but it is a technote,
  and its own opening says it introduces no new binding rule; what binds is the
  principle or requirement each guardrail makes falsifiable.
  [`goldens.md`](../design/goldens.md) is the as-built design record, and holds
  every frame and every band's current measured number.
