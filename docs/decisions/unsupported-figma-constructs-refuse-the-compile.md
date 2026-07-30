# A construct the v0.3 `Document` cannot express refuses the compile

    status   accepted (story #139, 2026-07-13); mechanism revised by story
             #140 (2026-07-16) — see "Revised at #140" below; the verdict made
             policy-dependent by story S0-impl (2026-07-18) — see "Revised at
             S0-impl" below
    scope    crates/dashc (lib, the figma module, the abi wire),
             importers/figma
    binds    #17 (the Deno importer is where a designer meets this error),
             #140, #143, #144, #145, #146, #147 (each gap it refuses), and
             every future widening of the Figma front end

## Revised at S0-impl: the verdict is a policy, not a fixed refusal

The verdict below — "correct or refused, never approximately right" — is now
one of two policies, chosen per compile by an `EmitPolicy` that threads from
the ABI request (`crates/dashc/src/abi/wire.rs`) into the walk. The full-real-
file-import epic's S0 gate decided this (human, 2026-07-18): a real, media-rich
Figma file hits at least one construct the vocabulary cannot express, so an
all-or-nothing refusal made "full import" unreachable — one unsupported node
anywhere refused every byte.

- **`EmitPolicy::Strict`** is the original posture, unchanged: any vocabulary
  gap is an error and the file refuses to emit (R6). This stays the Rust
  library default, so every existing caller and test keeps today's behavior.
- **`EmitPolicy::Partial`** downgrades one class of diagnostic — the
  `figma.unsupported` omission — from error to warning. The node's subtree is
  still skipped (nothing is lowered approximately), so the document emits with
  the covered majority and the skipped nodes ride back as named warnings. The
  Deno importer defaults to Partial; its new `--strict` flag restores Strict.

The line partial-emit does **not** cross is approximation. Two kinds of
vocabulary diagnostic already exist, and Partial treats them oppositely:

- **Omission** (`figma.unsupported`, minted by `Walk::unsupported_at`): the
  node is skipped, leaving a hole plus a named diagnostic. This is the only
  policy-sensitive diagnostic. Downgraded to a warning under Partial.
- **Approximation-if-shipped** (a REJECT-band construct triaged on the success
  path — noise, texture, progressive blur): the node **is** lowered, just
  without the rejected feature, so shipping it would render a picture the
  designer never authored. Stays a fatal error in both modes.

`figma.no-content` (a zero-node `.dsb` a downstream loader panics on) and a
parse failure also stay fatal in both modes. An unresolved image ref did too,
until issue #484 (2026-07-29) narrowed that: under Partial it is now fatal
only for a node that has no other blocker — a node already headed for the
skip over another blocker gets the unresolved ref folded into that same skip
instead, named alongside it. See
`docs/decisions/dashc-identifies-images-never-decodes.md` for the ruling.

This is within R6, which permits a vocabulary gap to be a warning
(`docs/specification/01-goals-and-requirements.md`), and it strengthens P4
rather than weakening it: a skip is still a named diagnostic, only its severity
changes, and nothing becomes silent. P1 holds because a skipped node leaves
nothing behind — no baked box, no placeholder extent.

**Consequence accepted at this gate.** A file whose _only_ blocker is a
REJECT-band construct on an otherwise-lowerable node still refuses under
Partial. Omitting just that one node from the success path — moving its triage
into the skip decision — is a per-construct follow-up, filed only if a real
target needs it. The conservative "still refuses" is the faithful reading of
"never approximate".

_Follow-up executed for backdrop blur (story B1, 2026-07-19), and superseded
at story #393 (2026-07-26)._ B1's `VECTOR` lowering un-skipped the Landify
hero's background vector; the node's backdrop blur — an error verdict under
profile:core — then reached the triage on the success path and blocked the
whole document, where before B1 the node skipped as an unsupported type and
emitted with a warning. Under Partial, a backdrop blur whose verdict was an
error joined the skip decision: the node was omitted whole (never lowered
minus its blur) and the gap named as the policy-sensitive `figma.unsupported`
omission.

That mechanism is gone. `docs/decisions/backdrop-blur-is-core-vocabulary.md`
made backdrop blur core vocabulary that lowers into the schema, so there is
no error verdict left to move into the skip decision, no
`profile.backdrop-blur` rule, and no whole-node omission. The node lowers and
keeps its blur under both policies. The reasoning above is kept because it is
the worked example of this rule's per-construct follow-up, and it is the
place to start if another construct ever needs the same treatment — no other
construct has moved.

## Revised at #140: the refusal is a diagnostic, and the walk keeps going

The verdict is unchanged — a construct the document cannot express is never
lowered approximately and never dropped in silence, and a file carrying one
never emits (R6). What changed is the mechanism. `CompileError::Unsupported`
stopped the compile at the first finding, which discarded every diagnostic
already collected (debt #149): a designer fixed one construct per compile.
Since #140, an unsupported construct is an **error-severity diagnostic**
under `dashc`'s own rule id `figma.unsupported` — producer-assembled, which
`docs/decisions/producer-assembles-its-own-diagnostics.md` made possible
after this record was written, and which does not add the `Construct`
variant P5 forbids. The offending node's subtree is skipped, the walk
continues, and one pass reports every finding. The error severity is what
keeps R6 holding: `compile_figma` returns `CompileError::Diagnostics` and
the bytes are discarded.

`CompileError::Unsupported` remains for the one refusal with no node to
diagnose at: a file with no root `FRAME` under its first `CANVAS`.

The as-built refused set is `docs/design/dashc.md`'s; the table below is
the v0.3 set this record originally froze.

## Context

P4 — vocabulary is validated, never discovered; every out-of-profile
construct is a named diagnostic, never a silent drop.

The import gate (`docs/decisions/validator-three-gates.md`) gives a producer
one way to report a construct it will not lower: map it onto a
`dashscene_validator::Construct` and let the validator return a `Diagnostic`.
That covers every construct
`docs/specification/04-figma-vocabulary-profile.md` puts in the LATER or
REJECT band —
layer blur, backdrop blur, advanced blend modes, corner smoothing, noise.

It does not cover a construct the v0.3 `Document` has **no field for**. Such a
construct is in neither band. It is not out-of-profile vocabulary the
validator holds a verdict about; it is vocabulary the document model cannot
carry at all. `Construct` has no variant for it, and adding one would turn
the validator's enum into a list of one producer's expressiveness gaps, which
P5 forbids. So it cannot become a `Diagnostic`, and the lowering has exactly
three options.

## Options

1. Lower it approximately — keep the part that fits, drop the part that does
   not.
2. Drop the node or the property in silence.
3. Refuse the compile.

## Choice

Option 3. A construct `Document` cannot express is `CompileError::Unsupported`,
which names the node path and the construct and stops the compile.

    pub enum CompileError {
        Parse(serde_json::Error),
        Unsupported { path: String, what: String },
        UnresolvedImage { path: String, image_ref: String },
        Diagnostics(Report),
    }

As-built, `Unsupported` covers:

| construct                                        | why `Document` cannot carry it                                      | debt                                                     |
| ------------------------------------------------ | ------------------------------------------------------------------- | -------------------------------------------------------- |
| a stacked fill or stroke (more than one visible) | `PaintEntry.fill`/`.stroke` are each one `Option`                   | #146                                                     |
| node rotation                                    | no rotation vocabulary (opacity/mask/hidden un-pinned at v0.8, #44) | #143 (remainder)                                         |
| a soft (alpha/luminance) or text-shaped mask     | the clip-region model is a hard box clip only (v0.8, #44)           | `masks-and-group-opacity.md`                             |
| an auto-layout frame                             | no flex vocabulary — and the boxes are results, not intent          | #140 (see `figma-auto-layout-refused-on-two-grounds.md`) |
| a dashed or non-`BASIC` stroke                   | `dashpaint::Stroke` is one color, one width, one align              | #145                                                     |
| a non-`FRAME` node                               | v0.3 lowers frames only                                             | —                                                        |

## Why

- **Option 2 is the silent drop P4 exists to forbid.** It is the failure mode
  the whole validator design is built around.
- **Option 1 is worse than option 2, not better.** An approximate lowering
  produces output that looks plausible. A stacked fill lowered to its
  bottom-most layer, a dashed border repainted as a continuous one, a
  40-percent-opacity node painted opaque — each renders a picture the designer
  never authored, and nothing anywhere in the pipeline says so. A silent drop
  at least leaves a hole; an approximation leaves a lie. This is also why
  `imageTransform` and `scalingFactor` are lowered rather than dropped: both
  are already `dashpaint` vocabulary, so dropping them would not be an
  expressiveness gap — it would silently lower a cropped or tiled image to a
  _wrong_ image.
- **Refusal is loud, local, and cheap to lift.** Each refusal is one debt
  issue; closing it means adding the field (or the `Construct` variant) and
  deleting one guard.

The cost is accepted deliberately: a real Figma file will hit these. Hidden
layers are routine, and the v0.3 front end refuses them. So this front end
compiles the captured fixture and refuses much of the wild. That is the
intended trade — correct or refused, never approximately right.

## Consequences

- **Every gap above is a filed debt issue, not a papered-over branch.** The
  guard and the issue land together, so the refusal is a tracked absence
  rather than a permanent policy.
- **The diagnostics found before the first refusal are lost.** `lower` returns
  `Err(CompileError::Unsupported)`, and the warnings it had already collected
  go with it, so a file with both a warning and an unsupported construct
  reports only the second. Filed as debt #149. _(Resolved by the #140
  revision above: every finding survives one pass.)_
- **#17 (the Deno importer)** is the surface where a designer meets
  `Unsupported`. The `path` field carries the slash-joined ancestor-name chain
  precisely so the message can name a layer rather than an index; note that
  the path cannot distinguish two siblings that share a name (debt #150).
  _(Resolved at #140: a duplicated sibling name is suffixed with the node's
  Figma id.)_
- **`Document` widening changes the refusal set, nothing else.** When `Document` gains
  flex (#140) or effects (v0.8), the corresponding guard is deleted and a real
  lowering replaces it. The pattern here does not change. _(#140 did both:
  it deleted the auto-layout guard and revised the pattern's mechanism —
  see "Revised at #140" above. #45 did it for effects: the document gained a
  shadow vocabulary, so drop and inner shadows now lower and the baked-shadow
  refusal is retired — `docs/decisions/effects-vocabulary-shadows.md`, debt
  #144 resolved. Noise, texture, and progressive blur stay REJECT-band, and a
  shadow with no color is a malformed-value refusal, not an expressiveness
  gap — the same class as "a SOLID with no color", so neither is tabled
  above.)_
