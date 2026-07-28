# Photographic content is target content — what that changes

    status   input, recorded 2026-07-28. Nothing here is implemented and
             nothing is decided. It records a statement about target content
             from the repository owner, and traces what that statement reaches.
             Garden into decisions when each question below is ruled on.

## The input

The repository owner stated that dashscene's target product content includes
**photorealistic 3D scene renders and background photographs**, carried as
image fills.

Both halves matter, and they are not the same content. A rendered interior is
dense material detail throughout. A background photograph — a landscape, a
beach, a mountain — is broad smooth regions of sky or water next to very fine
detail in foliage, sand or rock, often with the smooth part occupying most of
the frame. A block codec behaves differently on the two, and a band that suits
one need not suit the other.

That is new information about the workload, not a new requirement. It is
recorded because every number in the asset pipeline was chosen against content
that is not representative of it, and nobody would find that out by reading the
code — the records all look complete.

## Why it matters

`docs/decisions/asset-quality-profile-bands.md` already says, under "What this
does not pin":

> **Nothing measures a photograph.** The corpus has none. `detail-noise` stands
> in for high-frequency content and is honest about being synthetic.

That was written as a candid limitation. Under this input it becomes a
**material** one: the content class the bands are least evidenced against is the
content class the product ships.

The committed evidence is a gradient with flat rectangles (`import-image-fill`),
a 16x16 near-solid (`v03-paint`), and a synthetic perturbed gradient generated
inside the test (`detail-noise`). A photorealistic render resembles none of
them.

## What the statement reaches

Each of these is a question, not a conclusion. None is answered here.

### 1. The profile bands (#455, and the bands record)

LoFi is a per-texel threshold of 8 with a 5 % area budget; HiFi is 2 and 1 %.
Both were measured, and both have a mutation that fails them — they are gates,
not decoration. What is unevidenced is whether those numbers sit in the right
place for photorealistic content, which has per-texel variation everywhere
rather than in a few regions.

Issue #455 asked for one real high-frequency asset. Under this input it is no longer
a spot check: it is the first measurement of the class the product ships, and
its result should be read as evidence about the band rather than as one more
row in the table.

**Denoising is a trap worth stating.** A denoised path-traced render is much
smoother than the raw sampling result, and denoising removes exactly the
high-frequency content a block codec struggles with. A fixture rendered with
aggressive denoising can measure like `import-image-fill` and pin nothing. The
fixture's residual should be measured before it is committed, not after.

### 2. The memory budget (#462)

Issue #462 records that the packer has no aggregate memory budget, and was deferred to
v1 to be set against target hardware. This input supplies the other half of what
that decision needs: the content class. A document carrying several
photorealistic renders has a materially different resident footprint from one
carrying flat UI graphics at the same pixel dimensions, because the escalation
ladder will stop at finer rungs for detailed content.

Which direction it moves is a measurement, not an assumption — but #462 cannot
be ruled on against "raster content" in the abstract now that the class is
known.

### 3. The ladder's fine end

The bands record notes that no committed asset lands on 4x4 or 5x5, so the
ladder's fine end is walked but never chosen. Detailed content is exactly what
would stop there. Whether that gap closes on its own once a representative
asset exists is measurable as soon as one does.

### 4. The painter's working colour space

Already open, indexed in `docs/technotes/open-questions.md`, and the blocker
`#412` is held behind. Photorealistic renders sharpen it: such content is
commonly authored in a wide gamut and tone-mapped, so where the tone map happens
and what space the painter blends in stop being questions only about blur
falloff. This does not change #412's ruling to hold; it changes what settling
the question is worth.

### 5. Whether ASTC remains the right family

ASTC is a **texture** codec, chosen for GPU-resident sampling. Photorealistic
imagery is what photographic codecs are tuned for, and the two have different
strengths. This is not a proposal to change anything — the GPU-residency
constraint is what picked ASTC and that constraint has not moved — but the
choice was made without this content class stated, so it deserves to be
re-examined rather than assumed to carry over.

## The asset cannot be self-authored, and that is a policy question

The owner has stated they cannot supply a photorealistic render. So the asset
that would settle the questions above has to come from outside the project,
and `docs/decisions/figma-corpus-self-authored-only.md` says:

> **Nothing enters `corpus/` that the project did not author.**

That ruling is scoped to Figma fixture JSON and the design-source renders of
self-authored fixtures. A third-party image committed as a codec test asset is
not literally either — but the reasoning behind the ruling was licensing
ambiguity around committing other people's content as standing fixtures, and
this is squarely inside that reasoning even where the letter does not reach.

**So admitting one is a decision, not a download.** It needs the owner, and it
should be recorded as an amendment to that decision rather than done quietly.

If the answer is yes, the licensing tiers are not equal:

- **CC0** costs nothing and carries no standing obligation. Blender's demo files
  (Classroom, Barbershop) and Benedikt Bitterli's rendering-resources scenes
  (Country Kitchen, The Grey & White Room, Staircase) are the CC0 options, and
  are cluttered material-rich interiors — the content that stresses a block
  codec hardest.
- **CC-BY** is workable but is a standing attribution obligation on a committed
  artifact. `AGENTS.md` already lists `NOTICE(S)` among the shipped docs, so it
  has a home. This tier covers the Blender open-movie frames (Sintel, Tears of
  Steel), most of NVIDIA's ORCA scenes, and SVT Open Content.
- **Restricted** — JVET common-test-conditions sequences are largely limited to
  standardization participants and should not be considered.

Terms should be verified at the source before anything is committed; these
change, and a licence summarised from memory is not a licence check.

### The owner's direction, 2026-07-28

Two assets, not one:

- a **CC0 photorealistic 3D interior** — a Blender demo file (Classroom,
  Barbershop) or a Bitterli rendering-resources scene, rendered to a still.
  This is the target content class.
- a **real landscape photograph** — beach or mountain — as the second content
  class, not merely a second sample. Background photographs are target content
  in their own right, and their statistics differ from a rendered interior as
  described above. Having both is what makes a band that suits one but not the
  other visible rather than assumed.

The corpus-policy amendment is still owed as an explicit record, because
`figma-corpus-self-authored-only.md` is a standing ruling and this direction
departs from it. It should say what class of third-party asset is admitted
(CC0, or CC0 plus CC-BY with a NOTICE entry) and why the reasoning behind the
original ruling does not reach it.

## What would settle it

One representative asset, measured. Everything above is currently reasoning
about content nobody has run through the ladder. A single photorealistic render
in the corpus turns items 1 and 3 into numbers, and gives item 2 something to be
set against.

That is #455's fixture. The change this note makes is to its weight: it is not
a nice-to-have confirmation of a band that is already a gate, it is the first
evidence about the shipping content class — and it is now blocked on the policy
question above rather than on anyone's time.

## Trace

- Bands and their evidence: `docs/decisions/asset-quality-profile-bands.md`,
  `crates/dashpack/tests/band_contract.rs`.
- The asset pipeline design capture:
  `docs/wip/2026-07-19-asset-pipeline-profiles-and-baking.md`.
- Issues: #455 (the asset), #462 (the memory budget), #412 (the sigma constant,
  held behind the colour-space question).
