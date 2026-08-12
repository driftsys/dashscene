# Decision: blur blends in sRGB-encoded space, measured against Figma

    status   accepted (2026-07-30). Settles the "Blur colour space" open
             question raised by
             docs/archive/2026-07-19-color-space-blur-and-msdf.md
             and indexed at docs/technotes/open-questions.md. Decided by
             measurement against Figma's own render, not by preference.
    scope    crates/dashscene-skia (the surface allocation and both blur
             paths), crates/dashpaint (boundary B — the value every painter
             must match), goldens (the two frames that measure it)
    binds    every painter, present and future. Two painters that blur in
             different spaces produce visibly different pixels from the same
             document, so this is a property of the boundary-B contract and
             not an implementation detail a painter may choose
    related  docs/decisions/backdrop-blur-is-core-vocabulary.md,
             docs/decisions/q1-msdf-below-14px.md,
             docs/decisions/golden-comparison-space.md,
             docs/archive/2026-07-19-color-space-blur-and-msdf.md
    refs     #412, #474

## The decision

**A blur averages raw sRGB-encoded channel values, not linear light.** The
reference painter's surface therefore keeps its current allocation —
`surfaces::raster_n32_premul` with **no colour space attached**
(`crates/dashscene-skia/src/lib.rs`) — and that allocation is now a decided
property rather than an accident of the call that made it.

This was previously the implicit behaviour. It is now the measured one: Figma
blends `BACKGROUND_BLUR` in sRGB-encoded space too, and this project matches it.

**It applies to every blur, but only backdrop blur can observe it.** Drop and
inner shadow blur one flat shadow colour over the node's silhouette, and
averaging a colour with itself gives the same answer in either space, so the
rule is unobservable there and no shadow frame constrains it. Backdrop blur
averages multi-coloured content and is where the choice becomes visible. The
rule is still stated over all blurs rather than over backdrop blur alone,
because layer blur lands at v1 (`dashpaint::BlurKind::Layer`) and will average
multi-coloured content too — this must not have to be re-decided then.

The MSDF coupling that made the question hard is dissolved rather than managed.
The no-colour-space surface is what makes MSDF distance channels sample raw
(`docs/decisions/q1-msdf-below-14px.md`), and attaching a linear working space
to fix the blur would have corrupted them. There is nothing to fix: the same
allocation is correct for both, for two independent reasons.

## What was measured

The `backdrop-blur` corpus fixture rendered through the production path
(`goldens::render::render_dsb`) and diffed against Figma's own `GET /images`
export (`goldens/oracle/import-design-source/backdrop-blur.png`). The metric is
the max-over-RGBA absolute per-pixel delta, the same one the oracle bands grade
with. The measured region is the frosted panel itself — 200x90 at (60,45), 18000
px — because that is where the blur is, and the frame's headline number is
dominated by an unrelated anti-aliased ellipse rim.

The panel straddles two hard-edged colour seams: amber `rgb(250,199,51)` meets
navy `rgb(13,18,31)` at x=107, and navy meets pale `rgb(235,240,250)` at x=213.
The amber/navy seam is the strongest available signal, because a 50/50 average
of those two colours differs by roughly 50 code points between the two spaces.

Linear-light blending was produced by wrapping the backdrop blur in Skia's
`srgb_to_linear_gamma` / `linear_to_srgb_gamma` colour filters — a temporary
mutation, reverted, never committed. Isolating the blur is deliberate: attaching
a linear colour space to the whole surface would also change how every
anti-aliased edge and the panel's own 0.2-alpha fill composite, which would
confound the measurement with effects that are not the question.

### At the shipped sigma

| blend space              | panel mean | panel RMS | panel max | px over delta 40 |
| ------------------------ | ---------- | --------- | --------- | ---------------- |
| **sRGB-encoded (ships)** | **2.704**  | **3.652** | **16**    | **0**            |
| linear light             | 14.969     | 23.829    | 60        | 3117             |

### The sigma sweep, which closes the obvious objection

The objection to the table above is that sigma is itself unsettled (#412), so a
worse fit might only mean linear light wants a different blur width. Sweeping
the sigma mapping in each space separately answers it. Panel mean delta:

| sigma / radius | sRGB-encoded | linear light |
| -------------- | ------------ | ------------ |
| 0.20           | 8.777        | 10.682       |
| 0.25           | 6.319        | **10.363**   |
| 0.30           | 5.497        | 10.562       |
| 0.35           | 3.087        | 11.922       |
| 0.40           | 1.622        | 12.973       |
| **0.4375**     | **1.187**    | 13.431       |
| 0.45           | 1.930        | 14.463       |
| 0.50 (ships)   | 2.704        | 14.969       |
| 0.60           | 6.635        | 17.694       |

Each space's own best fit is in bold. Three things follow, and none of them
depends on which sigma is eventually chosen:

- **sRGB-encoded blending fits Figma better at every sigma from 0.20 to 0.60
  than linear-light blending does at its own best sigma.** The comparison is not
  close and it is not sigma-dependent.
- At its optimum, linear light is **8.7 times worse on mean** and 10.8 times
  worse on RMS than sRGB-encoded at its optimum (10.363 against 1.187).
- At its optimum sRGB-encoded leaves a max delta of 14 over the panel and
  **zero** pixels above even the tighter delta-24 threshold — the same level of
  agreement the frame already records for its unblurred regions. Linear light
  reaches zero at no swept sigma; at its best it still leaves 3438 pixels over
  delta 24 and 464 over delta 40.

The two spaces also disagree about the blur's _width_, not only its values:
linear light's best fit wants sigma ≈ 0.25·radius, about half of what
sRGB-encoded chooses. Fitting a narrower kernel is how the wrong space
compensates — it reduces how much of the wrong average is visible — and it still
cannot reach the right answer.

### Read directly, without a summary statistic

One scanline at y=120, crossing the amber/navy seam inside the panel, at the
seam column x=107:

| render             | RGB at x=107    |
| ------------------ | --------------- |
| ours, sRGB-encoded | `152, 134,  83` |
| **Figma's export** | `149, 133,  83` |
| ours, linear light | `195, 166,  81` |

Figma sits 3 code points from the sRGB-encoded blend and 46 from the
linear-light one — and on the _far_ side of ours from linear light, so no change
of blur width moves linear light toward it.

## What already gates this

No new check was added, because four named committed tests already fail on the
linear-light mutation. Measured by applying it and running the workspace with
`--no-fail-fast`:

- `the_import_renders_match_their_design_source`
  (`goldens/tooling/tests/import_oracle.rs`) — `backdrop-blur` measures **5.429
  %** and `vector-backdrop-blur` **4.866 %**, both against the aa-edge band's 2
  % budget. This is the one that compares against Figma, so it is the one that
  makes the finding a gate rather than a note.
- `the_frosted_panel_scene_matches_its_golden`,
  `the_backdrop_blur_spreads_at_the_mapped_sigma` and
  `the_backdrop_blur_reads_past_the_node_box`
  (`goldens/tooling/tests/v011_backdrop_blur.rs`) — this project's own
  invariants, which would fail for any change to the blur.

No other test in the workspace failed. Separately, and with the mutation
reverted, this change moves no committed artifact: exactly 9 of the repository's
750 tracked files differ from `origin/main`, all of them `.md`/`.rs`/`.json`,
and all 106 binary artifacts hash identically under `git hash-object`. Asserted
per file rather than inferred from a green suite.

## Why the question survived two slices

The premise that made it look unanswerable was **true when it was written and
went stale six days later**, and nothing re-checked it.

`docs/archive/2026-07-19-color-space-blur-and-msdf.md` recorded, correctly on
2026-07-19, that "our only blurs today are drop and inner shadow" and that a
backdrop-blur oracle frame over multi-coloured content "does not exist yet".
Story #393 landed backdrop blur on 2026-07-26 with exactly such a frame — three
hard-edged bands at high chroma and luminance contrast, with the frosted panel
placed to straddle both seams — and `vector-backdrop-blur` added a second.

The stale sentence was then copied forward into
`docs/technotes/open-questions.md` and into the driver prompt for this work,
each time as a statement of fact about the present. The fixture the question was
said to need had been committed for four days when the question was last
restated.

**The frames were never blind to this.** A linear-light blend fails both of them
by more than double their budget. Had anyone rendered the mutation, the question
would have closed the day #393 landed.

## Consequence for #412

**#412 is unblocked, and its measurement stands.**

Issue #412 was held on the reasoning that its sigma fit was "taken through this
confound" — that a best-fitting sigma of 0.42–0.45·radius might be partly
compensating for blending in the wrong space. It was not. The space is right, so
the fit measures what it claims to measure.

The sweep above independently reproduces #412's numbers from a separate harness:
sigma 0.4375·radius (7 on the authored radius 16) gives panel mean 1.187 and RMS
1.545, against 2.704 and 3.652 for the shipped 0.5 — the same four figures that
issue #412 recorded. Two of its three stated reasons to hold are discharged: the
colour-space confound does not exist, and a frame that can see the difference
does. The third stands untouched and is the whole of what remains — the constant
is shared with shadows, where `blur-falloff` was tuned against the shadow
fixtures, so retuning it is a cross-effect change that must measure the shadow
frames too.

The sigma constant is **not** changed here. That is #412's own scope.

## Alternatives considered

- **Author a new fixture over a red/green or blue/yellow edge**, as the driver
  prompt for this work directed, and measure that. Rejected once the existing
  frames were checked: they already separate the two hypotheses by a factor of
  8.7, and both already fail on the mutation by more than double their budget. A
  new fixture would need a Figma Desktop session from the repository owner to
  produce evidence that is already committed. A more separated pair would have
  raised the signal somewhat — amber against navy spans 237, 181 and 20 code
  points in red, green and blue, where a blue/yellow edge would span 255 in all
  three — but the existing separation is already enough that linear light fails
  by 2.4 to 2.7 times the tolerance budget and loses by 8.7 times on mean.
  Nothing in the conclusion turns on that headroom.
- **Attach a linear working colour space and tag the MSDF atlas as
  colour-space-independent**, the path the archived capture sketched. Rejected:
  the measurement says the current space is the one that matches Figma, so this
  would trade a correct render for an incorrect one and take on the MSDF
  re-tagging risk to do it.
- **Record the result as a technote rather than a decision.** Rejected: this
  binds every future painter's output, which is the definition this project uses
  for a decision record. A painter that blurs in linear light is non-conforming,
  and that has to be findable from `docs/decisions/`.
