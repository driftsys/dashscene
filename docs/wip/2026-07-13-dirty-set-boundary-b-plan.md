# Dirty set across boundary B — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps
> use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a painter see the dirty set that `commit` already produces,
and prove — in CI, without a GPU — that honouring it renders the same
pixels as ignoring it.

**Architecture:** `CommittedScene::dirty()` is computed on every commit
and currently consumed by nobody: `Painter::paint` does not receive it,
so R-T4 ("CPU frame cost = dirty-range instance-buffer upload from the
rect table + submission. Nothing else.") is not implementable by any
painter. This plan adds the dirty set to the trait as an **advisory**
`Option<&[u32]>`, gives `SkiaPainter` a second mode that models the R-T4
instance buffer (a retained rect-table copy refreshed only at the dirty
indices, then fully redrawn), and adds a differential test that drives
both modes over a sequence of mutate-commit-paint steps and asserts
pixel equality. No behaviour changes for any existing caller.

**Tech Stack:** Rust 2024, `dashpaint` (boundary-B types, no deps),
`dashscene-skia` (skia-safe CPU raster), the `goldens` harness crate.

## Global Constraints

- Boundary B is `dashpaint`. It must not gain a dependency on
  `dashscene-core` — the dirty set crosses as a plain `&[u32]` slice, not
  as a `CommittedScene` (`docs/decisions/painter-trait-infallible-slice-input.md`).
- **Advisory contract:** ignoring the dirty set and redrawing everything
  is always correct. A painter that honours it MUST produce output
  identical to one that does not. This is the property the oracle tests.
- **No damage-region partial redraw.** On a tiling GPU, restoring the
  previous framebuffer into tile memory to repaint part of it is the
  flush-and-resolve R-T1 forbids. The retained mode models the _instance
  buffer_, not the _pixels_: it refreshes rect-table entries, then
  redraws every quad.
- Rust edition 2024, `resolver = "3"`. `just build` must be green before
  any commit. Clippy runs with `-D warnings`.
- Commit messages are conventional, scoped to a crate from
  `.git-std.toml` (`dashpaint`, `dashscene-skia`, `goldens`, `docs`).

## Scope

This plan implements **D6** of
`docs/wip/2026-07-13-reactive-bindings-spec.md` only.

**D5 (incremental commit — retained Taffy tree, pruned readback,
retained paint interner, `LayoutSolver` partial-solve contract) is
deliberately excluded** and gets its own plan once this lands. D5 is the
change that makes the dirty set _derived from what was written_ rather
than _discovered by diffing every rect_. The oracle built here is the
test that will catch a derived dirty set that misses an entry. Build the
net before the fall.

Nothing here makes anything faster on its own. It makes R-T4 possible
and it makes D5 safe.

## File Structure

| File                                                     | Responsibility                                                                                                                          |
| -------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/dashpaint/src/lib.rs`                            | `Painter::paint` gains `dirty: Option<&[u32]>`; the advisory contract is documented on the trait.                                       |
| `crates/dashscene-skia/src/lib.rs`                       | `DirtyMode` enum; `SkiaPainter` holds a retained rect table and refreshes it from the dirty set in `Retained` mode.                     |
| `crates/dashpaint/tests/boundary_b.rs`                   | A recording painter proves the dirty set crosses the boundary in both `Some` and `None` forms.                                          |
| `crates/dashscene-skia/tests/painter.rs`                 | `Retained` mode with a complete dirty set matches `Full`; with an incomplete one it does not (proving the mode actually reads `dirty`). |
| `goldens/tooling/tests/dirty_oracle.rs`                  | The differential oracle: a mutation sequence through core, both painter modes, pixel equality at every step.                            |
| `docs/decisions/dirty-set-advisory-across-boundary-b.md` | The decision record.                                                                                                                    |

Existing `paint` call sites that must pass `None` (they hand the painter
hand-built tables and have no `CommittedScene`, so they have no dirty
set):

- `goldens/tooling/tests/v01.rs:47`
- `goldens/tooling/tests/v02_flex.rs:68`
- `goldens/tooling/tests/v03.rs:60`, `:163`
- `goldens/tooling/tests/v03_clips.rs:123`
- `goldens/tooling/tests/v03_families.rs:90`, `:158`, `:228`, `:310`
- `crates/dashpaint/tests/boundary_b.rs:279`

---

## Task 1: The dirty set crosses boundary B

`Painter::paint` gains a sixth parameter. `Option<&[u32]>` rather than
`&[u32]`: eight of the ten existing call sites build their tables by hand
and genuinely have no dirty set, so `None` states "the caller has no
dirty information" instead of forcing them to fabricate a full one.

**Files:**

- Modify: `crates/dashpaint/src/lib.rs:466-495` (the `Painter` trait)
- Modify: `crates/dashscene-skia/src/lib.rs:85-92` (the impl signature)
- Modify: the ten call sites listed above
- Test: `crates/dashpaint/tests/boundary_b.rs`

**Interfaces:**

- Produces: `Painter::paint(&mut self, rects: &[RectEntry], paints:
  &PaintTable, images: &ImageTable, clips: &ClipTable, dirty:
  Option<&[u32]>)`. Task 2 and Task 3 both call this signature.

- [ ] **Step 1: Write the failing test**

Append to `crates/dashpaint/tests/boundary_b.rs`:

```rust
/// The dirty set is advisory, but it must reach the painter. A painter
/// that wants to honour R-T4 cannot do so if boundary B does not carry
/// the set (`docs/decisions/dirty-set-advisory-across-boundary-b.md`).
#[derive(Default)]
struct RecordingPainter {
    seen_dirty: Option<Vec<u32>>,
    seen_rects: usize,
}

impl Painter for RecordingPainter {
    fn paint(
        &mut self,
        rects: &[RectEntry],
        _paints: &PaintTable,
        _images: &ImageTable,
        _clips: &ClipTable,
        dirty: Option<&[u32]>,
    ) {
        self.seen_dirty = dirty.map(<[u32]>::to_vec);
        self.seen_rects = rects.len();
    }
}

#[test]
fn the_dirty_set_crosses_boundary_b() {
    let mut paints = PaintTable::new();
    let paint = paints.push(PaintEntry::solid(RED));
    let rects = vec![RectEntry {
        x: 0.0,
        y: 0.0,
        w: 4.0,
        h: 4.0,
        paint,
        clip: ClipIndex::UNCLIPPED,
    }];

    let mut painter = RecordingPainter::default();

    // A caller with a committed scene passes the set it produced.
    painter.paint(
        &rects,
        &paints,
        &ImageTable::new(),
        &ClipTable::new(),
        Some(&[0]),
    );
    assert_eq!(painter.seen_dirty.as_deref(), Some(&[0u32][..]));
    assert_eq!(painter.seen_rects, 1);

    // A caller with hand-built tables has no dirty information.
    painter.paint(&rects, &paints, &ImageTable::new(), &ClipTable::new(), None);
    assert_eq!(painter.seen_dirty, None);
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `cargo test -p dashpaint --test boundary_b the_dirty_set_crosses_boundary_b`

Expected: FAIL to compile — `error[E0050]: method 'paint' has 6
parameters but the declaration in trait 'Painter' has 5`.

- [ ] **Step 3: Add the parameter to the trait**

In `crates/dashpaint/src/lib.rs`, change the `Painter::paint` signature
and extend its doc comment. Keep every existing doc paragraph; add the
`dirty` paragraph before `Infallible by design`:

```rust
/// `dirty` is the rect indices whose entry changed since the commit
/// that produced the previous `rects` — **advisory**. `None` means
/// the caller has no dirty information (hand-built tables, or a
/// first frame). Ignoring it and redrawing everything is always
/// correct, and a painter that honours it MUST produce output
/// identical to one that does not. It exists so a painter can meet
/// R-T4 (DESIGN_1.md §9): per-frame CPU cost is the dirty-range
/// instance-buffer upload from the rect table plus submission, and
/// nothing else. It is not a licence for damage-region partial
/// redraw, which R-T1 forbids on a tiling GPU.
fn paint(
    &mut self,
    rects: &[RectEntry],
    paints: &PaintTable,
    images: &ImageTable,
    clips: &ClipTable,
    dirty: Option<&[u32]>,
);
```

- [ ] **Step 4: Update `SkiaPainter` to the new signature**

In `crates/dashscene-skia/src/lib.rs`, change only the signature for
now — the body is untouched, so the reference painter keeps redrawing
everything, which the advisory contract permits:

```rust
impl Painter for SkiaPainter {
    fn paint(
        &mut self,
        rects: &[RectEntry],
        paints: &PaintTable,
        images: &ImageTable,
        clips: &ClipTable,
        _dirty: Option<&[u32]>,
    ) {
        let canvas = self.surface.canvas();
        canvas.clear(skia_safe::colors::TRANSPARENT);
        // ... existing draw loop, unchanged ...
```

- [ ] **Step 5: Update the ten existing call sites**

Each hand-built-table call site gains a trailing `None`. In
`goldens/tooling/tests/v03.rs:60` and the four in
`goldens/tooling/tests/v03_families.rs`, the call is a one-liner:

```rust
painter.paint(&rects, &paints, &ImageTable::new(), &ClipTable::new(), None);
```

In `goldens/tooling/tests/v03_clips.rs:123`, which does have a
`CommittedScene`, pass the set it produced:

```rust
painter.paint(
    scene.rects(),
    scene.paints(),
    &ImageTable::new(),
    scene.clips(),
    Some(scene.dirty()),
);
```

Apply the same treatment to `v01.rs:47`, `v02_flex.rs:68`,
`v03.rs:163`, and `crates/dashpaint/tests/boundary_b.rs:279` (the
existing call there has hand-built tables, so it takes `None`).

- [ ] **Step 6: Run the test and verify it passes**

Run: `cargo test -p dashpaint --test boundary_b the_dirty_set_crosses_boundary_b`

Expected: PASS.

- [ ] **Step 7: Verify nothing else regressed**

Run: `just build`

Expected: green. Every golden still matches — the reference painter's
pixels have not changed, because its body has not changed.

- [ ] **Step 8: Commit**

```bash
git add crates/dashpaint crates/dashscene-skia goldens
git commit -m "feat(dashpaint): carry the dirty set across boundary B

Painter::paint gains an advisory dirty: Option<&[u32]>. CommittedScene
computes a dirty set on every commit and no painter could see it, so
R-T4's dirty-range instance-buffer upload was not implementable. None
means the caller has no dirty information; ignoring the set and
redrawing everything stays correct."
```

---

## Task 2: `SkiaPainter` gains a retained-buffer mode

The reference painter grows a second mode that models what a product
painter on a tiling GPU actually does: keep a persistent copy of the
rect table, refresh **only** the entries the dirty set names, then redraw
every quad from that copy. It does not touch pixels selectively — R-T1
forbids that — so the mode is a faithful simulation of the _upload_ path,
and a stale entry in the retained buffer shows up as a wrong pixel.

**Files:**

- Modify: `crates/dashscene-skia/src/lib.rs:26-46` (struct + constructors)
- Modify: `crates/dashscene-skia/src/lib.rs:85-95` (the `paint` body)
- Test: `crates/dashscene-skia/tests/painter.rs`

**Interfaces:**

- Consumes: `Painter::paint(..., dirty: Option<&[u32]>)` from Task 1.
- Produces: `dashscene_skia::DirtyMode` (`Full` | `Retained`) and
  `SkiaPainter::with_mode(width: i32, height: i32, mode: DirtyMode) ->
  SkiaPainter`. Task 3 uses both.

- [ ] **Step 1: Write the failing test**

Append to `crates/dashscene-skia/tests/painter.rs`:

**What the retained buffer can and cannot go stale on — read this
before writing the test.** The buffer holds `RectEntry` values, and a
`RectEntry` carries _indices_ (`paint`, `clip`), not resolved colours or
regions. The paint and clip tables are handed to the painter fresh on
every frame. So a stale entry only renders wrong pixels when the entry's
**bits** changed — `x`, `y`, `w`, `h`, the paint index, or the clip
index. A fill change that happens to re-intern to the _same_ index
leaves the bits identical, and the stale entry resolves against the new
table and renders correctly.

That is not a hole in the simulation: it is exactly R-T4, which names the
_rect table_ as the thing that delta-uploads. The small paint and clip
tables re-upload wholesale, which they must today anyway, because both
are re-interned from scratch every commit and their indices are
therefore unstable. The tests below must mutate **geometry** (or force an
index shift) to create staleness. A colour swap alone will not.

```rust
use dashscene_skia::DirtyMode;

/// Two side-by-side rects, so an incomplete dirty set can starve one.
fn two_rects(left_w: f32) -> (Vec<RectEntry>, PaintTable) {
    let mut paints = PaintTable::new();
    let l = paints.push(PaintEntry::solid(RED));
    let r = paints.push(PaintEntry::solid(GREEN));
    let rects = vec![
        RectEntry { x: 0.0, y: 0.0, w: left_w, h: 16.0, paint: l, clip: ClipIndex::UNCLIPPED },
        RectEntry { x: 8.0, y: 0.0, w: 8.0, h: 16.0, paint: r, clip: ClipIndex::UNCLIPPED },
    ];
    (rects, paints)
}

fn render(mode: DirtyMode, frames: &[(Vec<RectEntry>, PaintTable, Option<Vec<u32>>)]) -> Vec<u8> {
    let mut painter = SkiaPainter::with_mode(16, 16, mode);
    for (rects, paints, dirty) in frames {
        painter.paint(
            rects,
            paints,
            &ImageTable::new(),
            &ClipTable::new(),
            dirty.as_deref(),
        );
    }
    painter.rgba_bytes()
}

/// With a complete dirty set, the retained buffer always equals the
/// input table, so the retained mode is pixel-identical to a full
/// redraw. This is the advisory contract.
#[test]
fn retained_mode_with_a_complete_dirty_set_matches_a_full_redraw() {
    let (r0, p0) = two_rects(8.0);
    let (r1, p1) = two_rects(4.0); // rect 0's width changed: its bits differ

    let frames = vec![
        (r0, p0, None),          // first frame: no dirty information
        (r1, p1, Some(vec![0])), // rect 0 is dirty, rect 1 is not
    ];

    let full = render(DirtyMode::Full, &frames);
    let retained = render(DirtyMode::Retained, &frames);
    assert_eq!(full, retained, "a complete dirty set must not change the pixels");
}

/// The mode must actually read `dirty`. If it does, withholding a
/// changed index leaves a stale entry in the retained buffer and the
/// pixels diverge. If this test passes trivially, `Retained` is not
/// honouring the set and the oracle in Task 3 would prove nothing.
#[test]
fn retained_mode_starves_on_an_incomplete_dirty_set() {
    let (r0, p0) = two_rects(8.0);
    let (r1, p1) = two_rects(4.0); // rect 0 shrank...

    let frames = vec![
        (r0, p0, None),
        (r1, p1, Some(vec![])), // ...but the dirty set does not say so
    ];

    let full = render(DirtyMode::Full, &frames);
    let retained = render(DirtyMode::Retained, &frames);
    assert_ne!(
        full, retained,
        "an incomplete dirty set must leave the retained buffer stale"
    );
}
```

Add `RED` and `GREEN` colour constants and the `dashpaint` imports
(`ClipIndex`, `ClipTable`, `Color`, `ImageTable`, `PaintEntry`,
`PaintTable`, `Painter`, `RectEntry`) at the top of the file if the
existing test module does not already have them.

- [ ] **Step 2: Run the tests and verify they fail**

Run: `cargo test -p dashscene-skia --test painter retained_mode`

Expected: FAIL to compile — `error[E0432]: unresolved import
'dashscene_skia::DirtyMode'`.

- [ ] **Step 3: Add `DirtyMode` and the retained buffer**

In `crates/dashscene-skia/src/lib.rs`, replace the struct and its
constructor block:

```rust
/// How a painter treats the advisory dirty set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DirtyMode {
    /// Redraw every rect from the caller's table. Ignores `dirty`, and
    /// is always correct — the reference behaviour, and what the golden
    /// images are rendered with.
    #[default]
    Full,
    /// Model R-T4: keep a persistent copy of the rect table, refresh
    /// only the entries `dirty` names, and redraw every quad from that
    /// copy. This simulates a product painter's instance buffer, so a
    /// dirty set that omits a changed rect leaves a stale entry and
    /// renders a stale pixel — which is what makes the two modes a
    /// differential test of the dirty set
    /// (`goldens/tooling/tests/dirty_oracle.rs`).
    Retained,
}

/// The reference painter: draws boundary-B input onto a CPU raster
/// surface (N32 premultiplied).
pub struct SkiaPainter {
    surface: skia_safe::Surface,
    mode: DirtyMode,
    /// The simulated instance buffer. Empty in `Full` mode.
    retained: Vec<RectEntry>,
}

impl SkiaPainter {
    /// A CPU raster surface of the given pixel size, in [`DirtyMode::Full`].
    ///
    /// # Panics
    ///
    /// Panics if `width` or `height` is not positive.
    pub fn new(width: i32, height: i32) -> Self {
        Self::with_mode(width, height, DirtyMode::Full)
    }

    /// A CPU raster surface of the given pixel size, in `mode`.
    ///
    /// # Panics
    ///
    /// Panics if `width` or `height` is not positive.
    pub fn with_mode(width: i32, height: i32, mode: DirtyMode) -> Self {
        assert!(
            width > 0 && height > 0,
            "surface dimensions must be positive, got {width}x{height}"
        );
        let surface =
            surfaces::raster_n32_premul((width, height)).expect("raster surface allocation");
        Self {
            surface,
            mode,
            retained: Vec::new(),
        }
    }
```

Leave `png_bytes` and `rgba_bytes` exactly as they are.

- [ ] **Step 4: Refresh the retained buffer in `paint`**

In `crates/dashscene-skia/src/lib.rs`, replace the head of the `paint`
body (everything before the `for rect in ...` loop). The draw loop itself
is unchanged except that it iterates `source` instead of `rects`:

```rust
impl Painter for SkiaPainter {
    fn paint(
        &mut self,
        rects: &[RectEntry],
        paints: &PaintTable,
        images: &ImageTable,
        clips: &ClipTable,
        dirty: Option<&[u32]>,
    ) {
        // Refresh the simulated instance buffer, then draw from it.
        // A full refresh when the caller has no dirty set, or when the
        // node count changed (every index is new, so the whole buffer
        // re-uploads — this is the first-frame and structural-change
        // path).
        if self.mode == DirtyMode::Retained {
            match dirty {
                Some(indices) if self.retained.len() == rects.len() => {
                    for &i in indices {
                        let i = i as usize;
                        self.retained[i] = rects[i];
                    }
                }
                _ => {
                    self.retained.clear();
                    self.retained.extend_from_slice(rects);
                }
            }
        }

        // Disjoint field borrows: `retained` is read while `surface` is
        // borrowed mutably.
        let source: &[RectEntry] = match self.mode {
            DirtyMode::Full => rects,
            DirtyMode::Retained => &self.retained,
        };

        let canvas = self.surface.canvas();
        canvas.clear(skia_safe::colors::TRANSPARENT);
        for rect in source {
            // ... existing draw loop body, unchanged ...
```

The rest of the loop — paint resolution, clip save/restore, the
`Solid` / `Gradient` / `Image` arms — is untouched.

- [ ] **Step 5: Run the tests and verify they pass**

Run: `cargo test -p dashscene-skia --test painter retained_mode`

Expected: PASS, both tests. `retained_mode_starves_on_an_incomplete_dirty_set`
passing is what proves the mode reads `dirty` at all.

- [ ] **Step 6: Run the full suite**

Run: `just build`

Expected: green. Goldens are rendered in `DirtyMode::Full` (the
`SkiaPainter::new` default), so no golden image changes.

- [ ] **Step 7: Commit**

```bash
git add crates/dashscene-skia
git commit -m "feat(dashscene-skia): add the retained-buffer dirty mode

DirtyMode::Retained keeps a persistent copy of the rect table, refreshes
only the entries the dirty set names, and redraws every quad from that
copy. It models a product painter's instance buffer (R-T4) rather than
doing damage-region partial redraw, which R-T1 forbids on a tiling GPU.
Full stays the default, so the goldens are unaffected."
```

---

## Task 3: The differential oracle

The two modes must agree over a _sequence_. Staleness only exists across
frames, so a single-frame comparison cannot catch a dirty set that omits
an entry — the retained buffer would have been fully populated on the
first frame and would never have had a chance to go stale.

This test is the reason the previous two tasks exist. It is the
regression net that D5 (the incremental commit) will be built against.

**Files:**

- Create: `goldens/tooling/tests/dirty_oracle.rs`

**Interfaces:**

- Consumes: `DirtyMode`, `SkiaPainter::with_mode` (Task 2);
  `Painter::paint(..., dirty)` (Task 1); `dashscene_core::{Arena, Prop,
  CommittedScene}`.

- [ ] **Step 1: Write the test**

Create `goldens/tooling/tests/dirty_oracle.rs`:

```rust
//! The dirty-set oracle.
//!
//! `DirtyMode::Retained` refreshes its rect-table copy only at the
//! indices the dirty set names, so a dirty set that omits a changed rect
//! leaves a stale entry and renders a stale pixel. `DirtyMode::Full`
//! ignores the set entirely and is correct by construction. Rendering a
//! mutation sequence through both and comparing pixels at every step is
//! therefore a test of `commit`'s dirty set, not of the painter.
//!
//! On a product painter the same bug is a stale instance-buffer entry —
//! a frozen gauge, a telltale that will not clear — which is
//! intermittent and hard to diagnose on target hardware. Here it is a
//! deterministic pixel diff in CI, with no GPU.
//!
//! Staleness only exists across frames: a single-frame comparison would
//! pass trivially, because the retained buffer is fully populated on the
//! first paint. The sequence is the test.

use dashpaint::{ImageTable, Painter};
use dashscene_core::{Arena, Color, NodeId, Prop};
use dashscene_skia::{DirtyMode, SkiaPainter};

const SIZE: i32 = 64;

fn rgba(r: f32, g: f32, b: f32) -> Color {
    Color { r, g, b, a: 1.0 }
}

/// Paints the arena's committed scene into `painter`, handing it the
/// dirty set that commit produced.
fn paint(painter: &mut SkiaPainter, arena: &Arena) {
    let scene = arena.committed();
    painter.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        Some(scene.dirty()),
    );
}

/// A frame: mutate, commit, paint into both painters, and require that
/// they agree. Panics with the step name on divergence.
fn step(
    label: &str,
    arena: &mut Arena,
    full: &mut SkiaPainter,
    retained: &mut SkiaPainter,
    mutate: impl FnOnce(&mut dashscene_core::Txn<'_>),
) {
    let mut txn = arena.open();
    mutate(&mut txn);
    txn.commit();

    paint(full, arena);
    paint(retained, arena);

    assert_eq!(
        full.rgba_bytes(),
        retained.rgba_bytes(),
        "dirty set is incomplete after '{label}': the retained buffer \
         rendered different pixels from a full redraw, which means commit \
         did not mark every rect whose rendered output changed"
    );
}

/// A clipping frame with two children, mutated through the cases that
/// have historically broken dirty sets:
///
/// - a geometry change (rect bits differ);
/// - a fill change (rect bits identical, resolved paint differs);
/// - a *new* fill that shifts the paint table's interning order, so an
///   untouched node's paint index now resolves to a different entry;
/// - resizing the clipping ancestor, which changes a child's resolved
///   clip region without touching the child's own rect;
/// - a commit that changes nothing at all.
#[test]
fn the_dirty_set_survives_a_mutation_sequence() {
    let mut arena = Arena::new();

    let (frame, left, right) = {
        let mut txn = arena.open();
        let frame = txn.add_node(None, Some("frame"));
        txn.set_prop(frame, Prop::Width(48.0));
        txn.set_prop(frame, Prop::Height(48.0));
        txn.set_prop(frame, Prop::Fill(rgba(0.06, 0.08, 0.16)));
        txn.set_prop(frame, Prop::Clip(true));

        let left = txn.add_node(Some(frame), Some("left"));
        txn.set_prop(left, Prop::X(4.0));
        txn.set_prop(left, Prop::Y(4.0));
        txn.set_prop(left, Prop::Width(16.0));
        txn.set_prop(left, Prop::Height(40.0));
        txn.set_prop(left, Prop::Fill(rgba(0.9, 0.2, 0.1)));

        let right = txn.add_node(Some(frame), Some("right"));
        txn.set_prop(right, Prop::X(26.0));
        txn.set_prop(right, Prop::Y(4.0));
        txn.set_prop(right, Prop::Width(16.0));
        txn.set_prop(right, Prop::Height(40.0));
        txn.set_prop(right, Prop::Fill(rgba(0.2, 0.7, 0.4)));
        txn.commit();
        (frame, left, right)
    };

    let mut full = SkiaPainter::with_mode(SIZE, SIZE, DirtyMode::Full);
    let mut retained = SkiaPainter::with_mode(SIZE, SIZE, DirtyMode::Retained);

    // Frame 0: both painters see the whole scene for the first time.
    paint(&mut full, &arena);
    paint(&mut retained, &arena);
    assert_eq!(full.rgba_bytes(), retained.rgba_bytes(), "first frame");

    step("move left", &mut arena, &mut full, &mut retained, |txn| {
        txn.set_prop(left, Prop::X(6.0));
    });

    step("recolor right", &mut arena, &mut full, &mut retained, |txn| {
        txn.set_prop(right, Prop::Fill(rgba(0.9, 0.8, 0.1)));
    });

    // Recolouring `left` to a colour that did not previously exist
    // re-interns the paint table in a different order, so `right`'s
    // paint index can now resolve to a different entry even though
    // `right` was not touched. The dirty set must catch that.
    step("shift the paint table", &mut arena, &mut full, &mut retained, |txn| {
        txn.set_prop(left, Prop::Fill(rgba(0.1, 0.3, 0.9)));
    });

    // Shrinking the clipping frame changes both children's resolved clip
    // region without changing either child's own rect entry.
    step("shrink the clip", &mut arena, &mut full, &mut retained, |txn| {
        txn.set_prop(frame, Prop::Height(24.0));
    });

    // A commit that changes nothing must produce an empty dirty set and
    // leave the retained buffer untouched — and still match.
    step("no-op commit", &mut arena, &mut full, &mut retained, |_txn| {});
}

/// A guard on the guard: if `DirtyMode::Retained` stopped honouring the
/// dirty set (or the dirty set became "always everything"), the oracle
/// above would pass no matter what. Withholding a known-dirty index must
/// still diverge.
///
/// The mutation must change the rect entry's **bits** — geometry here.
/// A fill change would not do: the retained entry carries a paint
/// *index*, and the paint table is handed to the painter fresh each
/// frame, so a stale entry whose index is unchanged still resolves to
/// the new colour and renders correctly.
#[test]
fn the_oracle_can_fail() {
    let mut arena = Arena::new();
    let node: NodeId = {
        let mut txn = arena.open();
        let node = txn.add_node(None, Some("box"));
        txn.set_prop(node, Prop::Width(32.0));
        txn.set_prop(node, Prop::Height(32.0));
        txn.set_prop(node, Prop::Fill(rgba(0.9, 0.2, 0.1)));
        txn.commit();
        node
    };

    let mut full = SkiaPainter::with_mode(SIZE, SIZE, DirtyMode::Full);
    let mut retained = SkiaPainter::with_mode(SIZE, SIZE, DirtyMode::Retained);
    paint(&mut full, &arena);
    paint(&mut retained, &arena);

    let mut txn = arena.open();
    txn.set_prop(node, Prop::Width(12.0));
    txn.commit();

    let scene = arena.committed();
    assert!(!scene.dirty().is_empty(), "the width change must be dirty");

    // Hand the retained painter an empty set — a simulated dirty-set bug.
    full.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        Some(scene.dirty()),
    );
    retained.paint(
        scene.rects(),
        scene.paints(),
        &ImageTable::new(),
        scene.clips(),
        Some(&[]),
    );

    assert_ne!(
        full.rgba_bytes(),
        retained.rgba_bytes(),
        "withholding a dirty index must diverge, or the oracle proves nothing"
    );
}
```

- [ ] **Step 2: Run the oracle**

Run: `cargo test -p goldens --test dirty_oracle`

Expected: both tests PASS. `the_dirty_set_survives_a_mutation_sequence`
passing says the dirty set on `main` is complete for these cases.
`the_oracle_can_fail` passing says the test would have noticed if it
were not.

If `the_dirty_set_survives_a_mutation_sequence` **fails**, stop: that is
a real dirty-set bug in `commit_with` on `main`, and the failing step
name says which case. Fix it in `crates/dashscene-core/src/arena.rs`
(the diff is at `arena.rs:733-747`, and `entry_bits` at `arena.rs:895`)
in its own commit before continuing.

- [ ] **Step 3: Run the full suite**

Run: `just build`

Expected: green.

- [ ] **Step 4: Commit**

```bash
git add goldens/tooling/tests/dirty_oracle.rs
git commit -m "test(goldens): add the dirty-set differential oracle

Renders a mutation sequence through DirtyMode::Full and
DirtyMode::Retained and asserts pixel equality at every step. Retained
refreshes its rect-table copy only at the dirty indices, so a dirty set
that omits a changed rect renders a stale pixel. Covers geometry, fill,
a paint-table interning shift that moves an untouched node's index, a
clip-region change with no rect change, and a no-op commit.

the_oracle_can_fail guards the guard: withholding a known-dirty index
must diverge, so the suite cannot pass vacuously."
```

---

## Task 4: Record the decision

**Files:**

- Create: `docs/decisions/dirty-set-advisory-across-boundary-b.md`
- Modify: `docs/decisions/README.md` (add the one-line index entry, in
  the style of the existing entries)

- [ ] **Step 1: Write the record**

Create `docs/decisions/dirty-set-advisory-across-boundary-b.md`,
following the house format (`status` / `scope` block, then Context,
Options, Choice, Why):

```markdown
# The dirty set crosses boundary B as advisory input

    status   accepted (2026-07-13)
    scope    dashpaint's Painter trait; dashscene-skia's modes

## Context

`commit` computes a dirty set on every commit, and no painter could see
it: `Painter::paint` took only the four tables. R-T4 (DESIGN_1.md §9)
specifies the per-frame CPU cost as "dirty-range instance-buffer upload
from the rect table + submission. Nothing else." — which no painter
could implement.

## Options

1. Pass the dirty set as an advisory `Option<&[u32]>` on `paint`.
2. Add a separate `paint_incremental` method with a default
   implementation that delegates to `paint`.
3. Pass the whole `CommittedScene`.

## Choice

Option 1. `None` means the caller has no dirty information (hand-built
tables, or a first frame); `Some(&[])` means nothing changed. Ignoring
the set is always correct, and a painter that honours it must produce
identical output.

`DirtyMode::Full` and `DirtyMode::Retained` on `SkiaPainter` implement
both halves of that contract, and the two are compared over a mutation
sequence in `goldens/tooling/tests/dirty_oracle.rs`.

## Why

- Option 3 would make `dashpaint` depend on `dashscene-core`, collapsing
  boundary B (`painter-trait-infallible-slice-input.md`). A slice keeps
  the painter free of the semantic model.
- Option 2 leaves two entry points that can disagree, and the product
  painter would implement one and stub the other. One method with an
  explicit "no information" case is the honest signature.
- The retained mode models the **instance buffer**, not the pixels. It
  is not damage-region partial redraw: restoring a framebuffer into tile
  memory to repaint part of it is the flush-and-resolve R-T1 forbids.
  The GPU redraws every quad in one pass; what R-T4 removes is the CPU
  work and the upload.
- The reference painter's second mode exists as a **test oracle**, not
  for speed. A dirty set that omits a changed rect is a stale
  instance-buffer entry on the product painter — intermittent, and
  diagnosed on target hardware. The same bug is a deterministic pixel
  diff in CI here, with no GPU. That is what makes the incremental
  commit (the retained Taffy tree and the derived dirty set) safe to
  build next.
```

- [ ] **Step 2: Format and lint**

Run: `dprint fmt && npx markdownlint-cli docs/decisions/dirty-set-advisory-across-boundary-b.md`

Expected: clean. Code blocks in `docs/` are indented, not fenced
(`.markdownlint.json` sets MD046 to `indented`) — except inside this
record, whose own fenced block is the record's content and is written as
an indented block in the real file.

- [ ] **Step 3: Commit**

```bash
git add docs/decisions
git commit -m "docs(docs): record the advisory dirty set across boundary B"
```

---

## Definition of done

Per `AGENTS.md` "Story workflow":

- [ ] `just build` green.
- [ ] `/code-review` run on the diff; every finding captured as a
      checklist in the PR description.
- [ ] Critical findings fixed; each minor finding filed as its own
      `debt`-labelled issue linked to the story.
- [ ] Branch rebased onto `main`, squashed to one conventional commit,
      force-pushed; merged with `gh pr merge --merge`.

## What this plan does not do

- It does not make anything faster. No painter in the tree honours the
  dirty set for real work yet; `Retained` exists to test the set.
- It does not touch `commit_with`, the `LayoutSolver` contract, the
  Taffy solver, or the paint interner. That is D5, and it gets its own
  plan — written against the oracle this plan builds.
