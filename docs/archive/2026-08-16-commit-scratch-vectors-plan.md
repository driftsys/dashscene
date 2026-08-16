# Commit scratch vectors implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:executing-plans`
> to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for
> tracking.

**Goal:** Make `Txn::commit_with`'s per-node scratch scale with the shown root's
subtree rather than with the whole document, and give the per-frame band a third
term that can see the difference.

**Architecture:** The slot-to-rect map the walk needs already exists as the
committed buffer's `rect_index`. Hoisting its construction above the solve lets
every other scratch vector be keyed by rect index and sized at `order.len()`,
and lets a non-structural commit reuse the previous map instead of building one.

**Tech Stack:** Rust 2024, `dashscene-core`, `goldens/tooling` integration
tests, `cargo`/`nextest` via `just`.

**Spec:** `docs/archive/2026-08-16-commit-scratch-vectors-design.md`

**Gardened 2026-08-16.** Every task landed and every predicted number was
measured exactly: 69, 65, 45, 21, 17.

## Global constraints

- Workspace is `edition = "2024"`, `resolver = "3"`. **No new dependencies** —
  the counting allocator is written by hand against `std::alloc::System`.
- Prose in plain literal English. Markdown is formatted with `prim fmt` before
  every commit; `prim fmt --check` must exit 0. **Check the exit code without a
  pipe** — `prim ... | tail` reports `tail`'s status, which reads as success.
- Commit scopes come from `.git-std.toml`: `docs(docs)`, `feat(core)` /
  `fix(core)` for `crates/dashscene-core`, `test(goldens)` for
  `goldens/tooling/`.
- `just test` between edits; `just build` before the pull request. The diff
  touches no path in the `packer` filter, so `just calibrate` is not required by
  path — it is still owed at slice close.
- The band binary runs under `cargo test` in CI
  (`.github/workflows/ci.yml:992`), which runs tests as parallel **threads in
  one process**. Any counter the allocator writes must therefore be
  thread-local, not a global atomic.
- Never write a closing keyword (`fix`, `close`, `resolve`, in any inflection)
  next to an issue number in a commit message or the pull request body, except
  the single intended `Closes #944` on its own line at the end of the pull
  request body. Write `Refs #N` everywhere else.

## The measured ladder

Every task below moves the band's third term by a predicted amount. The
prediction is the check: if a step does not move the number by exactly its
predicted bytes, the attribution in the spec is wrong and the step stops rather
than having its constant adjusted to fit.

    task   what it bounds                       B/extra root after
    1      the term itself                                     69
    2      rect_of_slot (reuse the buffer map)                 65
    3      solved and the carry-forward loop                   45
    4      the six walk vectors and carried_out                21
    5      dfs_order's reserve                                 17

---

### Task 1: the band's third term

**Files:**

- Modify: `goldens/tooling/tests/per_frame_scaling.rs`

**Interfaces:**

- Produces: `fn bytes_of_one_commit(...) -> u64` measured through a
  `#[global_allocator]`; `FrameCost` gains a `bytes: u64` field;
  `within_band(paint, layout)` gains a third breach clause;
  `BYTES_PER_EXTRA_ROOT: u64 = 69`.

The term is landed **before** the fix, reading the pre-change number, because a
band added in the same change that improves what it measures cannot show what
the change was worth. That is this file's own stated rule and it is why story
#836 landed before story #838.

- [ ] **Step 1: write the failing guard assertion**

In `the_confinement_is_what_makes_the_number_one`, extend the existing breach
check to require the byte term to move as well:

```rust
assert!(
    breach.contains("layout frame ran")
        && breach.contains("rect rows against")
        && breach.contains("bytes per extra root"),
    "all three terms must move when the confinement goes, not just two; it \
     reported: {breach}"
);
```

- [ ] **Step 2: run it and watch it fail**

Run:
`cargo test -p goldens --test per_frame_scaling the_confinement -- --nocapture`
Expected: FAIL — the reported breach names two terms, not three.

- [ ] **Step 3: add the counting allocator**

At the top of the file, after the module documentation. A const-initialised
thread-local `Cell<u64>` allocates nothing and registers no destructor, so it is
safe to touch from inside `alloc`:

```rust
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    /// Bytes this thread requested while [`COUNTING`] was on. Const-initialised
    /// so reading it from inside the allocator cannot itself allocate.
    static BYTES: Cell<u64> = const { Cell::new(0) };
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}

/// The system allocator, counting request sizes on the measuring thread only.
///
/// Thread-local rather than a global atomic because CI runs this binary with
/// `cargo test`, which runs tests as threads in one process
/// (`.github/workflows/ci.yml`); a global counter would be the sum of whatever
/// else happened to be running. Under nextest, which gives each test its own
/// process, the two are equivalent.
struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.get() {
            BYTES.set(BYTES.get() + layout.size() as u64);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNTING.get() {
            BYTES.set(BYTES.get() + new_size.saturating_sub(layout.size()) as u64);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;
```

- [ ] **Step 4: measure the commit**

`FrameCost` gains `bytes: u64`. In `frame`, wrap only `commit_with` — the `open`
and `set_prop` before it are the producer's, not the frame's:

```rust
fn frame(loaded: &mut Loaded, solver: &mut TaffySolver<'_>, prop: Prop) -> FrameCost {
    let before = solver.solves();
    let mut txn = loaded.arena.open();
    txn.set_prop(loaded.shown, prop);
    BYTES.set(0);
    COUNTING.set(true);
    txn.commit_with(solver);
    COUNTING.set(false);
    FrameCost {
        solves: solver.solves() - before,
        rect_rows: loaded.arena.committed().rects().len(),
        bytes: BYTES.get(),
    }
}
```

`FrameCost` derives `PartialEq`/`Eq`; adding a `u64` keeps both.

- [ ] **Step 5: state the term as a slope**

The band's other two terms are levels because they are counts of work. Bytes are
not: a level would move on any dependency bump that added a fixed allocation.
The slope over two document sizes cancels every fixed cost, so only a
document-scaled allocation can move it.

```rust
/// Bytes one steady-state **layout** frame allocates per root beyond the first,
/// over the many-root document — the third term, and the one that can see what
/// a commit's per-node scratch costs.
///
/// **69 until this story; 17 after it.** The two terms above are counts of
/// work and are stated as levels; this one is stated as a slope, taken between
/// the one-root and the many-root documents, because the level also holds every
/// fixed allocation a frame makes and would move whenever one of those changed.
/// The slope cancels them and leaves exactly what grows with the document.
///
/// **The layout frame, not the paint-only one.** The paint-only frame's byte
/// count moves across repeats over one document — 884, 884, 1284, 1172 on the
/// small one — because the paint table's own interning and pooled-entry
/// compaction move with it, and neither is a document-scaled cost. The layout
/// frame repeats bit-identically.
///
/// **An allocation count cannot see this at all**, which is worth recording
/// because issue #944 offers one as an alternative: a steady-state layout frame
/// makes exactly 21 allocation calls over the one-root, seventeen-root and
/// sixty-five-root documents alike. Only the sizes differ.
const BYTES_PER_EXTRA_ROOT: u64 = 69;
```

- [ ] **Step 6: add the third clause to the band**

`within_band` takes the small document's layout frame and the many document's
root count so it can take the slope. Update its two other callers to match.

```rust
fn within_band(
    paint: FrameCost,
    layout: FrameCost,
    small_layout: FrameCost,
    extra_roots: u64,
) -> Result<(), String> {
    // ... the two existing terms, unchanged ...
    let slope = (layout.bytes - small_layout.bytes) / extra_roots;
    if slope != BYTES_PER_EXTRA_ROOT {
        breaches.push(format!(
            "a layout frame allocated {slope} bytes per extra root against \
             {BYTES_PER_EXTRA_ROOT} ({} bytes over {extra_roots} roots beyond the \
             one-root document's {})",
            layout.bytes - small_layout.bytes,
            small_layout.bytes,
        ));
    }
```

Guard the subtraction: if `layout.bytes < small_layout.bytes` the slope is
negative and the `u64` subtraction panics, which is a worse report than a
breach. Use `saturating_sub` and let the zero read as a breach.

`report` gains the byte figure beside the solves and rect rows it already
prints, and the criterion's ratio line gains the slope, so the CI log this step
writes carries all three terms rather than two.

- [ ] **Step 7: run the whole binary**

Run: `cargo test -p goldens --test per_frame_scaling -- --nocapture` Expected:
PASS, all three tests. The criterion prints 69 bytes per extra root; the
confinement guard now names three terms.

- [ ] **Step 8: record the before-number in the module documentation**

Extend the two tables under "The measurement, on macos aarch64" and "Before and
after" with the byte column, stating 69 as the measurement taken before the
commit changed and marking the after as pending in this same branch.

- [ ] **Step 9: commit**

```bash
prim fmt goldens/tooling/tests/per_frame_scaling.rs
git add goldens/tooling/tests/per_frame_scaling.rs
git commit -m "test(goldens): give the per-frame band a bytes-per-extra-root term

Refs #944."
```

---

### Task 2: reuse the committed slot-to-rect map

**Files:**

- Modify: `crates/dashscene-core/src/arena.rs` — `Txn::commit_with`

**Interfaces:**

- Produces: `rect_of_slot: Arc<Vec<u32>>` built once above the solve and moved
  into the committed buffer at the end, replacing both the walk's own
  `vec![NO_RECT; n]` and the buffer's separate `rect_index` construction.

- [ ] **Step 1: hoist `structural` above the solve**

`structural` is computed today after the solve, from `order.len()`,
`previous.node_ids.len()` and `renumbered`. All three are known before it. Move
the `let structural = ...` binding (and the `let previous = &arena.buffers[...]`
it needs) to just below `let order = arena.dfs_order();`, keeping its comment.

- [ ] **Step 2: build the map there**

```rust
// The slot-to-rect map, built once and shared. On a structural commit
// this is the vector the buffer needs anyway, so building it here
// rather than at the end costs nothing and gives the walk the map it
// needs to key its scratch by rect index instead of by arena slot
// (issue #944). On a non-structural commit the previous commit's map
// still describes this DFS order — which is the premise the buffer
// below already relies on when it shares it by reference — so there is
// nothing to build at all.
//
// `NO_RECT` rather than zero, for the reason D4 of
// `docs/decisions/the-runtime-paints-the-shown-root.md` gives: a zero
// default is a *valid* rect index — row 0, the shown root's own rect —
// so every slot the walk does not reach would answer with the shown
// root's geometry instead of admitting it has none (issue #980).
let rect_of_slot: Arc<Vec<u32>> = if structural {
    let mut map = vec![NO_RECT; arena.nodes.len()];
    for (i, &id) in order.iter().enumerate() {
        // In range for u32 by the add_node guard.
        map[id.index()] = i as u32;
    }
    Arc::new(map)
} else {
    Arc::clone(&previous.rect_index)
};
```

- [ ] **Step 3: delete the walk's own map**

Remove `let mut rect_of_slot: Vec<u32> = vec![NO_RECT; n];` and the
`rect_of_slot[id.index()] = i as u32;` assignment at the top of the walk. Every
read site (`rect_of_slot[parent.index()]` in the `subtree_end` pass, and both
`rect_of_slot_checked` calls) already takes a slice, so `&rect_of_slot` works
unchanged.

- [ ] **Step 4: hand the same map to the buffer**

Replace the `if structural { ... }` block that builds `rect_index` at the end
with a reuse of the value already in hand:

```rust
let (node_ids, rect_index) = if structural {
    (Arc::new(order), Arc::clone(&rect_of_slot))
} else {
    (Arc::clone(&previous.node_ids), Arc::clone(&rect_of_slot))
};
```

Both arms now name `rect_of_slot`, so the binding collapses to
`let rect_index = Arc::clone(&rect_of_slot);` with `node_ids` keeping its
conditional. Keep the comment explaining that the maps change only on a
structural change.

Note `order` is moved into `Arc::new(order)` in the structural arm, so the walk
must be finished with it — it is; the `subtree_end` and group-opacity passes are
above this point.

- [ ] **Step 5: run the core tests**

Run: `cargo nextest run -p dashscene-core` Expected: PASS. This step changes no
committed value; it changes where one vector is built.

- [ ] **Step 6: re-measure and move the constant**

Run: `cargo test -p goldens --test per_frame_scaling -- --nocapture` Expected:
FAIL, reporting **65** bytes per extra root against 69 — exactly the 4 bytes
`rect_of_slot`'s `u32` per slot was costing. Move `BYTES_PER_EXTRA_ROOT` to 65
and re-run to PASS.

**If it reports anything other than 65, stop.** The spec's attribution is wrong
and the remaining tasks' predictions cannot be trusted.

- [ ] **Step 7: commit**

```bash
git add crates/dashscene-core/src/arena.rs goldens/tooling/tests/per_frame_scaling.rs
git commit -m "perf(core): build the commit's slot-to-rect map once

Refs #944."
```

---

### Task 3: key `solved` by rect index

**Files:**

- Modify: `crates/dashscene-core/src/arena.rs` — `Txn::commit_with`

- [ ] **Step 1: size it by the walk and map the solver's output**

```rust
// Geometry from the solver, keyed by rect index — the walk's own
// index, so this is one entry per node the commit draws rather than
// one per node the document holds (issue #944). Malformed solver
// output is a broken contract, named loudly (P4): duplicates and
// foreign ids never commit silently.
let mut solved: Vec<Option<SolvedRect>> = vec![None; order.len()];
for (id, rect) in solver.solve(arena) {
    let &slot = rect_of_slot.get(id.index()).unwrap_or_else(|| {
        panic!("solver returned a rect for {id:?}, which is not a node of this arena")
    });
    // A node outside the shown root's subtree. The solver is handed
    // the whole arena, so reporting one is not an error — this commit
    // simply resolves no rect for it (story #838).
    if slot == NO_RECT {
        continue;
    }
    assert!(
        solved[slot as usize].replace(rect).is_none(),
        "solver returned two rects for {id:?}"
    );
}
```

- [ ] **Step 2: bound the carry-forward loop**

It runs once per node in the document today. Walk `order` instead:

```rust
// Carry forward the previous commit's rect for every node the solver
// did not report — by NodeId, so a structural change that shifted the
// DFS index still finds the right previous rect. A node that is
// neither solved now nor present in a previous commit stays `None` and
// trips the invariant below.
{
    let previous = &arena.buffers[arena.front];
    for (i, &id) in order.iter().enumerate() {
        if solved[i].is_none()
            && let Some(&ri) = previous.rect_index.get(id.index())
            && ri != NO_RECT
        {
            let r = previous.rects[ri as usize];
            solved[i] = Some(SolvedRect { x: r.x, y: r.y, w: r.w, h: r.h });
        }
    }
}
```

- [ ] **Step 3: read it by rect index in the walk**

`let geometry = solved[id.index()]` becomes `let geometry = solved[i]`, where
`i` is the walk's enumerate index. The panic message is unchanged.

- [ ] **Step 4: run the core tests**

Run: `cargo nextest run -p dashscene-core` Expected: PASS.

- [ ] **Step 5: re-measure and move the constant**

Run: `cargo test -p goldens --test per_frame_scaling -- --nocapture` Expected:
FAIL at **45** against 65 — 20 bytes for `Option<SolvedRect>` per slot. Move the
constant to 45; re-run to PASS. Anything else: stop.

- [ ] **Step 6: commit**

```bash
git add crates/dashscene-core/src/arena.rs goldens/tooling/tests/per_frame_scaling.rs
git commit -m "perf(core): key the commit's solved rects by rect index

Refs #944."
```

---

### Task 4: key the walk's cascade vectors by rect index

**Files:**

- Modify: `crates/dashscene-core/src/arena.rs` — `Txn::commit_with`,
  `parent_region_out`, and that helper's two unit tests

**Interfaces:**

- Produces:
  `parent_region_out(region_out: &[Option<ClipIndex>], pi: usize,
  parent: NodeId) -> ClipIndex`
  — the lookup moves to a rect index, the `NodeId` stays for the panic message.

- [ ] **Step 1: size the six vectors and `carried_out` by the walk**

`region_out_index`, `region_out_changed`, `mask_region`, `mask_changed`,
`eff_hidden`, `hidden_changed` and `carried_out` change from `n` to
`order.len()`. Keep every existing comment; each one explains a rule that has
not changed, and add to the `mask_region` comment that the key is now the
parent's rect index rather than its slot.

- [ ] **Step 2: resolve the parent once per node**

At the top of the walk body, beside `let node = &arena.nodes[id.index()];`:

```rust
// The parent's rect index, resolved once. A node in `order` has
// its parent in `order` too — the walk covers whole subtrees — or
// is a root and has none, and the DFS reaches the parent first, so
// every read below lands on an entry this walk already wrote.
let parent_i = node.parent.map(|p| rect_of_slot[p.index()] as usize);
```

- [ ] **Step 3: rewrite the reads**

Each `parent.index()` becomes `pi`, and each `id.index()` on these seven vectors
becomes `i`:

```rust
let (region_in_index, region_in_changed) = match parent_i {
    Some(pi) => {
        let parent_out = parent_region_out(&region_out_index, pi, node.parent.unwrap());
        let changed = region_out_changed[pi] || mask_changed[pi];
        match mask_region[pi] {
            Some(masked) => (masked, changed),
            None => (parent_out, changed),
        }
    }
    None => (ClipIndex::UNCLIPPED, false),
};
```

`node.parent.unwrap()` inside the arm is unpleasant; bind
`match (node.parent, parent_i) { (Some(parent), Some(pi)) => ... }` instead so
neither is unwrapped.

The visibility pair, keeping the same semantics:

```rust
let parent_hidden = parent_i.is_some_and(|pi| eff_hidden[pi]);
let parent_hidden_changed = parent_i.is_some_and(|pi| hidden_changed[pi]);
eff_hidden[i] = parent_hidden || !node_visible;
hidden_changed[i] = parent_hidden_changed || visible_toggled_set.contains(&id.index());
```

`visible_toggled_set`, `paint_dirty_set`, `clip_toggled_set` and
`mask_toggled_set` stay keyed by slot. They are hash sets sized by the dirty
set, not by the document, so they are not part of this issue.

The mask write site keeps writing into the **parent's** entry:
`mask_region[pi] = ...` and `mask_changed[pi] = true`.

The group-opacity pass:

```rust
for (i, &id) in order.iter().enumerate() {
    let node = &arena.nodes[id.index()];
    let base = match node.parent {
        Some(parent) => carried_out[rect_of_slot[parent.index()] as usize],
        None => 1.0,
    };
    // ...
    carried_out[i] = 1.0;   // and carried_out[i] = alpha in the other arm
```

- [ ] **Step 4: change `parent_region_out` and its tests**

```rust
fn parent_region_out(region_out: &[Option<ClipIndex>], pi: usize, parent: NodeId) -> ClipIndex {
    region_out[pi].unwrap_or_else(|| {
        panic!(
            "commit reached a child of {parent:?} before {parent:?} itself: the clip cascade \
             requires parent-before-child document order (P4)"
        )
    })
}
```

Its two unit tests pass a `NodeId` today. They become:

```rust
    #[test]
    fn parent_region_out_returns_a_resolved_parents_region() {
        let region_out = vec![Some(ClipIndex(7)), None];
        assert_eq!(parent_region_out(&region_out, 0, NodeId(0)), ClipIndex(7));
    }

    #[test]
    #[should_panic(expected = "requires parent-before-child document order")]
    fn parent_region_out_panics_when_the_parent_is_unresolved() {
        // What a child-before-parent traversal would hand the cascade: the
        // parent's entry still unset. Reading `UNCLIPPED` there instead would
        // mis-clip the whole subtree in silence.
        let region_out = vec![None, Some(ClipIndex(1))];
        let _ = parent_region_out(&region_out, 0, NodeId(0));
    }
```

- [ ] **Step 5: run the core tests**

Run:
`cargo nextest run -p dashscene-core && cargo nextest run -p dashscene-core --test arena`
Expected: PASS. The mask, clip, visibility and group-opacity tests are the
behaviour-preserving check for this task — the walk's interior is re-keyed and
its committed output must not move.

- [ ] **Step 6: re-measure and move the constant**

Run: `cargo test -p goldens --test per_frame_scaling -- --nocapture` Expected:
FAIL at **21** against 45 — 8 + 8 for the two `Option<ClipIndex>` vectors, 4 for
the four `bool` vectors, 4 for `carried_out`. Move the constant to 21; re-run to
PASS. Anything else: stop.

- [ ] **Step 7: commit**

```bash
git add crates/dashscene-core/src/arena.rs goldens/tooling/tests/per_frame_scaling.rs
git commit -m "perf(core): key the commit's clip, mask and opacity cascade by rect index

Refs #944."
```

---

### Task 5: stop `dfs_order` reserving the document

**Files:**

- Modify: `crates/dashscene-core/src/arena.rs` — `Arena::dfs_order`

- [ ] **Step 1: reserve nothing**

```rust
fn dfs_order(&self) -> Vec<NodeId> {
    // No `with_capacity(self.nodes.len())`: since story #838 this walk
    // covers the shown roots' subtrees, so reserving the document is the
    // one allocation left that scales with what is not drawn (issue #944).
    // Growth reallocates a handful of times over a bounded walk.
    let mut order = Vec::new();
```

- [ ] **Step 2: run the core tests**

Run: `cargo nextest run -p dashscene-core` Expected: PASS.

- [ ] **Step 3: re-measure**

Run: `cargo test -p goldens --test per_frame_scaling -- --nocapture` Expected:
FAIL at **17** against 21. Move the constant to 17; re-run to PASS.

17 is the residue in `dashscene-engine` and is where this story stops. Anything
other than 17 means the spec's attribution of the remaining bytes is wrong.

- [ ] **Step 4: commit**

```bash
git add crates/dashscene-core/src/arena.rs goldens/tooling/tests/per_frame_scaling.rs
git commit -m "perf(core): stop dfs_order reserving the whole document

Refs #944."
```

---

### Task 6: the narrowed solver contract, tested

**Files:**

- Modify: `crates/dashscene-core/tests/arena.rs`

The spec accepts one behaviour narrowing: a rect returned for a node outside the
shown subtree is skipped rather than stored. That path had no test before,
because before story #838 it could not happen.

- [ ] **Step 1: write the test**

A solver that reports every root, over an arena showing one of two. `GridSolver`
at `crates/dashscene-core/tests/arena.rs:695` already walks `arena.roots()`, so
this follows a shape the file has rather than inventing one.

```rust
/// A solver reporting a node outside the shown root's subtree is not an error:
/// it is handed the whole arena, and since story #838 the commit resolves rects
/// only for the shown root. The extra rect is skipped and the commit succeeds.
///
/// Untestable before that story, because every root was drawn. It is pinned now
/// because issue #944 keys the commit's `solved` scratch by rect index, so the
/// skip is what a node with no rect index resolves to — and the alternative,
/// row 0, is the shown root's own rect (issue #980).
#[test]
fn a_rect_for_an_unshown_root_is_skipped_rather_than_committed() {
    struct EveryRoot;
    impl LayoutSolver for EveryRoot {
        fn solve(&mut self, arena: &Arena) -> Vec<(dashscene_core::NodeId, SolvedRect)> {
            arena
                .roots()
                .iter()
                .map(|&id| {
                    (
                        id,
                        SolvedRect {
                            x: 1.0,
                            y: 2.0,
                            w: 3.0,
                            h: 4.0,
                        },
                    )
                })
                .collect()
        }
    }

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let first = txn.add_node(None, None);
    let _second = txn.add_node(None, None);
    txn.show_root(Some(first));
    txn.commit_with(&mut EveryRoot);

    assert_eq!(
        arena.committed().rects().len(),
        1,
        "the commit draws the shown root only, so the second root's rect is \
         skipped rather than committed or panicked on"
    );
}
```

Confirm `Txn::add_node`'s signature against the file before writing it — the
call above is copied from `crates/dashscene-core/tests/arena.rs:761`, and the
binding it returns is what `show_root` is given.

- [ ] **Step 2: run it**

Run: `cargo nextest run -p dashscene-core --test arena a_rect_for_an_unshown`
Expected: PASS on the new code. Verify it is a real test by reverting Task 3's
`if slot == NO_RECT { continue; }` to a panic and watching it fail, then
restoring.

- [ ] **Step 3: commit**

```bash
git add crates/dashscene-core/tests/arena.rs
git commit -m "test(core): pin that a rect for an unshown root is skipped

Refs #944."
```

---

### Task 7: records, debt, and the gate

**Files:**

- Modify: `goldens/tooling/tests/per_frame_scaling.rs` — the before-and-after
  tables
- Create: a durable record gardened from the spec
- Move: both `docs/wip/2026-08-16-commit-scratch-vectors-*.md` to
  `docs/archive/`

- [ ] **Step 1: complete the band's own before-and-after**

The module documentation's tables gain the byte column with both numbers, 69 and
17, and the 17 is attributed by name to `baseline_pass`'s `cross_offset`,
`incremental`'s `on_path` and `state.roots.clone()` in `dashscene-engine`, with
the debt issue's number from Step 2.

- [ ] **Step 2: file the residue as milestoned debt**

```bash
gh issue create --label debt --milestone "v0.19 — Android, the C ABI, and layer 0" \
  --title "The engine's per-frame scratch still scales with the document" \
  --body "..."
```

The body names the three allocations, the measured 17 bytes per extra root, and
the band term that now reports it. Write `Refs #944`, never a closing keyword.

- [ ] **Step 3: check what the branch's prose falsified**

`docs/technotes/engineering-guardrails.md` is named by issue #944 as reading
more broadly than the band supports. Re-read it against the third term and
correct it. Then:

```bash
grep -rn "#944" docs/ crates/ goldens/
grep -rn "per-frame" docs/decisions/the-shown-root-bounds-the-load-not-the-paint.md
```

- [ ] **Step 4: garden**

Write the durable record, move both `docs/wip/` files to `docs/archive/`, and
re-point any record citing their old paths — **one commit**. `docs/wip/` returns
to the eleven files it held, so `docs/wip/README.md`'s count is unchanged and
must not be edited.

- [ ] **Step 5: the gate**

```bash
just build 2>&1 | tail -40
```

Quote the `Summary` line. Then `prim fmt --check` on every Markdown file
touched, checking the exit code without a pipe.

- [ ] **Step 6: the closing-keyword sweep, before opening the pull request**

```bash
git log origin/main..HEAD --format=%B | grep -oiE "(fix|close|resolve)[a-z]* #[0-9]+"
```

Expected: no output. The single `Closes #944` goes in the pull request body
only, on its own line at the end.
