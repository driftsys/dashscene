# The commit's per-node scratch, and a band term that can see it

    status   WIP — spec for issue #944 (story-sized `debt`, milestone
             v0.19), written 2026-08-16 against `origin/main` at
             `4faeeda2`. Every number below was measured on the day with
             a counting global allocator over the band's own fixture, not
             recalled and not read out of the issue. Gardened when the
             work lands. **GARDENED 2026-08-16**, in the pull request
             that closed #944: the measurement rule is now
             docs/decisions/per-frame-allocation-is-measured-as-a-slope.md
             and the as-built commit interior is the "Scratch keyed by
             rect row" bullet in docs/design/dashscene-core-arena.md.
    issue    #944. Refs #838, #836, #980.

## What is wrong

`Txn::commit_with` in `crates/dashscene-core/src/arena.rs` sizes its per-node
scratch by the node count of the **whole document**, while story #838 confined
the solve, the committed table and the paint to the shown root. A frame that
draws one artboard out of sixty-five still allocates sixty-five entries in every
scratch vector, and the loop that carries forward the previous commit's rects
still runs once per node in the document.

Issue #944 names eight such vectors. **There are nine, and a tenth allocation
sits one function away.**

- Named by the issue: `solved`, `region_out_index`, `region_out_changed`,
  `mask_region`, `mask_changed`, `eff_hidden`, `hidden_changed`, `rect_of_slot`.
- **Not named by the issue: `carried_out`** — `vec![1.0; n]`, the group-opacity
  alpha carried down the tree. It is slot-keyed and document-sized exactly like
  the other eight, and it is in the same block.
- **Not named by the issue, and outside the block:** `Arena::dfs_order` builds
  its result with `Vec::with_capacity(self.nodes.len())`, so the order vector
  reserves the document even though it holds only the shown roots' subtrees.

The ninth vector in that block, `painted_extent`, is **already** bounded —
`vec![None; order.len()]`, converted under issue #980 for a different reason.
That line is the in-tree precedent this work extends rather than invents.

## What it costs, measured

A counting global allocator around one `commit_with`, over
`goldens/tooling/tests/common/many_root.rs`'s document at three sizes. macOS
aarch64, debug. The steady-state **layout** frame, repeated four times, is
bit-stable:

    document   roots   bytes/commit   alloc calls
    small          1            289            21
    mid           17           1393            21
    many          65           4705            21

    slope   69 bytes per extra root, exactly linear
    ratio   16.28x many over small

Two findings decide the shape of the band term below.

**An allocation count cannot see this.** The call count is 21 on all three
documents. Issue #944 offers "an allocation count or a bytes-touched count"; the
measurement rules the first one out.

**The paint-only frame is not a usable site.** Its byte count moves across
repeats over the same document — 884, 884, 1284, 1172 on the small one — because
the paint table's own interning and compaction move with it, which is not a
document-scaled cost. The layout frame is exact and is where the term is stated.

### Where the 69 bytes go

Attributed by backtrace capture on each allocation, not inferred from reading:

    52 B/node   dashscene-core
                  solved                20   (Option<SolvedRect>)
                  region_out_index       8   (Option<ClipIndex>)
                  mask_region            8
                  four bool vectors      4   (region_out_changed, mask_changed,
                                              eff_hidden, hidden_changed)
                  rect_of_slot           4
                  carried_out            4
                  dfs_order's reserve    4

    17 B/node   dashscene-engine
                  baseline_pass's cross_offset   8   (per node)
                  incremental's on_path          1   (per node)
                  state.roots.clone()            8   (per root; lib.rs:1036,
                                                      whose comment calls the
                                                      roots list small)

52 and 17 sum to the measured 69, so the attribution is complete rather than
partial. **This story fixes the 52.** The 17 is a second crate, and
`crates/dashscene-engine/src/lib.rs` is a known collision point for parallel
stories; it becomes its own milestoned `debt` issue, named in the band's own
prose so the residue is explained rather than merely left.

## What changes

### The slot-to-rect map becomes a value computed once, above the solve

This is the crux, and it is what makes the rest fall out. Every scratch vector
is keyed by `NodeId` slot today, so bounding them means keying them by something
else — and the something else is the rect index, which is what `rect_of_slot`
already maps slots to. That map is needed before it can be built, which is why
the issue calls this a change to the commit's whole interior.

It is not, because **the map already exists**: the committed buffer carries
`rect_index`, built at the end of `commit_with` on a structural commit and
`Arc::clone`d from the previous buffer otherwise. Hoisting that construction
above the solve gives the walk the map it needs:

    let structural = order.len() != previous.node_ids.len() || renumbered;
    let rect_of_slot: Arc<Vec<u32>> = if structural {
        let mut m = vec![NO_RECT; arena.nodes.len()];
        for (i, &id) in order.iter().enumerate() { m[id.index()] = i as u32; }
        Arc::new(m)
    } else {
        Arc::clone(&previous.rect_index)
    };

- On a **structural** commit this is the same vector the buffer builds today.
  Built once and shared, so a structural commit allocates one document-sized
  vector **fewer** than it does now.
- On a **steady-state** commit it allocates nothing at all.
- `structural` depends only on `order.len()`, `previous.node_ids.len()` and
  `renumbered`, all of which are known before `solver.solve` runs, so hoisting
  it changes no value.

Reusing the previous commit's map rests on the premise that a non-structural
commit walks the same DFS order as the previous one. That premise is **already
shipped**, not introduced here: the existing code `Arc::clone`s that same map
into the new buffer under the same condition. It holds because the arena has no
reparenting API — `NodeData::parent` is written at `add_node` and never
reassigned — so equal node count plus an unchanged shown root gives an identical
order.

### The nine vectors

- `solved` becomes `vec![None; order.len()]`, keyed by rect index. Each id the
  solver returns is mapped through `rect_of_slot`: out of range keeps the
  existing "not a node of this arena" panic, and `NO_RECT` means a node outside
  the shown subtree and is skipped.
- The carry-forward loop iterates `order` rather than every slot, reading the
  previous rect through `previous.rect_index[slot]` as it does today.
- `region_out_index`, `region_out_changed`, `mask_region`, `mask_changed`,
  `eff_hidden`, `hidden_changed` and `carried_out` size at `order.len()` and key
  by rect index. Every `parent.index()` read becomes
  `rect_of_slot[parent.index()]`. This is sound because a node in `order` has
  its parent in `order` or is a root and has no parent, and the DFS visits the
  parent first, so the entry is written before it is read.
- `rect_of_slot` is the hoisted map. `rect_of_slot_checked` already takes
  `&[u32]`, so it is indifferent to whether the map is owned or shared.
- `Arena::dfs_order` drops its `with_capacity(self.nodes.len())`.

### One behaviour narrowing, stated rather than buried

A solver returning **two** rects for a node **outside** the shown subtree stops
panicking: both fail the map lookup and are skipped, where today both are stored
and the second trips the duplicate assertion. Duplicates for shown nodes still
panic, and a foreign id still panics.

This trims a P4 diagnostic. It is accepted rather than worked around because
restoring it needs a document-sized structure, which is the cost this story
exists to remove, and because story #838 already made "the solver reported a
node this commit does not draw" an ordinary case rather than an error.

## The band's third term

`goldens/tooling/tests/per_frame_scaling.rs` measures Taffy layout computations
and committed rect rows. Neither moves when these allocations do, so the band
cannot falsify this work as it stands. A change here with no term that can see
it should not merge.

- **Bytes, not allocations**, for the measured reason above.
- **On the layout frame only**, for the measured reason above, with that reason
  recorded in the file rather than left as a choice a reader has to reconstruct.
- **Stated as a slope, not a level**:
  `(many_bytes - small_bytes) / extra_roots`, asserted as an exact integer
  equality, with the ratio derived and printed beside it. The slope cancels
  every fixed cost, so a Taffy or `std` change that adds a constant allocation
  leaves the band alone and only a document-scaled one moves it. A level would
  churn on every dependency bump.
- **`BYTES_PER_EXTRA_ROOT`: 69 today, 17 after**, the 17 attributed by name to
  the three `dashscene-engine` allocations and to the issue that carries them.
- **The instrument lives in `per_frame_scaling.rs` itself**, never in
  `goldens/tooling/tests/common/`, which eighteen test binaries compile (issue
  #932). The counter is a const-initialised `thread_local!` `Cell<u64>`, which
  is correct under `cargo test`'s in-process parallel threads as well as under
  nextest's process-per-test — and CI runs this binary under `cargo test`.
- **The sensitivity guard is already written.**
  `the_confinement_is_what_makes_the_number_one` clears the shown root, which
  returns `order.len()` to the document's size and the slope to its pre-change
  value. The third term inherits the same committed upward injection the other
  two terms have, which is what lets the term and the fix land in one change
  without the term being unfalsifiable. `within_band` grows a third breach
  clause so the guard's assertion can name it.

The file's own rule — that a band added in the same change that improves what it
measures cannot fail — is honoured by that guard and by the ordering below,
which lands the term first and records it failing at 69.

## Verification

1. Add the term. Run it: it must **fail**, reporting 69 bytes per extra root.
   That failure is the before-number, and it is recorded in the file's own
   before-and-after table.
2. Change `dashscene-core`. The term reads 17 and passes.
3. `just test` between edits; `just build` before the pull request, with its
   `Summary` line quoted rather than a claim that it passed.
4. The existing `dashscene-core` tests for masks, clips, visibility and group
   opacity are the behaviour-preserving check — the walk's interior is re-keyed
   and its output must not move.
5. One new test in `dashscene-core`: a solver returning a rect for a node
   outside the shown subtree is ignored, and the commit succeeds.

## Alternatives considered

- **Retain the scratch on the `Arena` across commits** — issue #944's second
  option. Rejected. It keeps slot keying, so the diff is smaller, but it retains
  document-sized memory for the arena's life, still clears O(n) per commit so a
  bytes-touched term would not move, and adds mutable state that the
  `InternerGuard` take-and-put dance would have to cover.
- **Bound only the six cheap vectors**, leaving `solved` and `rect_of_slot`
  slot-keyed. Rejected: it removes 12 of the 69 bytes per node and does not
  close the issue.
- **Widen the story to `dashscene-engine` so the term reaches zero.** Rejected
  for this story. It would let the third term read 1.00x like the other two,
  which is the better band — but it is a second crate, outside the issue's
  subject, and in a file parallel stories are known to collide on. The residue
  is filed instead.
- **An allocation count as the term.** Ruled out by measurement, not by
  preference: the count is 21 on every document size.
- **An absolute byte level as the term.** Rejected: it churns on any dependency
  bump that adds a fixed allocation, where the slope does not.

## Out of scope

- The 17 bytes per node in `dashscene-engine`. One milestoned `debt` issue,
  filed as part of this story, naming `cross_offset`, `on_path` and
  `state.roots.clone()`.
- The committed buffer's own `rect_index`, which stays document-sized. It is the
  slot-to-rect map itself and is required output, and it is allocated only on a
  structural commit — this change makes a structural commit build it once rather
  than twice.
- `docs/wip/README.md` says the gate "reports ten" while its own heading says
  eleven and eleven files are tracked. Pre-existing, unrelated to this branch,
  and reported rather than edited here.
