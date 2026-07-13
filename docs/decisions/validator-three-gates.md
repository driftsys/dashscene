# The validator is three gates, not one `validate()`

    status   accepted (story #15, 2026-07-13)
    scope    crates/dashscene-validator, crates/dashpaint
    binds    #16 (dashc calls the import + load gates), #41 (v0.7 waivers
             and workaround hints), #45/#44 (v0.8 effects widen the profile
             split)

## Context

P4 — "vocabulary is validated, never discovered; every out-of-profile
construct is a named diagnostic, never a silent drop" — and DESIGN §5's
requirement that the validator run from day one, while the permissive Skia
painter can still draw everything.

Story #15 assumed one validation entry point wired at "both producer
entries (DSL commit, dashc compile)". Reading the code and DESIGN §10.1
against each other showed that a single entry point cannot express the
rules, because the three things the validator must catch live on three
different surfaces:

1. **Out-of-profile constructs never reach the document.** DESIGN §10.1's
   triage table puts the entire v0.3 vocabulary — all four gradient kinds,
   image fills and scale modes, axis-aligned + rounded clip, full
   auto-layout — in the **NOW** band. The constructs that are out of profile
   (layer blur, backdrop blur, advanced blend modes, corner smoothing,
   luminance masks, noise, animated boolean ops) have no representation in
   the `.dsb` schema at all. By the time a construct is in the document, it
   is in the vocabulary. So this class is only catchable on the _producer's
   source_ vocabulary, at import.

2. **Referential integrity only exists on the document.** The bare-`u32`
   index fields issue #63 names (`Node.parent`, `Node.paint_entry`,
   `ImageFill.image`, and the text indices added since) can dangle only in
   the `.dsb`. A committed scene has nothing to dangle: every rect resolves
   to a pool entry (`boundary-b-unification.md`).

3. **Geometry budgets only exist post-solve.** Issue #100's over-wide
   inside stroke needs the node's _resolved_ box. P1 says the document
   carries intent, never results — a `Hug` or `Fill` node has no authored
   size — so this rule is unreachable at the document.

## Options

1. One `validate(&Document, Profile) -> Report`.
2. Two entry points: document and scene.
3. Three gates: import (source vocabulary), load (document), paint (solved
   scene).

## Choice

Option 3.

    triage(construct, profile, node) -> Diagnostic       import gate
    validate_document(&Document)     -> Report           load gate
    validate_scene(rects, paints, images) -> Report      paint gate

## Why

- **Option 1 cannot see two of the three classes.** It has no access to the
  source vocabulary (class 1) and no resolved boxes (class 3). It would
  have shipped a validator that silently fails to catch exactly the thing
  P4 is about.
- **Option 2** still leaves class 1 homeless, which is the headline P4 case.
- Checking geometry on the document's authored `FixedSizeLayout` instead of
  the solved scene was considered and rejected: it is correct only when both
  axes are `Fixed`, and silently wrong under `Hug`/`Fill` — worse than not
  checking, because it reports confidently on a number that means nothing.

### The import gate does not know what Figma is

P5: "Figma compatibility is a property of one producer." The validator owns
the **verdict**; the producer owns the **mapping**. So the gate is keyed by
a source-agnostic `Construct` enum naming DESIGN §10.1's LATER and REJECT
vocabulary, and `dashc` (#16) maps Figma REST JSON onto it. A `figma`
module inside the validator was rejected on P5 grounds: it would make every
future producer's vocabulary the validator's problem.

`Construct` names only out-of-profile vocabulary. The NOW band is simply the
schema, and needs no verdict.

### `validate_document` deliberately takes no `Profile`

There is nothing for it to differentiate: every construct the v0.3 schema
can express is in the NOW band. An ignored `Profile` parameter would imply
a check that does not happen. The parameter returns at v0.8, when effects
enter the schema and give it a rule to select.

At v0.3 the profiles therefore differ in exactly one place — the import
gate, on the two constructs DESIGN §10.1 annotates `(profile:full)`:

| construct                   | `profile:core`                                                          | `profile:full`                       |
| --------------------------- | ----------------------------------------------------------------------- | ------------------------------------ |
| backdrop blur               | Error — a lean painter never gets it, so there is nothing to degrade to | Warning — deferred, declared degrade |
| advanced blend modes        | Error                                                                   | Warning                              |
| every other LATER construct | Warning                                                                 | Warning                              |
| every REJECT construct      | Error                                                                   | Error                                |

### Severity means what DESIGN §5 says it means

`Error` blocks the document; `Warning` is deferred vocabulary with a
declared degrade. A release build runs strict — zero warnings, or an
explicit waiver entry. Waivers and the workaround hint DESIGN §5 also names
in the diagnostic tuple are v0.7 (#41); v0.3 ships
`{rule, severity, node path, message}`.

Rule ids are stable, greppable strings (`paint.gradient.no-stops`), not
numeric codes: a diagnostic a designer sees has to be searchable, and #41's
waiver files will key on them.

### The validator does not depend on `dashscene-core`

Publish order is `dashbuf → dashpaint → dashscene-core → … →
dashscene-validator`, so core cannot call the validator. It does not need
to: `CommittedScene`'s accessors already hand out `dashpaint` types
(`rects() -> &[RectEntry]`, `paints() -> &PaintTable`), so the paint gate
takes boundary-B types and the validator depends only on `dashbuf` +
`dashpaint`. **The producer calls the validator; the arena does not.**

### `MAX_GRADIENT_STOPS` moved to `dashpaint`

The budget was a private constant in `dashscene-skia`, which panics above
it, and issue #100 asked the validator to reject it upstream. Two
hard-coded `8`s that can drift make the validator's guarantee — "a
validated scene never trips the painter's assertion" — quietly false. The
budget is a property of the paint vocabulary, not of one backend (the lean
painter pays for stops in uniform slots), so it now lives on boundary B in
`dashpaint`, and the painter, its test, and the validator all read it.

## Consequences

- **#16 (dashc)** calls the import gate while lowering Figma JSON, and the
  load gate before emitting `.dsb`. It owns the Figma→`Construct` mapping.
- **#41 (v0.7)** adds waivers and workaround hints to `Diagnostic`, keyed on
  the rule ids fixed here, and grows the `Construct` list as the importer
  meets real files.
- **v0.8 effects** (#44, #45) put blur/mask/shadow vocabulary into the
  schema. That is when `validate_document` regains a `Profile` parameter,
  and when the Q-6 group-opacity RT budget gets a number fixed in
  `profile:core`.
- The producer API cannot yet _express_ the v0.3 paint vocabulary at all:
  core's `Prop` enum carries only `Fill(Color)`, so the "DSL commit"
  producer entry cannot construct a gradient, stroke, corner radius, image,
  or clip. Wiring the paint gate into `dashlang` today would check scenes
  that can only hold solid fills. Tracked separately; the natural place to
  close it is the document→arena loader in #16.
