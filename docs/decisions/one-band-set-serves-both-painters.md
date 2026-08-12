# One band set serves both painters

    status   accepted (2026-08-05)
    scope    the render oracle's tolerance bands (goldens/tooling/src/oracle.rs)
             and what layer 4 measures. Settles the two questions story #586
             carried: whether the oracle gains per-painter bands, and which
             corpus layer 4 runs on.

## Context

Story #586 owns two decisions it did not settle when it was written.

**The band question.** "Also decides: whether the render oracle gains
per-painter tolerance bands or a separate band set." Its own expectation was
stated plainly: "Expect the bands to move. A wgpu painter will not pixel-match
Skia: different antialiasing, different gradient dithering, different blur
falloff."

**The corpus question**, raised in a later comment: the story says what layer 4
is and never says which frames it runs on. Two candidates existed — the render
oracle's static Figma-sourced frames, whose bands are already tuned per residual
class, and the showcase scenes, which are animated and are what a person
actually looks at.

## Decision

**D1 — the corpus is split, because the two candidates measure different kinds
of thing.**

The oracle's frames carry a **design source**, so a diff against them is a
_fidelity_ measurement — the painter against Figma's own render of the same
scene. The showcase scenes carry none, so the only thing measurable there is
_parity_ between the two painters. Layer 4's band half therefore runs on the
seven oracle frames, and the frame-cost half runs on the showcase scenes, which
is also where `docs/technotes/frame-budget.md` took its own numbers and what
story #586's first comment asks it to extend rather than restart.

**D2 — the existing three bands serve both painters unchanged. No per-painter
bands, and no second band set.**

Measured, on an Apple M3 through Metal, every one of the seven frames inside its
existing band through **both** painters:

| frame            | band         | skia    | gpu     | budget |
| ---------------- | ------------ | ------- | ------- | ------ |
| v08-wrap         | aa-edge      | 0.000 % | 0.000 % | 2 %    |
| v08-grid-spans   | aa-edge      | 0.037 % | 0.037 % | 2 %    |
| v08-baseline     | msdf-text    | 1.816 % | 1.822 % | 3 %    |
| v05-text-latin   | msdf-text    | 0.033 % | 0.034 % | 3 %    |
| v06-text-arabic  | msdf-text    | 1.405 % | 1.405 % | 3 %    |
| v08-drop-shadow  | blur-falloff | 0.043 % | 0.000 % | 12 %   |
| v08-inner-shadow | blur-falloff | 0.000 % | 0.000 % | 12 %   |

Both `blur-falloff` frames are inside the gate at 0.000 % through both painters.

**The decision does not rest on the budgets being generous.** It rests on the
two painters landing in the same place: by the metric the table publishes, **six
of the seven agree to within 0.006 percentage points**, and the one that differs
— `v08-drop-shadow`, 0.043 % against 0.000 % — differs in the lean painter's
favour. A band set tuned for one painter and applied to the other would be
justified by a divergence that is not there.

## Would these bands catch a defect in the lean painter?

**Asked because "every frame passes" is not evidence on its own, and this
project already knows that.** Issue #422 measured that destroying a shadow
entirely could not reach `blur-falloff`'s 12 % residual, which is why that band
has a gate at all — and every mutation behind those numbers was run through the
**reference** painter. A healthy-versus-healthy table repeats the mistake #422
exposed unless the same question is asked of the second painter.

Three defects injected into `dashscene-gpu` and measured through the same
harness. The gpu column, against the same seven frames:

| defect                           | caught on                                                                                         |
| -------------------------------- | ------------------------------------------------------------------------------------------------- |
| antialiasing removed             | **nothing** — every frame still passes                                                            |
| the drop shadow drawn as nothing | `v08-drop-shadow`, by the **gate** (2.886 % against 1 %), not the residual (4.308 % against 12 %) |
| every quad shifted half a pixel  | `v08-wrap` (3.399 %), `v08-baseline` (4.827 %), and both shadow frames by the gate                |

**Two of the three are caught, and the shadow removal reproduces #422's finding
exactly** — the residual cannot see it and the gate can. The gate transfers to
the second painter, which is the part of this that was least obvious.

**Antialiasing removal is caught by nothing, and the reason is the corpus rather
than the band.** Every geometry value in `lowering-wrap` is an integer —
checked, 64 of 64 — so its rect edges land on pixel boundaries and there is no
partial coverage to lose. The mutation does move the worst channel delta, on
`v08-wrap` from 8 to 35 and on `v08-drop-shadow` from 16 to 96, so it is not
that nothing changed; it is that almost no pixel crossed a threshold. **A band
named `aa-edge` is currently stated over frames that barely exercise
antialiasing.** Recorded on issue #753 with the other corpus gaps.

**D3 — the reference painter is measured in the same run, and that is what makes
the numbers trustworthy.** `goldens/tooling/src/render.rs` records a deliberate
decision _not_ to share the oracle's font and atlas loaders — "the E7 oracle and
its helpers are left byte-identical" — and its own cascade is eight atlases
where the oracle's is three. Any harness outside that test file is therefore
building a scene the oracle did not build. Painting both painters from one scene
removes the dependency: the question is whether they land together, and that is
answerable without either number reproducing the oracle test's.

It also self-checks. `v06-text-arabic` measures **1.405 %** for the reference
painter here, which is exactly the figure the render oracle publishes for Skia
on that frame. The harness is faithful.

## What this measurement does not cover

**The oracle's seven frames exercise solid fills, rect edges, text and the two
shadow kinds. None of them carries a gradient, an image fill, a baked vector
field or a backdrop blur.** So D2 is established for the constructs those bands
govern and for no others — and story #586's own expectation named "different
gradient dithering" specifically, which this corpus cannot see.

That gap is not closable by choosing differently. A design-source frame requires
a hand-authored Figma file and a captured export, which is a human step
(`corpus/figma-fixtures/README.md`) and cannot be fabricated (G-11). Filed
rather than papered over, as issue #753.

What exists for those constructs instead is a **parity** measurement, on the
showcase scenes, from `goldens/tooling/examples/painter-diff.rs`: `surfaces`
holds all nine of the showcase's gradient fills, and every one of its flat
disagreements between the two painters is one or two code points. Parity is a
weaker claim than fidelity and is recorded as such.

## What would reopen this, and what it is not evidence for

**One adapter, one backend, one operating system, one run.** Apple M3 through
Metal. It says nothing about ANGLE, WebGL2 or a mobile GLES driver, and the
project plans a web target and eventually an entry-tier SoC. **Measuring a
second backend is the thing most likely to reopen D2**, and the roadmap's own
risk list names vendor GLES driver differences as the surviving reason Skia's
per-driver workarounds matter.

**The driver and its version are not recorded, and on this backend they cannot
be.** Story #586 asks for them by name. `wgpu-hal`'s Metal adapter builds its
`AdapterInfo` through `wgt::AdapterInfo::new`, which leaves `driver` and
`driver_info` empty, and nothing on that path fills them in — so the example
names the absence rather than printing two blanks. The macOS version is the
nearest available substitute and is not the same thing.

D2 is therefore accepted as: **no divergence between the painters was found in
solid fills, hard rect edges, MSDF text or the two shadow kinds, on Apple M3
through Metal, with two of three injected defects caught.** It is not a claim
about constructs the corpus lacks, and it does not foreclose a per-painter band
that a later measurement earns.

## Consequences

`goldens/tooling/src/oracle.rs` is unchanged — no band moved, none was added.
That is the whole point: the cheapest outcome was available and the measurement
chose it, rather than a threshold being tuned to a painter.

Layer 4 stays what story #586 says it is — a measurement, not a gate. It is an
example rather than a test, run on recorded hardware by a person, because CI is
entirely `ubuntu-latest` with no GPU and a band tuned on lavapipe would drift
with the Mesa version in the runner image while saying nothing about a real
driver.

## Alternatives considered

**Per-painter bands.** The shape the story expected. Rejected on the
measurement: it would encode a divergence the frames do not show, and a second
set of numbers is a second set to keep honest.

**A separate band set for the lean painter.** Same objection, plus it would
split the one place the project states what "close enough to the design source"
means.

**Measuring the lean painter against the reference painter's render rather than
against the design source.** That is parity, not fidelity, and it makes the
reference painter the specification for pixels rather than for behaviour. It is
worth having — `painter-diff.rs` is exactly that, and it is what covers the
constructs the oracle corpus lacks — but it cannot answer a question about
tolerance _bands_, which are stated against a design source.
