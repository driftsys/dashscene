# Motion is document data, keyed on the destination member; a smoothing spec is not

    status   accepted (story #771, 2026-08-09;
             the #773 producer landed 2026-08-11 — see the second amendment)
    scope    crates/dashbuf schema; crates/dashscene-core's mirrored
             vocabulary and its loader; crates/dashlang's frame loop;
             issues #771, #617, #255, #626, #852
    also     supersedes issue #625's sketch of the fix (see "The driver")

## Context

`dashbuf` did not depend on `dashcue`, and nothing in the schema carried a spec,
an easing, a duration or a keyframe. A `.dsb` held the two **ends** of an
animation — `variant_sets` are the states, `SignalDecl`/`Binding` the wiring —
and not the motion between them, so every dashscene animation had to be written
in Rust against `dashlang`. A Figma Smart Animate transition, the construct
`VariantTransition` mirrors, had no row to be stored in.

Issue #255 records the same absence on the **binding** side: a `smooth()` spec
does not serialize, so a binding loaded from a `.dsb` always writes its channel
directly. Its first acceptance criterion asks for one decision covering both
constructs. This is that decision.

## What was decided

### 1. A variant transition is document data, keyed on the destination member

`VariantMember` gains an optional `transition`. A switch **to** that member
animates the way it says; a member with no transition switches in one frame,
which is what every document written before v0.18 says and what makes this an
R7-cheap append.

Three keys were available, because the document names variant sets and members
and nothing else:

- **Per set.** One spec for every switch of that set. The smallest change, and
  it cannot express asymmetry — a shelf that expands with a spring and collapses
  with a 200 ms tween is ordinary authoring.
- **Per destination member.** Subsumes per-set: give every member the same spec.
  Chosen.
- **Per interaction.** Figma's own model — a `reactions` entry carries the Smart
  Animate settings and names a destination variant. It needs a trigger construct
  the document does not have, which is story #773, and #773 needs a Figma
  fixture only a person can author. Keying #771 on it would put a schema
  decision behind a fixture.

The destination is what preserves Figma's information in a document with no
interaction construct: every reaction into a member usually animates the same
way, and that is exactly what a per-member spec says. When #773 lands, a
reaction **overrides** the member's default rather than replacing the field,
which is an append and not a migration.

**#773 landed on 2026-08-11 and it is an override, as predicted.** A member
`COMPONENT`'s own reaction gives the set's default for the member it changes to;
an `INSTANCE`'s own reaction replaces that default for the member it names and
leaves every other member's alone. `prototype-smart-animate.json` is the case
that distinguishes the two: `easing-linear` declares only the switch _to_
`state=active`, so that arm becomes LINEAR / 0.05 s while the switch back keeps
the set's own EASE_IN / 0.2 s.

### 2. A smoothing spec stays producer-side, so #255 does not follow this

`dashcue` and the schema now agree on a vocabulary, so it would be mechanical to
give `Binding` a `smoothing` field. It is deliberately not done, and #255 should
be closed as decided rather than built:

- **They are not the same construct.** A variant transition describes a
  **discrete state change** the document itself declares — the two ends are both
  in the file, and the motion between them is the only missing third thing. A
  smoothing spec describes how a channel follows a **signal**, and a signal's
  values are producer-side always (P1: the document carries intent, never
  results). The document can state where a switch goes; it cannot state what a
  signal will do.
- **The endpoints differ in kind.** A FLIP binds `from`/`to` from two solved
  layouts the runtime has in hand. A smoothed binding's `to` is whatever the
  next signal write happens to be, so the spec would describe a response to data
  the file does not contain.
- It stays reachable: a producer sets smoothing through `dashlang`'s `smooth()`,
  which is where the signal lives too.

Issue #626 — `dashlang`'s `smooth` accepts only a `Spring`, so tween and
keyframe specs are unreachable from an authored scene — is untouched by this and
remains open. It is a gap in the **producer** surface, and this decision is
about the **document**.

### 3. A step is still not a fourth arm

`TransitionSpec` has three arms in the schema, matching `dashcue`. A step is two
`Keyframe` entries sharing a `t`
(`docs/decisions/a-step-is-a-pair-of-keyframes.md`, issue #852), so the schema
this story pins is the schema it would have pinned before that ruling.

## The driver

Issue #625 sketched the fix for "a variant switch and `VariantFlip` are not
reachable through the host's scene seam" as **widening the seam** — a third
callback, or a `Scene` trait replacing the two function pointers. It was closed
as completed although the change it sketched was never made, and `VariantFlip`
had no caller outside tests.

The seam does not need widening, because what changed is _what the motion is_. A
transition is now data the arena carries, so the runtime can read it at the
switch — it does not need the scene to hand it anything. The driver therefore
lives in `LiveScene::tick`, which is already the only per-frame work both hosts
do (`dashscene-desktop`'s frame loop and `dashscene-web`'s both call it as their
one scene-driving step). **Desktop and web animate a loaded document with no
host change at all**, and `App` gains no method.

Two consequences worth stating, because they are not obvious:

- **A switch is detected, not signalled.** `Txn::set_variant` is staged on the
  arena, which `LiveScene` does not own. The scene snapshots each set's active
  member and compares once per tick. Without that the frame takes the idle early
  return and the switch is never seen — which is exactly what issue #617
  observed against every committed fixture.
- **The FLIP tracks share the scheduler the smoothed bindings use.** One
  `advance` drains both and one settled-test covers both, so an in-flight
  transition holds the idle skip open by the mechanism that already existed. A
  sampled key is a binding if `key_index` knows it and a FLIP track if
  `flip_tracks` does.

`dashscene-engine`'s `VariantFlip` is **not** what runs. `dashlang`'s library
never depends on the engine — the solver is injected — and that is a stated
property of the crate graph, not an accident. `dashlang` binds the tracks onto
its own scheduler using `dashscene_core::prop_key`, which the engine's own
manifest comment already anticipated: "no engine dependency is needed to build a
key". `VariantFlip` remains what a host drives directly, and
`goldens/tooling/tests/loaded_variant_flip.rs` exercises both paths over the
same committed bytes.

## Why `dashbuf` mirrors rather than depends

`dashcue` is dependency-free by design and the direction is that consumers
depend on it, never the reverse (SCOPE §9). So the vocabulary appears three
times as plain data — in the schema, in `dashc`'s authored `Document`, and in
`dashscene-core` — and is converted at the one crate that depends on both core
and `dashcue`. That is the same shape `Channel` already has against the schema's
`BindingChannel`, and the price of it is one mechanical enum map, about
thirty-five lines in `dashlang`.

Nothing tests that map, and nothing needs to: both of its matches are
exhaustive, with no wildcard arm, so adding a variant to either
`dashscene_core::TransitionSpec` or `dashscene_core::Easing` fails to compile
until the map is updated. That is a stronger guarantee than a test — it cannot
be forgotten — and it is why the wildcard in `initial_channel_value` was a
defect where this is not.

`PropKey` is not stored. It is opaque and caller-encoded, and the packing math
is `dashscene_core::prop_key`, so the document stores `(node, channel)` and the
runtime packs it — exactly what `Binding` already does.

## Alternatives considered

- **Give `dashlang` a dependency on `dashscene-engine` and reuse
  `VariantFlip`.** Rejected, and it would not even buy what it appears to.
  `VariantFlip::start` takes a **`dashcue`** `VariantTransition`, so the
  core-to-`dashcue` conversion survives the change untouched — nothing is
  deduplicated. What it would add is a **second `Scheduler`** inside
  `LiveScene`, since `VariantFlip` owns one and the reactive layer already has
  one: two `advance` calls, two settled-tests, and an idle skip that has to
  consult both. Putting the FLIP tracks on the existing scheduler is what keeps
  one drain and one settled-test covering both. It would also pull the solver
  into a crate whose whole design is that the solver is injected, and break an
  invariant the manifest states outright.
- **A middleman crate depending on core and `dashcue`, consumed by both
  `dashlang` and the engine.** Rejected on price: a new workspace member means
  updating eight registries and reserving a crates.io name, for a
  thirty-five-line enum map that the exhaustiveness check already protects.
- **Invert the edge — let `dashcue` depend on `dashscene-core`.** Rejected:
  `dashcue` is dependency-free by design and SCOPE §9's direction is that
  consumers depend on it and never the reverse. That rule is the reason the
  vocabulary is mirrored at all, so inverting it to remove the mirror is
  circular.
- **Keep the transition out of the arena and read it off the `dashbuf` buffer at
  the switch.** Rejected: `load_document` returns a generation and the document
  is dropped, so a host would have to keep the mapped buffer alive and re-read
  it per switch, and `attach_live` is handed an `&mut
  Arena` and not a
  document. The arena is where the switch happens, so the arena is where the
  spec has to be readable.
- **A flat side table on `Document`, keyed `(set, member)`.** Rejected:
  `VariantOverride` already nests inside `VariantMember` by the same logic, and
  a side table admits orphan and duplicate rows a validator would then have to
  police.

## Amendment, 2026-08-09 — why the rect-only rule is narrower than P1

`transition.channel-not-a-rect` reads as a P1 consequence: a FLIP track's `from`
and `to` are two **resolved** layouts, which the document must not carry, so the
engine binds them at the switch. That is true, and it is not the whole
constraint.

Story #773 tried to widen the rule to the paint channels, on the reasoning that
a paint track's endpoints are the two members' **authored** values — the same
distinction story #772 drew when a loop track was allowed to name its own
endpoints (`a-loop-is-ambient-paint-anchored-at-load.md`). That reasoning still
holds. The widening was reverted anyway, because a second constraint sits
underneath it:

**A rect track and a paint track reach the committed output by different paths,
and only one of them has a seam.** Commit takes geometry from an injected
`LayoutSolver`, so a runtime can write a sample over the solve — which is
exactly what `FlipOverlay` does. A node's paint is resolved _inside_ commit,
from the variant overlay, ahead of the node's own staged value:

    let fill = arena.overlay(id).fill.clone().or_else(|| node.fill.clone());

A paint transition animates between two members while the destination member is
active, so every staged sample is masked by the override it is travelling
towards, and the committed value is the destination from the first frame.
Measured at 2026-08-09: a `FillR` track over a half-second linear tween commits
`0.0` on the tick where an eased sample would be `0.75`.

So the rule stays rect-only until commit grows a paint seam or a precedence
layer above the overlay. **That is issue #891**, which holds the three
mechanisms considered and why none is a bolt-on. The cost of leaving it is
recorded there too: Smart Animate interpolates a colour as readily as a box, so
a fill diff — "the one every real Figma file will hit" — is refused by name
until it lands.

## Amendment, 2026-08-11 — the producer, and what it does with a non-rect diff

Story #773's lowering landed, so the rows this record designed now have a
producer other than `dashlang`: `crates/dashc/src/figma/variants.rs` emits them
from Figma's component sets, and `docs/decisions/figma-component-lowering.md`
("Amendment, 2026-08-11") holds the mapping.

Two facts it fixes in place, because they follow from the rect-only rule above
rather than restating it:

- **A non-rect difference still lowers as an override.** The rule is about
  `PropTransition`, not about `VariantOverride`: `VariantFill`, `VariantVisible`
  and `VariantRotation` are v0.4 and #770 vocabulary and are unaffected. So two
  members differing in fill lower a fill override, the switch carries it, and
  only the _track_ is refused — named `figma.prototype.unsupported-motion`,
  which is a warning. Smart Animate would have interpolated it; dashscene
  changes it in one frame.
- **`stagger` is always 0 from this producer.** Figma has no stagger, so there
  is nothing to read it from. The field stays because `dashlang` writes it.

The producer's own refusals are warnings for the reason `figma/bindings.rs`
gives — the picture is right, only the motion is not carried — so a real Figma
file whose variants differ in colour still imports, still switches, and says so.
