# Plan — masks + group opacity (story #44)

TDD throughout: red test first, then green. Bottom-up so each crate builds
on a green dependency.

1. **dashpaint (boundary B).**
   - Add `RectEntry.opacity: f32` (effective free-path alpha). Update the
     pinned size test (24 -> 28 bytes).
   - Add `GroupComposite { start: u32, end: u32, alpha: f32 }`.
   - Add `groups: &[GroupComposite]` to `Painter::paint` (after `clips`).
   - Verify: `crates/dashpaint/tests/boundary_b.rs` exercises the new field
     and the new param; RecordingPainter records the groups.

2. **dashscene-skia.**
   - Per-rect: multiply the paint alpha by `rect.opacity`.
   - Render-target groups: `save_layer_alpha` at a group's `start`, pop with
     `restore` when the innermost active group's `end` is reached (a stack).
   - Verify: painter tests — a free-path rect at 0.5 alpha, an overlapping
     pair composited as a group (the group result differs from the
     per-rect-alpha result).

3. **dashscene-core.**
   - `Prop::Opacity(f32)`, `Prop::Mask(bool)`; `Node.opacity`, `Node.mask`;
     `Arena::opacity` reader.
   - Commit walk: mask -> add the mask box to following siblings' clip
     regions and resolve the mask node to draws-nothing; opacity -> per-rect
     free alpha + the group table + overlap detection (pairwise overlap of
     painted subtree rects).
   - `CommittedScene.groups()` accessor.
   - `prop_class`: Opacity and Mask are paint-side (no solve).
   - Verify: commit tests — free vs render-target split, mask-as-clip,
     effective alpha down a chain, dirty on an opacity change.

4. **core Channel (#253).** `Channel::Opacity = 9`, `ALL`, code round-trip.

5. **dashlang reactive (#253).** Route `Channel::Opacity` through
   `prop_for`/`classify`/`initial_channel_value` (paint-only, non-fill
   scalar). A1-mirror test: a bound opacity write is paint-only (no solve),
   and the opacity tracks the signal.

6. **schema (dashbuf.fbs).** Append `Node.opacity = 1.0`, `Node.mask = false`,
   `Node.visible = true`; `BindingChannel.Opacity = 9`. Regenerate the frozen
   fixture (`UPDATE_DSB_FIXTURE=1`) with the new fields at non-default values,
   add assertions, extend the roundtrip suites.

7. **core load path.** Read `node.opacity()`/`mask()`/`visible()` into the
   arena (stage only when non-default, the min/max pattern).

8. **dashc (#143 + #253).**
   - `document::Node` gains `opacity`/`mask`/`visible`; `BindingChannel::Opacity`.
   - `emit.rs` writes them.
   - `figma/mod.rs`: drop the opacity/mask/hidden blockers, lower them into
     the DocNode; keep the rotation blocker (P4).
   - `figma/bindings.rs`: map a bound node opacity to `Channel::Opacity`.
   - Verify: lowering tests — a node opacity/mask/hidden node lowers; a
     rotated node still refuses by name.

9. **validator (Q-6).** `RENDER_TARGET_BUDGET_PLACEHOLDER`, a scene-gate
   warning counting render-target groups, one test.

10. **goldens.** Masked scene, group-opacity free path, group-opacity
    render-target path (authored against core, the clip-golden pattern).

11. **Gates + gardening.** `just build`, `just verify`, `just wasm`; garden
    `docs/wip/` into the design records + a decision record; move raw
    spec/plan to `docs/archive/`.

## Verification per step

Each numbered step is `cargo test -p <crate>` green before moving on;
`just build` green at the end.
