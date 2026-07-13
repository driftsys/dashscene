# Plan — subtree clip resolution at commit (story #97)

Test-driven: each step writes the failing test first.

1. **dashpaint contract** — `ClipBox`, `ClipIndex` (+ `UNCLIPPED`),
   `ClipRegion`, `ClipTable`; `RectEntry.clip`; drop `PaintEntry.clip`;
   `Painter::paint` gains `clips`.
   Verify: `crates/dashpaint/tests/boundary_b.rs` — table push/get/
   resolve/panic, reserved index 0, `RectEntry` = 24 bytes `#[repr(C)]`,
   the recording painter reads the resolved region.

2. **core intent** — `Prop::Clip(bool)`, `Prop::Corners { .. }`;
   `NodeData` fields.
   Verify: `crates/dashscene-core/tests/arena.rs` — corners reach the
   committed `PaintEntry`; the paint interner keys on corners too.

3. **core resolution** — build the clip table in `commit_with`;
   `CommittedScene::clips()`.
   Verify: descendant regions, the ancestor chain, region dedup across
   siblings, a non-clipping node passing the region through, a clipping
   node not clipping itself.

4. **core dirty set** — the resolved-clip-region clause.
   Verify: resizing a clipping ancestor dirties its descendants whose
   own entry bits and paint are unchanged; toggling clip off dirties
   them; a no-op commit stays clean.

5. **skia painter** — consume the region; delete the `unimplemented!`.
   Verify: `crates/dashscene-skia/tests/painter.rs` — sharp clip, rounded
   clip, nested chain intersection, via exact interior/exterior probes.

6. **golden** — `goldens/tooling/tests/v03_clips.rs` → `v03-clips.png`:
   a scene authored through `dashscene-core` (producer → commit →
   painter, the whole point of the story), 2% tolerance like the other
   anti-aliased family goldens, with bit-stable interior probes.

7. **garden** — decision record; update
   `subtree-clip-resolution-deferred.md` in place; update
   `docs/design/{dashpaint,dashscene-core-arena,dashscene-skia,goldens}.md`
   and both READMEs; empty `docs/wip/`.

8. **verify** — `just build`, `just test`, `just lint`; squash; draft PR.
