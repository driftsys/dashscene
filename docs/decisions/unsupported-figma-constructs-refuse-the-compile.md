# A construct the v0.3 `Scd` cannot express refuses the compile

    status   accepted (story #139, 2026-07-13)
    scope    crates/dashc (the figma module)
    binds    #17 (the Deno importer is where a designer meets this error),
             #140, #143, #144, #145, #146, #147 (each gap it refuses), and
             every future widening of the Figma front end

## Context

P4 — vocabulary is validated, never discovered; every out-of-profile
construct is a named diagnostic, never a silent drop.

The import gate (`docs/decisions/validator-three-gates.md`) gives a producer
one way to report a construct it will not lower: map it onto a
`dashscene_validator::Construct` and let the validator return a `Diagnostic`.
That covers every construct DESIGN §10.1 puts in the LATER or REJECT band —
layer blur, backdrop blur, advanced blend modes, corner smoothing, noise.

It does not cover a construct the v0.3 `Scd` has **no field for**. Such a
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

Option 3. A construct `Scd` cannot express is `CompileError::Unsupported`,
which names the node path and the construct and stops the compile.

    pub enum CompileError {
        Parse(serde_json::Error),
        Unsupported { path: String, what: String },
        UnresolvedImage { path: String, image_ref: String },
        Diagnostics(Report),
    }

As-built, `Unsupported` covers:

| construct                                        | why `Scd` cannot carry it                                            | debt                                                     |
| ------------------------------------------------ | -------------------------------------------------------------------- | -------------------------------------------------------- |
| a stacked fill or stroke (more than one visible) | `PaintEntry.fill`/`.stroke` are each one `Option`                    | #146                                                     |
| node opacity, rotation, mask, hidden node        | no field, and no way to hide a node without shifting the DFS indices | #143                                                     |
| a baked shadow (any unmapped effect)             | no effects vocabulary; effects enter the schema at v0.8              | #144                                                     |
| an auto-layout frame                             | no flex vocabulary — and the boxes are results, not intent           | #140 (see `figma-auto-layout-refused-on-two-grounds.md`) |
| a dashed or non-`BASIC` stroke                   | `dashpaint::Stroke` is one color, one width, one align               | #145                                                     |
| a non-`FRAME` node                               | v0.3 lowers frames only                                              | —                                                        |

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
  reports only the second. Filed as debt #149.
- **#17 (the Deno importer)** is the surface where a designer meets
  `Unsupported`. The `path` field carries the slash-joined ancestor-name chain
  precisely so the message can name a layer rather than an index; note that
  the path cannot distinguish two siblings that share a name (debt #150).
- **`Scd` widening changes the refusal set, nothing else.** When `Scd` gains
  flex (#140) or effects (v0.8), the corresponding guard is deleted and a real
  lowering replaces it. The pattern here does not change.
