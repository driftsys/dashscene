# Single-Instance Sorted Commands Implementation Plan

    status  plan, executed 2026-09-03 (commits 686ef1f..137a18e). Working
            memory: archived verbatim once the durable records it points at
            (Task 5) had landed.
    note    Task 2 Step 2's nextest filter, `test(/single_instance/)`,
            selects nothing: nextest's `test()` predicate matches only a
            test function's own name, and none of the three functions in
            single_instance_commands.rs contain the substring
            "single_instance". The filter actually run was
            `binary(single_instance_commands)`. Not corrected in place —
            noted here for a reader re-running the step.

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every BRG draw command carrying `HasSortingPosition` names exactly one
visible instance, which eliminates the single-frame command-drop defect (issue
#1401).

**Architecture:** The emission loop in `BrgPainter.OnPerformCulling` stops
walking material runs and emits one command per instance; the command count
becomes the instance count. Key construction is untouched. Verification is a
red-first package-gate scan, an extended `f32` model case, and a before/after
20,000-frame soak recorded in the pull-request body.

**Tech Stack:** C# (Unity 6000.3.23f1, compiled by no CI job — gated by
`unity/package-gate` text scans in Rust), cargo-nextest, the RT-instrument probe
from the evidence shelf.

**Spec:** `docs/wip/2026-09-03-single-instance-sorted-commands-design.md` (this
directory).

## Global Constraints

- Worktree:
  `<worktrees>/dashscene-worktrees/single-instance-commands`,
  branch `debt/v021-single-instance-commands`. Absolute paths and `git -C`
  always; the shell cwd resets between commands.
- No document-format change, no C ABI change, no shader change (spec, Scope).
- `just test` between edits; `just build` before pushing (AGENTS.md).
- Prose in plain, literal English; commit messages in the repo's
  `fix(unity): lowercase sentence` style.
- Do not write `Closes #1401` anywhere: the PR targets
  `story/v021-showcase-host-parity`, not the default branch, so a closing
  keyword would not fire there — and a stray one in a later `main` PR body would
  fire wrongly. The issue is closed by hand after verification.
- The evidence shelf is
  `<worktrees>/dashscene-v021-lanes/probe-1401/`
  (outside the repository). Soak numbers land there and in the PR body, never
  hand-copied into repo prose without their derivation.

---

### Task 1: The before-soak — the defect measured on this branch's base

**Files:**

- Modify (uncommitted, reverted in Task 3):
  `unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneShowcase.cs`
- Output: `…/probe-1401/2026-09-03-arms/fixlane-before.log` (shelf)

**Interfaces:**

- Produces: the "before" band-frame count for the PR body, derived on this exact
  branch base rather than cited from another worktree's run.

- [ ] **Step 1: Apply the showcase probe hunks (only) from the shelf patch**

```bash
git -C <worktrees>/dashscene-worktrees/single-instance-commands \
  apply --include='unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneShowcase.cs' \
  <worktrees>/dashscene-v021-lanes/probe-1401/arms-uncommitted-2026-09-03.patch
```

Expected: clean apply; `git status` shows only that one modified file. The
painter is NOT patched — the probe's painter-side env arms do not exist in this
build, which is intended.

- [ ] **Step 2: Build the showcase player**

```bash
rm -rf <worktrees>/dashscene-worktrees/single-instance-commands/target/unity-demo
just -f <worktrees>/dashscene-worktrees/single-instance-commands/justfile \
     -d <worktrees>/dashscene-worktrees/single-instance-commands \
     unity-demo 6000.3.23f1 build
```

Expected: `[demo-build] build Succeeded, 0 error(s)`. Takes minutes; do not edit
files in the worktree while it runs.

- [ ] **Step 3: Run the 20,000-frame soak on the typography entry**

```bash
env DASHSCENE_PROBE1401_MEASURE=1 DASHSCENE_PROBE1401_ENTRY=1 \
    DASHSCENE_PROBE1401_QUIT=20000 DASHSCENE_PROBE1401_RT=1 \
    <worktrees>/dashscene-worktrees/single-instance-commands/target/unity-demo/Build/DashsceneShowcase.app/Contents/MacOS/DashsceneShowcase \
    -logFile <worktrees>/dashscene-v021-lanes/probe-1401/2026-09-03-arms/fixlane-before.log
```

A window opens and closes itself after ~5.5 minutes.

- [ ] **Step 4: Count band-frames and check the instrument's liveness**

```bash
L=<worktrees>/dashscene-v021-lanes/probe-1401/2026-09-03-arms/fixlane-before.log
grep -m1 "probe1401] BASELINE" "$L"
grep "DROP" "$L" | awk -F'frame=' '{split($2,a," "); print a[1]}' | sort -n | uniq -c | awk '$1>=4' | wc -l
grep "frame cost" "$L" | tail -2
```

Expected: BASELINE shows distinct non-uniform cell means (a uniform line means
the instrument is blind — stop, do not proceed on blind numbers); band-frame
count in the low hundreds (the shelf's prior runs: 292–317); keep the frame-cost
lines for Task 5. Record the count in the shelf's `RESULTS.md` under a "fix
lane" heading, with the exact grep above.

### Task 2: The gate goes red, the emission changes, the gate goes green

**Files:**

- Create: `unity/package-gate/tests/single_instance_commands.rs`
- Modify: `unity/com.driftsys.dashscene/Runtime/Engine/BrgPainter.cs` (emission
  loop in `OnPerformCulling`; delete `RunEnd` and `CommandsInBatch`; the
  counting line in `Draw`; the paint-order comment block; the
  `InstancesPerBatch` remark)

**Interfaces:**

- Consumes: `package_gate::{painter_source, PAINTER_PATH}`,
  `package_gate::cs_scan::member_body` (existing).
- Produces: the invariant later tasks and records rely on — one visible instance
  per flagged command, command count equals instance count.

- [ ] **Step 1: Write the failing gate**

`unity/package-gate/tests/single_instance_commands.rs`:

```rust
//! Every draw command that carries `HasSortingPosition` names exactly one
//! visible instance.
//!
//! **Why.** Issue #1401: Unity's sorted-transparent path was measured
//! dropping a contiguous subset of draw commands for single frames when a
//! flagged command carried more than one visible instance — ~300
//! dropped-band frames per 20,000 as built, 0 per 60,000 with one instance
//! per command, on macOS/Metal, 6000.3.23f1. Unity documents no
//! restriction; its own GPU Resident Drawer feeds this path
//! `visibleCount = 1` commands, and that shape is the only one measured
//! safe. `docs/technotes/batch-renderer-group.md` carries the tables.
//!
//! Text is weak: this pins that the painter still emits the safe shape,
//! never that Unity honours it — the same trade every scan beside this one
//! makes, stated in `sorting_keys.rs`.

use package_gate::cs_scan::member_body;
use package_gate::painter_source;
use package_gate::PAINTER_PATH as PAINTER;

const CULLING: &str = "private unsafe JobHandle OnPerformCulling(";

#[test]
fn every_flagged_draw_command_names_one_visible_instance() {
    let source = painter_source();
    let (s, e) = member_body(&source, CULLING);
    let culling = &source[s..=e];
    let (ls, le) = member_body(culling, "for (var at = first;");
    let emission = &culling[ls..=le];
    assert!(
        emission.contains("var end = at + 1;"),
        "{PAINTER}'s emission loop no longer fixes every run's end at one \
         instance. A HasSortingPosition command carrying more than one \
         visible instance is the configuration issue #1401 measured \
         dropping single frames."
    );
}

#[test]
fn no_material_run_walk_survives_anywhere_in_the_painter() {
    let source = painter_source();
    assert!(
        !source.contains("InstanceAtlas[end]"),
        "{PAINTER} walks a material run again (the `InstanceAtlas[end]` \
         comparison). Splitting emission at material boundaries built \
         multi-instance flagged commands, which is issue #1401's trigger; \
         one instance per command needs no walk."
    );
}

#[test]
fn the_command_count_is_the_instance_count() {
    let source = painter_source();
    let (s, e) = member_body(&source, "public void Draw(FrameLease lease)");
    let draw = &source[s..=e];
    assert!(
        draw.contains("_commandCount += InstancesInBatch(b);"),
        "{PAINTER}'s Draw no longer counts one command per instance. The \
         cached count and the emission loop must agree exactly — Unity \
         allocates the command array from the count and the loop writes \
         into it."
    );
}
```

- [ ] **Step 2: Run it, confirm all three fail for the stated reasons**

```bash
cargo nextest run --manifest-path <worktrees>/dashscene-worktrees/single-instance-commands/Cargo.toml -p package-gate -E 'test(/single_instance/)'
```

Expected: 3 FAIL — the loop calls `RunEnd(at, limit)` (not `at + 1`), `RunEnd`
contains `InstanceAtlas[end]`, and `Draw` sums `CommandsInBatch`. If any PASSES
here, the assertion is not pinning what it claims — stop and fix the test, not
the painter.

- [ ] **Step 3: Change the emission**

In `BrgPainter.cs`, in `OnPerformCulling`'s emission loop, replace:

```csharp
for (var at = first; at < limit && command < commandCount;)
{
    var end = RunEnd(at, limit);
    var run = end - at;
```

with:

```csharp
for (var at = first; at < limit && command < commandCount;)
{
    // **One visible instance per command — issue #1401.**
    // Unity's sorted-transparent path was measured dropping
    // a contiguous subset of draw commands for single
    // frames when a HasSortingPosition command carried more
    // than one instance; one instance per command is the
    // shape Unity's own GPU Resident Drawer feeds it, and
    // the only one measured safe. Tables:
    // docs/technotes/batch-renderer-group.md §5d.
    var end = at + 1;
    var run = end - at;
```

Delete the whole `RunEnd` method and the whole `CommandsInBatch` method. In
`Draw`, replace `_commandCount += CommandsInBatch(b);` with
`_commandCount += InstancesInBatch(b);` and adjust its comment to say the count
is one command per instance. In the paint-order comment block above the
`instanceSortingPositions` allocation, add one sentence: a flagged command
carries exactly one visible instance, per issue #1401, with the measurements in
the technote. In `InstancesPerBatch`, correct the closing remark that the
`Math.Min` "keeps one batch to one command" — it now keeps a batch's capacity
bounded, and a batch holds one command per instance.
`MaxInstancesPerDrawCommand` stays (that `Math.Min` still uses it).

- [ ] **Step 4: Run the gate and the whole package-gate suite**

```bash
cargo nextest run --manifest-path <worktrees>/dashscene-worktrees/single-instance-commands/Cargo.toml -p package-gate
```

Expected: the three new tests PASS and every existing scan stays green. A
pre-existing scan failing here means it pinned the old shape — read its message
and adjust that scan's bound, in this commit, with a sentence in its comment
saying why.

- [ ] **Step 5: Mutation — restore the walk, watch the gate catch it**

Temporarily re-add `var end = Math.Min(limit, at + 2);` in place of
`var end = at + 1;`, rerun the three tests, confirm the first fails. Revert the
mutation. (This is the required mutation: the gate must fail when the safe shape
is reverted, not merely pass beside it.)

- [ ] **Step 6: Sanity tier, then commit**

```bash
just -f <worktrees>/dashscene-worktrees/single-instance-commands/justfile \
     -d <worktrees>/dashscene-worktrees/single-instance-commands test
git -C <worktrees>/dashscene-worktrees/single-instance-commands \
  add unity/package-gate/tests/single_instance_commands.rs \
      unity/com.driftsys.dashscene/Runtime/Engine/BrgPainter.cs
git -C <worktrees>/dashscene-worktrees/single-instance-commands \
  commit -m "fix(unity): one visible instance per sorted draw command

Unity's sorted-transparent path drops a contiguous subset of draw
commands for single frames when a HasSortingPosition command carries
more than one visible instance — measured at 292-317 dropped-band
frames per 20,000 on macOS/Metal as built, 0 per 60,000 with one
instance per command, 0 per 40,000 with the flag removed, and 115 per
20,000 with the host's whole per-frame path stopped, which exonerates
every per-frame call this package makes. Unity documents no
restriction; visibleCount = 1 is the shape its own GPU Resident Drawer
feeds the path. Issue #1401.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

Note: the showcase probe file stays uncommitted — `git add` names files
explicitly here for exactly that reason.

### Task 3: The after-soak, and the probe leaves

**Files:**

- Reverted at the end:
  `unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneShowcase.cs`
- Output: `…/probe-1401/2026-09-03-arms/fixlane-after.log` (shelf)

**Interfaces:**

- Consumes: Task 2's committed painter; Task 1's still-applied probe.
- Produces: the "after" band-frame count (expected 0) and the after frame-cost
  lines for Task 5.

- [ ] **Step 1: Rebuild and soak, same commands as Task 1 Steps 2–3**, with the
      log at `…/fixlane-after.log`.

- [ ] **Step 2: Count band-frames**, same derivation as Task 1 Step 4.

Expected: BASELINE live, band-frames **0**. Any nonzero count: STOP — the fix
does not hold on this branch; return to analysis rather than proceeding, and say
so.

- [ ] **Step 3: Record both counts** in the shelf `RESULTS.md` fix-lane heading
      (before N, after 0, both greps quoted), and keep the two logs.

- [ ] **Step 4: Drop the probe file**

```bash
git -C <worktrees>/dashscene-worktrees/single-instance-commands \
  checkout -- unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneShowcase.cs
git -C <worktrees>/dashscene-worktrees/single-instance-commands status --short
```

Expected: clean tree (the named file only — never `checkout -- .`).

### Task 4: The arithmetic model covers instance-scale command counts

**Files:**

- Modify: `unity/package-gate/tests/sorting_key_arithmetic.rs`

**Interfaces:**

- Consumes: the file's own `step`, `keys`, `distinct`, `from_camera` helpers
  (already defined there).
- Produces: the floor's distinctness property pinned at the command counts the
  split now produces.

- [ ] **Step 1: Add the test**

```rust
/// One command per instance (issue #1401) raises the command count to the
/// instance count, so the floor must keep keys distinct at that scale too.
/// 10,000 commands at the review's hostile placement — a document 10,000
/// units from the origin, a camera half a unit away — spans 1,000 units
/// behind the sheet and every key must stay its own float, farthest first.
#[test]
fn the_floor_keeps_instance_scale_command_counts_distinct() {
    let (distance, base, commands) = (0.5f32, 10_000.0f32, 10_000);
    let s = step(distance, base);
    let k = keys(base, s, commands);
    assert_eq!(distinct(&k), commands as usize);
    for c in 1..commands as usize {
        assert!(
            from_camera(k[c], base, distance) < from_camera(k[c - 1], base, distance),
            "distance from the camera must fall with the command index; \
             it does not between commands {} and {}",
            c - 1,
            c
        );
    }
}
```

- [ ] **Step 2: Run it**

```bash
cargo nextest run --manifest-path <worktrees>/dashscene-worktrees/single-instance-commands/Cargo.toml -p package-gate -E 'test(/instance_scale/)'
```

Expected: PASS (D4's behind-the-sheet layout has no fold at any span). If it
FAILS, that is a real finding about the floor at scale — stop and bring it back
to the spec rather than loosening the assertion.

- [ ] **Step 3: Commit**

```bash
git -C <worktrees>/dashscene-worktrees/single-instance-commands \
  add unity/package-gate/tests/sorting_key_arithmetic.rs
git -C <worktrees>/dashscene-worktrees/single-instance-commands \
  commit -m "test(unity): the key floor holds at instance-scale command counts

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

### Task 5: Records, in place

**Files:**

- Modify: `docs/decisions/brg-draw-command-order-is-not-guaranteed.md`
- Modify: `docs/technotes/batch-renderer-group.md`
- Modify: `docs/design/unity-csharp-host.md`
- Modify: `unity/com.driftsys.dashscene/CHANGELOG.md`
- Check only: `docs/specification/07-embedding-and-distribution.md` (R-E22)

**Interfaces:**

- Consumes: Task 1/3 soak numbers and their frame-cost lines.
- Produces: the single-instance rule as a recorded constraint, cited by the code
  comment Task 2 wrote.

- [ ] **Step 1: Decision record** — add a D5 to
      `brg-draw-command-order-is-not-guaranteed.md`: a draw command carrying
      `HasSortingPosition` names exactly one visible instance; the measured
      basis (the four rates, dated, macOS/Metal, 6000.3.23f1); "What is still
      owed" gains the order re-measurement under the new shape (story 2) and
      loses nothing else. Status line notes the 2026-09-03 amendment.
- [ ] **Step 2: Technote** — add §5d with the band-defect tables (before
      ~300/20k and after 0/20k from Tasks 1/3, the freeze arm's 115/20k and the
      split/keysoff zeros from the shelf's RESULTS.md, each with its
      derivation); §7 gains the single-instance rule as a fourth pitfall. Do not
      rewrite §4/§5b — the order-semantics question stays open until story 2
      re-measures it.
- [ ] **Step 3: Design record** — in `unity-csharp-host.md`, correct the
      command-shape paragraph: one command per instance, why, and the R-T4
      frame-cost numbers from the soak logs (before/after `frame cost` lines,
      quoted with the entry name).
- [ ] **Step 4: CHANGELOG** — one entry under the unreleased heading, in the
      file's existing style.
- [ ] **Step 5: R-E22 check** — read its status line; this lane does not build
      the pixel gate, so it stays "not met" unless the wording claims something
      this change falsifies. Edit only if wrong.
- [ ] **Step 6: Prose gates and commit**

```bash
just -f <worktrees>/dashscene-worktrees/single-instance-commands/justfile \
     -d <worktrees>/dashscene-worktrees/single-instance-commands lint
git -C <worktrees>/dashscene-worktrees/single-instance-commands add -A docs unity/com.driftsys.dashscene/CHANGELOG.md
git -C <worktrees>/dashscene-worktrees/single-instance-commands \
  commit -m "docs(unity): the single-instance command rule, and the measurements behind it

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

### Task 6: File the follow-ups

**Files:** none (GitHub issues).

- [ ] **Step 1:** File three issues, each read back after creation
      (`gh issue view <n>` — never trust `gh issue create`'s output under `||`):
      story 2 (order semantics under the new shape: §5b re-run, sandwich
      fixture, order gate, step-negation mutation must flip the composite), the
      Android/Vulkan soak (one-device rule), and the lit-class command-shape
      refinement (debt). A fourth for the Unity upstream micro-repro.
      Milestones: story 2 and the device soak on v0.21; the refinement and
      upstream repro where the owner places them — ask in the PR body rather
      than guessing.

### Task 7: Garden, ship

- [ ] **Step 1:** Invoke the `sdd-gardening` skill to archive this plan and the
      spec beside it into `docs/archive/` (they are this branch's working
      memory; the durable content already landed in Task 5's records).
- [ ] **Step 2:** `just build` green in the worktree; quote its Summary line.
- [ ] **Step 3:** Push; open the PR **ready** (never draft), base
      `story/v021-showcase-host-parity`, body carrying: the defect, the evidence
      table, both soak derivations, the R-T4 numbers, the mutation result, and
      the follow-up issue numbers. No closing keyword (see Global Constraints).
- [ ] **Step 4:** Run the full multi-seat review per the **shipping-a-change**
      skill; a disposition against every finding; merge per that skill; close
      #1401 by hand with a comment linking the PR and the mechanism; remove the
      worktree per stage 8.

## Self-review

- Spec coverage: requirement → Task 2; verification (gate, model, soak, R-T4) →
  Tasks 1–4; records → Task 5; follow-ups → Task 6. No gap found.
- Placeholders: none; every code step carries its content.
- Type consistency: `InstancesInBatch(b)` (existing method) is what Task 2's
  test asserts and Task 2's change writes; helper names in Task 4 match the
  file's existing `step`/`keys`/`distinct`/`from_camera`.
