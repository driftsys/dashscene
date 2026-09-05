# The Unity painter beside a faithful Canvas — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task by task. Steps use
> checkbox (`- [ ]`) syntax for tracking. **In this repository every task after
> Task 0 is one GitHub story worked in its own worktree on its own branch**,
> through the `implementing-a-change` and `shipping-a-change` skills; this plan
> is the epic's plan and each task is that story's plan.

**Goal:** the Unity painter, measured beside a faithful uGUI Canvas of the same
scenes on the Pixel 5, presents at or below the Canvas's frame-ready cadence on
every showcase scene and costs less CPU per presented frame, at rest and during
a transition.

**Architecture:** one player carries both renderers and an empty floor; the
compositor reads GPU, a new thread-time instrument reads CPU. The painter's host
stops working when nothing changed and touches only dirty rows when something
did; its two blended classes leave `BatchRendererGroup` for in-order procedural
draws; the shared WGSL gains a plain-fill path, a baked gradient strip and
per-document specialisation; and an occlusion pass on the shared Rust side hands
both painters only the visible pieces of each rect.

**Tech Stack:** C# under Unity 6000.3.23f1 with URP and `BatchRendererGroup`;
Rust workspace (edition 2024, resolver 3) with `wgpu`, `naga`, `bytemuck`; WGSL
single-sourced to HLSL through `naga`; `just` recipes; `dumpsys SurfaceFlinger`
and `/proc/<pid>/stat` over `adb` on a Pixel 5 (`11181FDD4002MY`, Android 14,
Adreno 620).

**Spec:**
[`2026-09-05-unity-painter-beside-a-faithful-canvas.md`](2026-09-05-unity-painter-beside-a-faithful-canvas.md)
in this directory. The plan argues from it; executors read both.

## Global constraints

Each line points at its owner; nothing here restates a rule.

- **Device, mode, instruments, criterion, fairness:** the spec's §2 (D1), §3
  (D2) and §4 (D3), which story #1442 turns into the decision record every
  reading cites. In one line: the Pixel 5 at 1080x2340 in the 60 Hz mode; GPU at
  or below the Canvas with no tolerance above; CPU per presented frame lower,
  and main-thread cost above the floor lower.
- **One device is one lane:** the `project-gates` skill; readings batch between
  code stories.
- **Territory:** a file that story #1412's open branch `story/v021-opaque-cores`
  also touches is coordinated per file with that lane — read
  `git diff --stat origin/main...origin/story/v021-opaque-cores` when a story
  opens, since that set moves; `DashsceneShowcase.cs` and
  `DashsceneFrameCost.cs` are track O's. New behaviour goes in new files where
  the spec says so, and a Unity-free class goes in `Runtime/`, beside
  `CommitPacer.cs`, where `package-compat` and `ffi-check` compile it by glob
  and `runtime_split.rs` refuses it under `Runtime/Engine/`.
- **Principles and rules:** `docs/specification/02-principles.md` P1–P5;
  `docs/specification/03-target-hardware-rules.md` R-T4 and R-T5;
  `docs/specification/07-embedding-and-distribution.md` R-E17 and R-E20.
- **Gates and tiers:** the `project-gates` skill; name the tier in the PR body.
- **Test first; mutate where the `implementing-a-change` skill says a mutation
  is required.**
- **Prose, commits, PRs:** `AGENTS.md`, the `shipping-a-change` skill and the
  user's rules. The commit commands below carry the trailer literally so they
  run as written.

## How this plan is organised

Ten stories under one gating epic on milestone
`v0.21 — Unity and Android on target hardware`. Task 0 files them. Tasks 1 to 10
are the stories, in dependency order:

        1 record ──── 2 canvas ─────────────────────────────┐
    3 instrument ─┬─ 4 settle ─ 5 dirty ─ 9 occlusion ────┤
                  ├─ 6 per-command ─ 7 in-order draws ────┼─ 10 table
                  └─ 8 fast paths + specialisation ───────┘

Tasks 1 and 3 start on the current tree with no device; Task 2 starts against
the spec's §3 and cites Task 1's record when it lands. Tasks 3, 5, 6, 7, 8, 9
and 10 each end in a device reading, batched between code stories; the code
before each reading needs none. Which tasks coordinate with story #1412's branch
is the territory rule above: read its diff when the task opens.

**The tree moves under this plan.** Every symbol below was read from
`origin/main` at `789103c` on 2026-09-05, and the three maps behind it are in
this session's record. Two merges landed after the maps and before this plan's
base: PR #1431 (`5d472b4`) changed `unity/demo/DemoBuild.cs` and the `justfile`,
added `Samples~/Showcase/DashsceneFramePacing.cs` and
`package_gate::cs_files_under`; PR #1433 (`9ad9874`) changed
`Runtime/Engine/BrgPainter.cs`, `unity/render-gate/DashsceneRenderGate.cs` and
the package gate's tests, and closed issue #1402. Tasks 2, 3, 4 and 7 read those
files at the base before their first edit. A story that opens later re-reads
each file it names before its first edit, the way a driver prompt is written
from the tree; a symbol that has moved is corrected in the story's plan section,
not worked around silently.

**Each task's shape** is the repository's: worktree + branch, the failing test
first, the light review after each behaviour change, `docs/wip/` gardened before
the PR, the multi-seat review, findings dispositioned, merge through the queue,
worktree removed.

    git -C /Users/sebastientasson/Workspace/driftsys/dashscene-staging fetch origin
    git -C /Users/sebastientasson/Workspace/driftsys/dashscene-staging worktree add \
      -b story/v021-<slug> \
      /Users/sebastientasson/Workspace/driftsys/dashscene-worktrees/v021-<slug> origin/main
    cd /Users/sebastientasson/Workspace/driftsys/dashscene-worktrees/v021-<slug> && ./bootstrap

## File structure

| story         | creates                                                                                                                                                                                                                                                                               | modifies                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 record      | `docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`                                                                                                                                                                                                           | `docs/decisions/README.md` (index row), `docs/roadmap.md` (the epic line, if Task 0 did not)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| 2 canvas      | `Samples~/Showcase/DashsceneCanvasBaseline.cs`, `Samples~/Showcase/CanvasScene.cs`, `Samples~/Showcase/CanvasSprites.cs`, `Samples~/Showcase/SpriteBake.shader`, `unity/demo/DemoFonts.cs`, `unity/package-gate/tests/canvas_baseline.rs`                                             | `justfile` (`unity-demo`, `unity-demo-android`: manifest gains `com.unity.ugui`, font copied under `Assets/Fonts/`, a `DemoFonts.Create` editor step, a `compare` action)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| 3 instrument  | `Runtime/ThreadCostMath.cs`, `Runtime/ThreadCostAccumulator.cs` (both Unity-free), `Runtime/Engine/DashsceneThreadCost.cs`, `Samples~/Showcase/DashsceneThreadCostReporter.cs`, `measure/android/fixtures/unity-frame-cost.log`, `unity/package-gate/tests/thread_cost_instrument.rs` | `measure/android/unity-frame-cost.sh` (captures the thread-cost line beside the frame-cost line), `measure/android/frame-table.py` (the two `[showcase]` line regexes and a `unity-showcase` provenance entry — the one parser), `unity/ffi-check/Program.cs` (the arithmetic and accumulator tests), `unity/demo/DemoBuild.cs` (`CreatePipeline`: five URP fields set explicitly), `unity/render-gate/DashsceneRenderGate.cs` (reads one thread-cost line), `Samples~/Showcase/DashsceneShowcase.cs` (two accessors, `CurrentIndex` and `DrawnFrames`, and `DashsceneFrameCost.cs` calling `ThreadCostMath` — both coordinated with track O)                                                                                                                                                                                                                                                                                                                             |
| 4 settle      | `Runtime/SettleLoop.cs` (Unity-free), `unity/package-gate/tests/settle_path.rs`                                                                                                                                                                                                       | `Samples~/FrameLoop/DashsceneFrameLoop.cs` and `Samples~/Showcase/DashsceneShowcase.cs` (both loops decide through `SettleLoop`; the showcase edit is coordinated with track O), `Runtime/Engine/BrgPainter.cs` (`BindHeap` on change — reallocation, atlases, or the scalars it binds — and `HeapBindCount`), `unity/render-gate/DashsceneRenderGate.cs` (draws-fewer-than-frames, the picture held across a skip, the bind count on a resize, zero-alloc), `unity/ffi-check/Program.cs` (the settle decision)                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 5 dirty       | `Runtime/InstanceSpans.cs`, `Runtime/StreamLayout.cs` (both Unity-free), `unity/package-gate/tests/dirty_range_upload.rs`                                                                                                                                                             | `Runtime/FramePacker.cs` (span bookkeeping, in-place rewrite of instances; the heap repacked whole), `Runtime/Engine/BrgPainter.cs` (`UploadInstances` ranges per batch and stream, `LastUpload`), `unity/ffi-check/Program.cs` (the packer's ranges, the stream layout, the four coalescing cases ported from `dirty_ranges`'s tests), `unity/render-gate/DashsceneRenderGate.cs` (the uploaded row count against the dirty spans)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| 6 per-command | `driftsys/dashscene-v021-lanes/probe-1406/` (outside the repo, the reading)                                                                                                                                                                                                           | `docs/design/android-toolchain.md` (the reading), `docs/design/unity-csharp-host.md` (the gaps list)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| 7 in-order    | `Runtime/Engine/OrderedDrawFeature.cs`, `Runtime/Engine/OrderedDrawPass.cs`                                                                                                                                                                                                           | `Runtime/Engine/BrgPainter.cs` (class routing, `CutRuns`), `Runtime/Shaders/DashsceneInstance.hlsl` (`SV_InstanceID` path), `Runtime/Resources/Dashscene/UnlitOverlay.shader` and `Text.shader` (a `DASHSCENE_PROCEDURAL` keyword), `unity/demo/DemoBuild.cs` (adds the feature to the renderer data), `unity/render-gate/RenderGateBuild.cs` (same), `docs/decisions/unity-painter-uses-brg.md` (D1, D3), `docs/decisions/brg-draw-command-order-is-not-guaranteed.md` (D5 scope), `unity/package-gate/tests/single_instance_commands.rs` (scope)                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 8 fast paths  | `crates/dashpaint/src/gradient_strip.rs`, `crates/dashpaint/src/kind_set.rs`, `crates/dashscene-gpu/tests/kind_set.rs`                                                                                                                                                                | `crates/dashscene-gpu/src/shaders/paint.wgsl` (early-out, strip sample, `override` constants), `crates/dashscene-gpu/src/render.rs` (binding 11/12, the pipeline cache per kind set), `crates/dashscene-gpu/tests/layer2_conformance.rs` (its `reference_ramp` moves into `dashpaint` as the one CPU ramp), `crates/dashscene-core/src/committed.rs` (holds the kind set and the strip, both per commit), `crates/dashscene-ffi/src/lib.rs` (`ds_runtime_kind_set`, `ds_runtime_gradient_strip`), `Runtime/Native.cs` and `Runtime/DashsceneRuntime.cs` (their imports and forwarders), `Runtime/Shaders/DashsceneInstance.hlsl` (strip sample, keywords), every `.shader` under `Runtime/Resources/Dashscene/` (`#pragma multi_compile` for the kind keywords), `Runtime/Engine/BrgPainter.cs` (binds the strip, re-selects keywords per frame), `unity/ffi-check/Program.cs`, `conformance/layer2-probes.json`, `docs/design/android-toolchain.md` (the per-kind sweep) |
| 9 occlusion   | `crates/dashscene-core/src/occlusion.rs`                                                                                                                                                                                                                                              | `crates/dashpaint/src/lib.rs` (the `Piece` row), `crates/dashscene-core/src/committed.rs` (`pieces`), `crates/dashscene-core/src/arena.rs` (`commit_with` calls the pass and dirties every rect whose pieces changed), `crates/dashpaint-abi/src/lib.rs` (`Piece` on the boundary-B surface), `crates/dashscene-ffi/src/lib.rs` (`DsFrame.pieces`, `DS_ABI_VERSION`), `Runtime/BoundaryB.cs` (the `Piece` mirror), `crates/dashscene-ffi/include/dashscene.h`, `crates/dashscene-ffi/tests/abi.c`, `Runtime/Native.cs`, `Runtime/FrameLease.cs` (`RowSizes`, and its "nineteen arrays" comment), `docs/specification/07-embedding-and-distribution.md` (R-E17's met-by clause counts the arrays), `Runtime/FramePacker.cs` (packs pieces), `crates/dashscene-gpu/src/render.rs` (packs pieces), `unity/ffi-check/Program.cs`, `crates/dashscene-gpu/tests/shaded_area.rs` (the #1296 instrument PR #1417 landed, extended to pieces)                                      |
| 10 table      | —                                                                                                                                                                                                                                                                                     | `docs/design/android-toolchain.md` (the comparison section), `docs/features.md`, `docs/roadmap.md` (the epic's close line), `docs/wip/README.md` (archives this plan and the spec)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |

---

### Task 0: File the epic and the ten stories — executed 2026-09-05

Filed from this worktree on 2026-09-05, in dependency order, each number read
back from GitHub before it was cited anywhere (`gh issue create` prints a URL
and has no `--json`, and a concurrent session can consume a guessed number):

| story | issue | title                                                                       |
| ----- | ----- | --------------------------------------------------------------------------- |
| S1    | #1442 | the criterion and the fairness rules are a record                           |
| S3    | #1443 | a thread-time instrument beside the frame-cost line, and the URP floor      |
| S2    | #1444 | the faithful Canvas entries in the demo player                              |
| S4    | #1445 | the Unity host idles when the tick reports no advance                       |
| S5    | #1446 | the packer rewrites dirty rows in place and uploads ranges                  |
| S6    | #1447 | the per-command term, read on the Pixel 5                                   |
| S7    | #1448 | the overlay and text classes draw in order through a render-graph pass      |
| S8    | #1449 | a plain-fill path, a baked gradient strip, and the document's kind set      |
| S9    | #1450 | an occlusion pass on the shared side hands both painters the visible pieces |
| S10   | #1451 | the comparison table, and the epic's close                                  |

Epic **#1441**, labelled `epic`, on milestone
`v0.21 — Unity and Android on
target hardware`, carries the story table, the
planned count (ten, also posted as a comment for the phase-end revision) and
`Refs` lines; every story is labelled `story` on the same milestone. **GitHub
holds the bodies and this plan does not copy them**: a copy drifts from an
amendment made on the issue, which is how this repository amends, and the spec's
§8 table is the one in-repo mirror of the story rows. The level-two sweep of
`docs/decisions/slices-are-planned-against-their-inflow.md` was run after
filing: every story is named by the epic, and #1441 itself is named by comments
on #1106, #1107 and #1120. The roadmap paragraph naming the third gating epic is
in this PR.

- [x] the epic and the ten stories created and read back
- [x] every cross-reference in every body in the `#N` form the sweep greps for
- [x] the sweep passes for the ten
- [x] the roadmap line and the ledger rows committed

---

### Task 1: The criterion and fairness record (story S1)

Worktree `v021-canvas-criterion-record`, branch
`story/v021-canvas-criterion-record`.

**Files:**

- Create:
  `docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`
- Modify: `docs/decisions/README.md` (one index row)

**Interfaces:**

- Produces: the record's D1 (criterion), D2 (fairness rules 1-8), D3
  (instruments and their definition against `shell.rs` and
  `DashsceneFrameCost.cs`). Every later reading cites `D1`/`D2`/`D3` by name.

- [ ] **Step 1: Find what test reads decision records, so the record's shape is
      what it expects**

Run:
`grep -rn "docs/decisions" --include=*.rs crates demo goldens unity/package-gate | grep -v "^.*//" | head`
Expected: the tests that parse a record (the D3 table parser, the doc-links
gate). Note their expected heading shapes; the new record uses the same status
block as `the-gpu-frame-on-the-target-device-is-budgeted.md`.

- [ ] **Step 2: Write the record**

Sections, in the house shape: status block (`status`, `date`, `source`, `scope`,
`related`), `## Context` (three paragraphs: the question, the assessment, the
four rulings — from the spec's §0), `## Decision` with:

- **D1 — the criterion.** The spec's §2 verbatim, with "**Met when**" for each
  of the three clauses naming the instrument and the window.
- **D2 — the fairness rules.** The spec's §3, rules 1-8, numbered.
- **D3 — the instruments, and their definition against the two that exist.** The
  spec's §4's GPU and CPU paragraphs and the definition paragraph.

`## Consequences` (binds the ten stories, not the slice; the budget record's D1
still binds; issue #549 stands), `## Alternatives considered` (the spec's §9,
all nine bullets).

Numbers: cite `docs/design/android-toolchain.md` for the shaded areas and PR
#1431's section for the cadence; cite
`driftsys/dashscene-v021-lanes/probe-1412/RESULTS.md` for the occluded
fractions. No number without a source line.

- [ ] **Step 3: D4 — the reinterpretation of R-T2, carried in the record until
      the pass lands**

The specification is as-built and the pass does not exist yet, so the paragraph
below is **not** written into `docs/specification/03-target-hardware-rules.md`
by this story: the record carries it as **D4**, and story 9 adds it beside R-T2
in the same PR as the pass, in the past tense with the reading beside it. The
text D4 holds and story 9 moves:

```markdown
R-T2's intent is that a pixel covered by a later opaque rect is never shaded.
Two mechanisms satisfy it, and a painter may take either or both: the
depth-tested opaque core the rule names, and an occlusion pass over the
resolved rect table before packing, which emits only the visible pieces of
each rect and needs no depth buffer
(`docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`).
The depth-tested form was measured on the Pixel 5 on 2026-09-05 and did not
shorten the frame (story #1412); the pass form is story #1450's.
```

- [ ] **Step 4: Index and gates**

Add the row to `docs/decisions/README.md` in the same shape as its neighbours.
Run:

```bash
just prim && just test
```

Expected: green. `just test` is the sanity tier and reads records; a heading the
parser expects and does not find fails here.

- [ ] **Step 5: Commit, review, PR**

```bash
git add docs && git commit -m "docs(decisions): the Unity painter is measured against a faithful Canvas" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

Then `shipping-a-change`: `just build`, PR ready, the multi-seat review,
dispositions, merge.

---

### Task 2: The faithful Canvas entries (story S2)

Worktree `v021-canvas-baseline`, branch `story/v021-canvas-baseline`. Depends on
Task 1's record being on `main`.

**Files:**

- Create:
  `unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneCanvasBaseline.cs`,
  `…/CanvasScene.cs`, `…/CanvasSprites.cs`, `unity/demo/DemoFonts.cs`,
  `unity/package-gate/tests/canvas_baseline.rs`
- Create also: `Samples~/Showcase/SpriteBake.shader`, `Runtime/FrameRows.cs`
  (`public static ReadOnlySpan<T> Of<T>(DsSlice slice) where T : unmanaged`, the
  package's one unsafe accessor a sample reads rows through)
- Modify: `justfile` (`unity-demo`, `unity-demo-android`: the manifest, the font
  copy, the `DemoFonts.Create` step, the `compare` action; append-only inside
  the two recipes)

**Interfaces:**

- Consumes: `DashsceneRuntime` (`Tick`, `AcquireFrame`, `LoadDocumentWithText`,
  `ReadAtlases`), Task 4's `SettleLoop` (the Canvas loop settles exactly as the
  painter's does; until Task 4 lands, the baseline carries the same skip inline
  and adopts the class when it exists), the demo producer's `DemoScenes.Name(i)`
  / scene build and pulse calls exactly as `DashsceneShowcase.ReadSceneTable`
  and `PulseIfShowingAScene` use them (read those two methods first; the
  baseline mirrors them), `FrameLease.Frame` (`Rects`, `PaintEntries`, `Solids`,
  `Gradients`, `GradientStops`, `Strokes`, `ClipRegions`, `ClipBoxes`,
  `GlyphRuns`, `Dirty`),
  `RectEntry`/`PaintEntry`/`Color`/`Gradient`/`GradientStop`/`Stroke`/`ClipBox`
  as declared in `Runtime/BoundaryB.cs`, the sheet matrix
  `BrgPainter.FillStaging` writes as the batch head (`WritePackedMatrix`) — the
  Canvas uses the same document-to-world mapping.
- Produces: `DashsceneCanvasBaseline : MonoBehaviour` with
  `public enum Renderer { Painter, Canvas, None }`,
  `public static Renderer FromArguments(string[] args)`;
  `CanvasScene.Build(DsFrame frame, Transform root, CanvasSprites sprites, TMP_FontAsset font) -> CanvasScene`
  with `void Apply(DsFrame frame)` (dirty rows only) and `int ElementCount`;
  `CanvasSprites.Get(CornerRadii corners, float strokeWidth, StrokeAlign align) -> Sprite`
  (cached by key); the log line
  `[showcase] drew <label>: <n> element(s) through the canvas` and, under the
  painter, the showcase's own line unchanged; the `compare` action's
  `report.txt` with `PASS`.

- [ ] **Step 1: The package-gate test that pins the per-frame shape (write it
      first)**

`unity/package-gate/tests/canvas_baseline.rs`:

```rust
use package_gate::{cs_scan, root};

fn showcase(name: &str) -> String {
    let path = root().join("unity/com.driftsys.dashscene/Samples~/Showcase").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn apply_walks_the_dirty_set_and_never_the_element_list() {
    let scanned = cs_scan::blank_comments_and_strings(&showcase("CanvasScene.cs"));
    let (s, e) = cs_scan::member_body(&scanned, "public void Apply(DsFrame frame)");
    let body = cs_scan::squeeze(&scanned[s..e]);
    assert!(body.contains("var dirty = FrameRows.Of<uint>(frame.Dirty);"), "the dirty set is the span the loop walks: {body}");
    assert!(body.contains("for (var d = 0; d < dirty.Length; d++)"), "the one loop is bounded by the dirty count: {body}");
    assert!(body.contains("var i = (int)dirty[d];"), "the element index comes from the dirty set: {body}");
    assert!(!scanned.contains("unsafe "), "a sample compiled into Assembly-CSharp cannot be unsafe; FrameRows is the seam");
    for forbidden in ["_elements.Count", "frame.Rects.Count", "foreach (", "ElementCount"] {
        assert!(!body.contains(forbidden), "Apply walks every element through `{forbidden}`: {body}");
    }
    assert_eq!(body.matches("for (").count(), 1, "exactly one loop in Apply");
}

#[test]
fn the_baseline_logs_the_drew_line_the_cycle_recipe_reads() {
    // Read raw and drop comment lines: `blank_comments_and_strings` would blank
    // the literal this test looks for, and a comment must not satisfy it.
    // `blank_comments_and_strings` blanks the literal, so blank comments only:
    // strip `//` lines and `/* … */` blocks, keep strings.
    let code = cs_scan::blank_comments_only(&showcase("DashsceneCanvasBaseline.cs"));   // a two-line helper this test adds to cs_scan
    let re = regex::Regex::new(r#"Debug\.Log\(\$"\[showcase\] drew \{[^}]+\}: \{[^}]+\} element\(s\) through the canvas"#).unwrap();
    assert!(re.is_match(&code), "the drew line, in the cycle recipe's shape, is logged from code");
}
```

The scan pins the shape; the count is pinned at run time in Step 6, where the
`compare` action asserts `CanvasScene.AppliedLastFrame` equals the dirty count.

Run: `cargo nextest run -p package-gate canvas_baseline` Expected: FAIL —
neither file exists.

- [ ] **Step 2: Sprites baked once at load, through the single-sourced distance
      — `CanvasSprites.cs`**

The rounded-box distance exists once, in
`crates/dashscene-gpu/src/shaders/sdf.wgsl`, and `just sdf-hlsl` compiles it to
`Runtime/Shaders/Sdf.hlsl` (R-T5). A third copy in C# would drift with no gate
watching, and it would be the copy the epic's own comparison is judged against.
So the sprite is baked **on the GPU through the generated include**: a small
sample shader, `Samples~/Showcase/SpriteBake.shader`, includes
`Packages/com.driftsys.dashscene/Runtime/Shaders/Sdf.hlsl`, takes the radii,
stroke width and alignment as material properties, and writes coverage to a
`RenderTexture`; `Graphics.Blit(null, rt, material)` once per distinct shape,
`ReadPixels` into a `Texture2D`, `Sprite.Create` with the 9-slice border.
Nothing here runs per frame.

```csharp
public sealed class CanvasSprites : System.IDisposable
{
    private readonly Material _bake = new Material(Shader.Find("Dashscene/Samples/SpriteBake"));
    private readonly Dictionary<Key, Sprite> _cache = new Dictionary<Key, Sprite>();
    private readonly struct Key { /* rounded radii, stroke*4, align, ring flag */ }

    public Sprite Get(CornerRadii corners, float strokeWidth, StrokeAlign align) => Cached(corners, strokeWidth, align, ring: false);
    public Sprite GetRing(CornerRadii corners, float strokeWidth, StrokeAlign align) => Cached(corners, strokeWidth, align, ring: true);

    private Sprite Cached(CornerRadii c, float w, StrokeAlign a, bool ring)
    {
        var key = new Key(c, w, a, ring);
        if (_cache.TryGetValue(key, out var sprite)) return sprite;
        sprite = Bake(c, w, a, ring); _cache[key] = sprite; return sprite;
    }

    private Sprite Bake(CornerRadii c, float strokeWidth, StrokeAlign align, bool ring)
    {
        var r = Mathf.Max(c.TopLeft, c.TopRight, c.BottomRight, c.BottomLeft);
        var outset = align == StrokeAlign.Outside ? strokeWidth : align == StrokeAlign.Center ? strokeWidth * 0.5f : 0f;
        var border = Mathf.CeilToInt(r + outset + 1f);      // radius + stroke + the one-pixel band
        var size = border * 2 + 1;                           // a one-pixel stretchable centre
        _bake.SetVector("_DsCorners", new Vector4(c.TopLeft, c.TopRight, c.BottomRight, c.BottomLeft));
        _bake.SetVector("_DsStroke", new Vector4(strokeWidth, (int)align, ring ? 1f : 0f, 1f /* aa */));
        var rt = RenderTexture.GetTemporary(size, size, 0, RenderTextureFormat.ARGB32, RenderTextureReadWrite.Linear);
        Graphics.Blit(null, rt, _bake);
        var tex = new Texture2D(size, size, TextureFormat.RGBA32, false, true);
        var was = RenderTexture.active; RenderTexture.active = rt;
        tex.ReadPixels(new Rect(0, 0, size, size), 0, 0); tex.Apply(false, true);
        RenderTexture.active = was; RenderTexture.ReleaseTemporary(rt);
        return Sprite.Create(tex, new Rect(0, 0, size, size), new Vector2(0.5f, 0.5f), 1f, 0,
            SpriteMeshType.FullRect, new Vector4(border, border, border, border));
    }
    public void Dispose() { /* destroy the textures and the material */ }
}
```

`SpriteBake.shader`'s fragment: `d = rounded_box_sdf(p, half, radii)` from the
include, `fill = coverage(d, aa)`,
`ring = stroke_coverage(d, width, align, aa)`, output white with alpha
`ring ? ring : max(fill, ring)`. The fill is white so `Image.color` tints it; a
rect whose stroke colour differs from its fill gets a second `Image` with the
ring sprite.

- [ ] **Step 3: The scene builder — `CanvasScene.cs`**

Skeleton with the two methods the gate reads:

```csharp
using System.Collections.Generic;
using TMPro;
using UnityEngine;
using UnityEngine.UI;

namespace Driftsys.Dashscene.Samples
{
    public sealed class CanvasScene
    {
        private readonly List<Element> _elements = new List<Element>();   // index = rect index
        private readonly Transform _root;
        private readonly CanvasSprites _sprites;
        private readonly TMP_FontAsset _font;
        public int ElementCount => _elements.Count;

        private sealed class Element { public RectTransform Rect; public Image Fill; public Image Ring; public TMP_Text Text; public uint BakedGradientRow; public GradientStop[] BakedStops; }

        public static CanvasScene Build(DsFrame frame, Transform root, CanvasSprites sprites, TMP_FontAsset font)
        {
            var scene = new CanvasScene(root, sprites, font);
            // FrameRows (Runtime/FrameRows.cs, this story) wraps a DsSlice as a ReadOnlySpan<T>
            // inside the package, whose asmdef allows unsafe code; a sample compiled into
            // Assembly-CSharp does not, so the sample stays safe.
            var rects = FrameRows.Of<RectEntry>(frame.Rects);
            var entries = FrameRows.Of<PaintEntry>(frame.PaintEntries);
            for (var i = 0; i < rects.Length; i++)
                scene._elements.Add(scene.Make(i, in rects[i], in entries[(int)rects[i].Paint], frame));   // RectEntry.Paint is the index itself, a uint
            scene.PlaceGlyphRuns(frame);
            return scene;
        }

        /// Per frame: the dirty rows only. A careful Unity developer diffing
        /// their own state would touch exactly these; a Canvas that rewrote
        /// every element would be the painter's advantage, and it is not taken.
        public void Apply(DsFrame frame)
        {
            var rects = FrameRows.Of<RectEntry>(frame.Rects);
            var entries = FrameRows.Of<PaintEntry>(frame.PaintEntries);
            var dirty = FrameRows.Of<uint>(frame.Dirty);
            AppliedLastFrame = dirty.Length;
            for (var d = 0; d < dirty.Length; d++)
            {
                var i = (int)dirty[d];
                Isolate(_elements[i]);                       // rule 8: a rect the pulses move gets its own child Canvas, once
                Place(_elements[i], in rects[i]);
                Tint(_elements[i], in entries[(int)rects[i].Paint], frame);
            }
        }
        // Make: an Image (sliced sprite from _sprites, Image.color = fill) or, for a
        // gradient, an Image with a baked Texture2D at node size; a second Image for
        // a stroke whose colour differs; Place: anchoredPosition/sizeDelta from the
        // rect through the sheet matrix; Tint: Image.color from the solid row; for a
    // gradient, a re-bake only when the row's stops differ from the ones the
    // Element cached at its last bake (an animated gradient re-bakes, as a real
    // Canvas would; the showcase animates none); PlaceGlyphRuns: one TMP_Text per
        // glyph run at the run's origin with the run's string and size.
    }
}
```

Fill in `Make`, `Place`, `Tint`, `PlaceGlyphRuns`,
`BakeGradient(Gradient, GradientStop*, int w, int h)`; a linear gradient whose
primary handle is axis-aligned bakes `256x1` and stretches, every other kind
bakes `w x h`. Refused constructs (`PaintEntry.Shadows`, `Blurs`, an `Image`
fill, `Shape`, and the render-target groups) are skipped and counted; the count
goes into the `drew` line.

Run: `cargo nextest run -p package-gate canvas_baseline` Expected: PASS.

- [ ] **Step 4: The component and the renderer switch —
      `DashsceneCanvasBaseline.cs`**

```csharp
[DefaultExecutionOrder(-100)]
public sealed class DashsceneCanvasBaseline : MonoBehaviour
{
    public enum Renderer { Painter, Canvas, None }
    public static Renderer FromArguments(string[] args)
    {
        for (var i = 0; i + 1 < args.Length; i++)
            if (args[i] == "-renderer")
                return args[i + 1] == "canvas" ? Renderer.Canvas : args[i + 1] == "none" ? Renderer.None : Renderer.Painter;
        return Renderer.Painter;
    }
    // Awake: read the argument; under Canvas or None, DestroyImmediate(GetComponent<DashsceneShowcase>())
    // — immediate, because Destroy is deferred past the current loop and the showcase's own
    // Awake would still construct a painter, draw one frame and log a drew line; this
    // component's execution order is earlier, so the showcase's Awake never runs.
    // Under Canvas: construct a DashsceneRuntime, load the same entry the showcase would
    // (mirror ReadSceneTable / the manifest), ReadAtlases is not needed; build a Canvas
    // (RenderMode.ScreenSpaceCamera, worldCamera = Camera.main, planeDistance 1) and
    // CanvasScene.Build (its `Isolate` moves a rect the first pulse dirties onto its own
    // child Canvas — rule 8 — so a rebuild covers the moving elements only); per Update:
    // pacer, `var advanced = _runtime.Tick(dt)`,
    // `if (!_settle.ShouldDraw(advanced)) return;` (the same decision as the painter's
    // loop — spec §3 rule 5 and §2's at-rest clause), AcquireFrame, _scene.Apply(frame),
    // frame.MarkDrawn(), the drew line once, then the two instruments in the phase the
    // showcase pushes them: DashsceneFrameCost.Push with drawTicks bracketing Apply,
    // and DashsceneThreadCost.Push(label, width, height). Key C: Destroy this runtime and canvas, AddComponent<DashsceneShowcase>()
    // (or the reverse). PageDown/PageUp walk entries the way the showcase does.
}
```

The baseline reuses `DashsceneFrameCost` (it is in the same sample directory) so
both renderers report the same line shape.

- [ ] **Step 5: The TMP font asset at build time — `DemoFonts.cs`, and the
      manifest**

`unity/demo/DemoFonts.cs`, an editor script the recipe runs as its own
`-executeMethod DemoFonts.Create` step before `DemoBuild.Build`, so
`DemoBuild.cs` — story 3's file — is not edited here:

```csharp
public static class DemoFonts
{
    public const string FontAssetPath = "Assets/Resources/DashsceneCascade.asset";
    public static void Create()
    {
        var failures = new List<string>();
        CreateTmpAsset(failures);
        if (failures.Count > 0) { Debug.LogError(string.Join("\n", failures)); EditorApplication.Exit(1); }
    }
    public static void CreateTmpAsset(List<string> failures)
    {
        var font = AssetDatabase.LoadAssetAtPath<Font>("Assets/Fonts/cascade.ttf");
        if (font == null) { failures.Add("Assets/Fonts/cascade.ttf did not import as a Font"); return; }
        var asset = TMP_FontAsset.CreateFontAsset(font, 64, 6, GlyphRenderMode.SDFAA, 1024, 1024);
        if (!AssetDatabase.IsValidFolder("Assets/Resources")) AssetDatabase.CreateFolder("Assets", "Resources");
        AssetDatabase.CreateAsset(asset, FontAssetPath);
        // the atlas texture and the material are in-memory sub-objects until added — TMP's
        // own creation menu adds both, and an asset saved without them loads with neither
        AssetDatabase.AddObjectToAsset(asset.atlasTextures[0], asset);
        AssetDatabase.AddObjectToAsset(asset.material, asset);
        AssetDatabase.SaveAssets();
    }
}
```

In the `justfile`'s `unity-demo` and `unity-demo-android` recipes: the written
`Packages/manifest.json` gains `"com.unity.ugui": "2.0.0"` (read the exact key
the editor's `BuiltInPackages` lists — `ls "$builtin" | grep ugui`), and the
cascade `.ttf` the recipe already copies into `StreamingAssets/cascade/` is also
copied to `Assets/Fonts/cascade.ttf`. Add a `compare` action beside
`run`/`build`/`cycle`: launches with `-renderer painter -capture <dir> -judge`,
then the player switches renderers itself per entry, captures both, and writes
`report.txt`.

- [ ] **Step 6: The comparison through the goldens' comparator, its band, and
      the run-time pins**

The comparator exists: `goldens/tooling/src/lib.rs`'s `compare_pngs` and the
`compare-images` binary, whose threshold is documented for exactly two painters'
captures on a device. So nothing is copied into the package: under `-judge`, the
player captures each entry through the painter and through the Canvas to PNG
files (the showcase's own capture path, `CaptureRequest` and `_capture` in
`DashsceneShowcase.cs`), and the `compare` action of the recipe runs
`cargo run -p goldens --bin compare-images` over each pair at a recorded
threshold and reads the fraction, the bounds and the max channel delta from its
JSON. Record the first run's per-entry fraction in the PR; pin the band as the
largest observed times 1.5, rounded up to a whole percent, in the recipe. The
band is a calibration, so the PR names the run it came from.

Two run-time pins in the same mode, the Canvas half of the spec's §4:

- **the dirty count** — during the pulse frame, `Fail` unless
  `_scene.AppliedLastFrame == frame.Dirty.CountAsLong`, and at rest unless the
  loop skipped the frame (an idle tick commits nothing, so the last commit's
  dirty rows stay on the frame — the skip, not an empty set, is the rest
  signal); and, rule 8's pin, `Fail` unless the `Canvas.BuildBatch` marker reads
  0 at rest and the rebuilt Canvas holds only isolated elements during the
  pulse;
- **zero allocation at rest** — sixty frames after the entry settles, in a run
  launched with `-no-frame-cost -no-thread-cost` (both instruments allocate a
  key string per push by design, and they are not the renderer under test),
  `GC.GetAllocatedBytesForCurrentThread()` before and after must be equal.

**Build once, run many.** `just unity-demo 6000.3.23f1 build` rebuilds the
project from scratch and costs tens of minutes; every run below launches the
built player with its own arguments (`-renderer`, `-judge`, `-cycle`) rather
than rebuilding, and the recipe's `cycle` action gains a renderer argument so
"under each renderer" is three launches of one build.

Run: `just unity-editor` (compiles the samples and the bake shader), then
`just unity-demo 6000.3.23f1 compare`. Expected: `report.txt` with `PASS`, one
line per entry with its fraction, the applied count and the allocation delta.

- [ ] **Step 7: Mutation, then commit**

Mutate: make `Apply` write every element (loop over `Rects` instead of `Dirty`):
the package-gate scan fails — that mutation needs no player run. Mutate: bake
the sprite with `aa = 0`; relaunch the built player under `-judge`: the fraction
must move. Revert both.

```bash
just test && git add -A unity justfile && git commit -m "feat(unity): the faithful Canvas entries beside the painter, one player, one key" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

Then `just build`, `just unity-demo … cycle` under each renderer, the PR.

---

### Task 3: The thread-time instrument and the URP floor (story S3)

Worktree `v021-thread-cost-instrument`, branch
`story/v021-thread-cost-instrument`. No dependency; runs in parallel with Tasks
1 and 2.

**Files:**

- Create: `Runtime/ThreadCostMath.cs`, `Runtime/ThreadCostAccumulator.cs`,
  `Runtime/Engine/DashsceneThreadCost.cs`,
  `measure/android/fixtures/unity-frame-cost.log`,
  `unity/package-gate/tests/thread_cost_instrument.rs`
- Modify: `measure/android/unity-frame-cost.sh`,
  `measure/android/frame-table.py`, `unity/ffi-check/Program.cs`,
  `unity/demo/DemoBuild.cs` and `unity/render-gate/RenderGateBuild.cs`
  (`CreatePipeline`, the same five lines in both),
  `unity/render-gate/DashsceneRenderGate.cs` (`Judge()` reads one line and the
  asset back), `Samples~/Showcase/DashsceneShowcase.cs` (two lines after the
  frame-cost push, with track O)

**Interfaces:**

- Produces: `ThreadCostMath` (`Mean`, `P95` with `AwayFromZero`, `NsToMs`,
  `PerFrame`) and `ThreadCostAccumulator` (`Sample = 240`, `WarmUp = 60`,
  `Push(entry, width, height, mainNs, renderNs, canvasNs, gcBytes) -> ThreadCostSample?`),
  both Unity-free in `Runtime/`; `DashsceneThreadCost` in `Runtime/Engine/`
  (`OffArgument`, `Armed`, `Reason`, `Push(entry, width, height)`);
  `ThreadCostSample.Line()` producing
  `<entry> at WxH over 240 frames — main mean 1.23 p95 2.10 ms, render mean 0.80 p95 1.40 ms, canvas 0.00 ms, gc 0 B/frame`,
  logged as `[showcase] thread cost — …`; `frame-table.py` writing
  `unity-threads.md` beside `unity-frames.md`.

- [ ] **Step 1: The package-gate scans for the instrument and the floor (failing
      first)**

`unity/package-gate/tests/thread_cost_instrument.rs`:

```rust
use package_gate::{cs_scan, root};

fn blanked(rel: &str) -> String {
    cs_scan::blank_comments_and_strings(&std::fs::read_to_string(root().join(rel)).unwrap())
}

#[test]
fn both_pipeline_builders_set_the_five_floor_fields_explicitly() {
    for rel in ["unity/demo/DemoBuild.cs", "unity/render-gate/RenderGateBuild.cs"] {
        let scanned = blanked(rel);
        let (s, e) = cs_scan::member_body(&scanned, "private static void CreatePipeline(List<string> failures)");
        let body = &scanned[s..e];
        for field in ["urp.supportsHDR = false", "urp.msaaSampleCount = 1", "urp.supportsCameraDepthTexture = false",
                      "urp.supportsCameraOpaqueTexture = false", "renderer.postProcessData = null"] {
            assert!(body.contains(field), "{rel}: CreatePipeline must set {field}");
        }
    }
}

#[test]
fn the_arithmetic_and_the_accumulator_have_no_unity_dependency() {
    for rel in ["unity/com.driftsys.dashscene/Runtime/ThreadCostMath.cs", "unity/com.driftsys.dashscene/Runtime/ThreadCostAccumulator.cs"] {
        let scanned = blanked(rel);
        assert!(!scanned.contains("using UnityEngine") && !scanned.contains("using Unity."), "{rel} must compile outside Unity");
    }
    let m = blanked("unity/com.driftsys.dashscene/Runtime/ThreadCostMath.cs");
    assert!(m.contains("MidpointRounding.AwayFromZero"), "P95 rounds as DashsceneFrameCost.At does, or the two lines disagree at a midpoint");
}

#[test]
fn the_instrument_reads_unitys_recorders_and_refuses_an_unknown_counter() {
    let scanned = blanked("unity/com.driftsys.dashscene/Runtime/Engine/DashsceneThreadCost.cs");
    assert!(scanned.contains("ProfilerRecorder.StartNew("), "the counters are Unity's recorders, not Stopwatch brackets");
    assert!(scanned.contains("!_canvasSend.Valid || !_canvasBatch.Valid || !_gcAlloc.Valid"), "every counter must be valid, or the instrument disarms and says which");
}
```

Run: `cargo nextest run -p package-gate thread_cost_instrument` — Expected:
FAIL.

- [ ] **Step 2: The arithmetic and the accumulator in `Runtime/`, the recorders
      in `Runtime/Engine/`**

The two Unity-free classes sit beside `CommitPacer.cs` in `Runtime/`, where
`package-compat` and `ffi-check` compile them by glob and `runtime_split.rs`
would refuse them under `Runtime/Engine/`. The recorder wrapper is
`Runtime/Engine/` code, which the render gate sees because it imports the
package.

`Runtime/ThreadCostMath.cs`:

```csharp
namespace Driftsys.Dashscene
{
    public static class ThreadCostMath
    {
        public static double Mean(double[] v) { double t = 0; foreach (var x in v) t += x; return t / v.Length; }
        // DashsceneFrameCost.At's rule, rounding included: values[round((len - 1) * p)] over the sorted copy.
        public static double P95(double[] v)
        {
            var c = (double[])v.Clone(); System.Array.Sort(c);
            return c[(int)System.Math.Round((c.Length - 1) * 0.95, System.MidpointRounding.AwayFromZero)];
        }
        public static double NsToMs(long ns) => ns / 1e6;
        public static long PerFrame(long total, int frames) => total / frames;
    }
}
```

`Runtime/ThreadCostAccumulator.cs` — the sampling logic, keyed on `entry@WxH`
like the frame-cost line, one sample per 240 drawn frames, and **the first 60
frames after a key change discarded** so an entry's load and the Canvas's bakes
do not land in its first sample:

```csharp
namespace Driftsys.Dashscene
{
    public sealed class ThreadCostSample
    {
        public string Entry; public int Width, Height, Frames;
        public double MainMean, MainP95, RenderMean, RenderP95, CanvasRebuildMean; public long GcAllocBytesPerFrame;
        public string Line() => string.Format(System.Globalization.CultureInfo.InvariantCulture,
            "{0} at {1}x{2} over {3} frames — main mean {4:F2} p95 {5:F2} ms, render mean {6:F2} p95 {7:F2} ms, canvas {8:F2} ms, gc {9} B/frame",
            Entry, Width, Height, Frames, MainMean, MainP95, RenderMean, RenderP95, CanvasRebuildMean, GcAllocBytesPerFrame);
    }

    public sealed class ThreadCostAccumulator
    {
        public const int Sample = 240;
        public const int WarmUp = 60;
        private readonly double[] _main = new double[Sample], _render = new double[Sample], _canvas = new double[Sample];
        private long _gc; private int _n, _skip; private string _key;

        public ThreadCostSample Push(string entry, int width, int height, long mainNs, long renderNs, long canvasNs, long gcBytes)
        {
            var key = entry + "@" + width + "x" + height;
            if (key != _key) { _key = key; _n = 0; _gc = 0; _skip = WarmUp; }
            if (_skip > 0) { _skip--; return null; }
            _main[_n] = ThreadCostMath.NsToMs(mainNs); _render[_n] = ThreadCostMath.NsToMs(renderNs); _canvas[_n] = ThreadCostMath.NsToMs(canvasNs);
            _gc += gcBytes; _n++;
            if (_n < Sample) return null;
            var s = new ThreadCostSample { Entry = entry, Width = width, Height = height, Frames = Sample,
                MainMean = ThreadCostMath.Mean(_main), MainP95 = ThreadCostMath.P95(_main),
                RenderMean = ThreadCostMath.Mean(_render), RenderP95 = ThreadCostMath.P95(_render),
                CanvasRebuildMean = ThreadCostMath.Mean(_canvas), GcAllocBytesPerFrame = ThreadCostMath.PerFrame(_gc, Sample) };
            _n = 0; _gc = 0; return s;
        }
    }
}
```

`Runtime/Engine/DashsceneThreadCost.cs` — the recorders, and nothing else:

```csharp
using Unity.Profiling;

namespace Driftsys.Dashscene
{
    /// Per-thread frame time from Unity's own recorders — what the frame-cost
    /// line excludes by construction: the culling callback, the render
    /// thread's encode, and a Canvas rebuild. Both include the engine floor;
    /// subtract the empty entry's line for the renderer's share. Pushed from
    /// the host loop on drawn frames only, in the same phase as the
    /// frame-cost line, so the two lines describe the same frames.
    public sealed class DashsceneThreadCost : System.IDisposable
    {
        public const string OffArgument = "-no-thread-cost";
        public bool Armed { get; }
        public string Reason { get; }                 // why it is disarmed, for the log
        private ProfilerRecorder _main, _render, _canvasSend, _canvasBatch, _gcAlloc;
        private readonly ThreadCostAccumulator _acc = new ThreadCostAccumulator();

        public DashsceneThreadCost(string[] args)
        {
            if (System.Array.IndexOf(args, OffArgument) >= 0) { Reason = OffArgument; return; }
            _main = ProfilerRecorder.StartNew(ProfilerCategory.Internal, "Main Thread", 1);
            _render = ProfilerRecorder.StartNew(ProfilerCategory.Internal, "Render Thread", 1);
            _canvasSend = ProfilerRecorder.StartNew(ProfilerCategory.Gui, "Canvas.SendWillRenderCanvases", 1);
            _canvasBatch = ProfilerRecorder.StartNew(ProfilerCategory.Gui, "Canvas.BuildBatch", 1);
            _gcAlloc = ProfilerRecorder.StartNew(ProfilerCategory.Memory, "GC Allocated In Frame", 1);
            // every counter, not the two thread counters alone: a marker absent from a
            // non-development player reads as zero, and a zero Canvas rebuild term would
            // read as the Canvas being free
            if (!_main.Valid || !_render.Valid || !_canvasSend.Valid || !_canvasBatch.Valid || !_gcAlloc.Valid)
            { Reason = "a counter is not recordable on this player: " + Missing(); Dispose(); return; }
            Armed = true;
        }

        public ThreadCostSample Push(string entry, int width, int height) => Armed
            ? _acc.Push(entry, width, height, _main.LastValue, _render.LastValue, _canvasSend.LastValue + _canvasBatch.LastValue, _gcAlloc.LastValue)
            : null;

        public void Dispose() { _main.Dispose(); _render.Dispose(); _canvasSend.Dispose(); _canvasBatch.Dispose(); _gcAlloc.Dispose(); }
    }
}
```

A disarmed instrument is a warning in the log and a `Fail` in the render gate
(Step 5), not an exception in a shipped type. If the Canvas markers are absent
from a non-development player, the reading players are built with
`BuildOptions.Development` and the record says so beside every row.

- [ ] **Step 3: The arithmetic and the accumulator under ffi-check (failing
      first), then the two call sites**

Both classes are in `Runtime/`, so `FfiCheck.csproj`'s glob already compiles
them; `Program.cs` gains:

```csharp
Check("the thread-cost arithmetic is the frame-cost line's", () =>
{
    Expect(ThreadCostMath.Mean(new[] { 1.0, 2.0, 3.0 }) == 2.0, "mean");
    var twenty = new double[20]; for (var i = 0; i < 20; i++) twenty[19 - i] = i;   // reversed, so the sort matters
    Expect(ThreadCostMath.P95(twenty) == 18.0, "p95 is values[round(19 * 0.95)] = values[18] of the sorted copy");
    var thirtyOne = new double[31]; for (var i = 0; i < 31; i++) thirtyOne[i] = i;
    Expect(ThreadCostMath.P95(thirtyOne) == 29.0, "(31 - 1) * 0.95 = 28.5 rounds away from zero, as DashsceneFrameCost.At does");
    Expect(ThreadCostMath.NsToMs(1_500_000) == 1.5, "ns to ms");
    Expect(ThreadCostMath.PerFrame(2400, 240) == 10, "bytes per frame");
});

Check("the accumulator discards the warm-up, samples every 240 drawn frames, and resets on a key change", () =>
{
    var acc = new ThreadCostAccumulator();
    ThreadCostSample s = null;
    for (var f = 0; f < ThreadCostAccumulator.WarmUp + ThreadCostAccumulator.Sample - 1; f++)
        Expect(acc.Push("a", 10, 20, 1_000_000, 500_000, 0, 24) == null, $"no sample before the window closes (frame {f})");
    s = acc.Push("a", 10, 20, 1_000_000, 500_000, 0, 24);
    Expect(s != null && s.Frames == 240 && s.Width == 10 && s.MainMean == 1.0 && s.RenderMean == 0.5 && s.GcAllocBytesPerFrame == 24, "the sample after 60 + 240 pushes");
    Expect(s.Line().Contains(" at 10x20 over 240 frames"), "the line carries the extent the sweep script checks");
    for (var f = 0; f < 100; f++) acc.Push("a", 10, 20, 1_000_000, 500_000, 0, 0);
    Expect(acc.Push("b", 10, 20, 1_000_000, 500_000, 0, 0) == null, "a new entry resets and warms up again");
});
```

Run: `just unity-ffi` — Expected: FAIL (types missing), then PASS. Mutation:
`P95` without `AwayFromZero` returns `28.0` on the thirty-one — the check fails;
drop the warm-up skip — the first check's frame count fails.

**The call sites are the two host loops, in the same phase as the frame-cost
push.** In `DashsceneShowcase.Update`, directly after
`_frameCost.Push(Label(_index), Screen.width, Screen.height, tickTicks, drawTicks)`:
`var tc = _threadCost.Push(Label(_index), Screen.width, Screen.height); if (tc != null) Debug.Log($"[showcase] thread cost — {tc.Line()}");`
— two lines in track O's file, named in the PR and made with that lane; the
Canvas baseline (Task 2) pushes the same way from its own loop. Drawn frames
only, the label the loop already holds, no second component, no accessor, no log
scraping. `DashsceneFrameCost.At` is offered the same `ThreadCostMath.P95` in
the same two-line change so one arithmetic exists; if track O declines, the two
stay equal by the `AwayFromZero` scan above.

- [ ] **Step 4: The URP asset fields**

In `DemoBuild.CreatePipeline`, after `urp.useSRPBatcher = true;`:

```csharp
// The shared floor, set explicitly so the parity reading is not confounded by
// a default: each line names the default it replaced on 6000.3.23f1.
urp.supportsHDR = false;                    // default true — about one frame per second on surfaces (PR #1409)
urp.msaaSampleCount = 1;                    // default 4 in the asset, off at the camera; pinned at 1
urp.supportsCameraDepthTexture = false;     // default false; pinned
urp.supportsCameraOpaqueTexture = false;    // default false; pinned
renderer.postProcessData = null;            // default: URP's post-process data asset
```

The scan pins the source; the built asset is read back in Step 5's gate run
(`GraphicsSettings.currentRenderPipeline` cast to the URP asset, the five values
logged and `Fail`ed on mismatch), so a later overwrite is caught.

Run: `cargo nextest run -p package-gate thread_cost_instrument` — Expected:
PASS.

- [ ] **Step 5: The render gate reads one line and reads the asset back**

In `DashsceneRenderGate.Judge()`, a new numbered block: construct
`new DashsceneThreadCost(new string[0])` — the package type, visible to the gate
— `Fail` unless `Armed` (the counter names, confirmed on this editor), `Push` it
once per **`Update`** across a new 300-frame plan step (the gate runs one
`Render()` per `Update`, and a recorder's `LastValue` moves only when a Unity
frame ends — repeated synchronous `Render()` calls inside `Judge()` would read
one value 240 times), then `Line($"thread cost —
{sample.Line()}")`, `Fail` if
`MainMean <= 0`, and the five URP values read back from
`GraphicsSettings.currentRenderPipeline` and `Fail`ed on mismatch —
`RenderGateBuild.CreatePipeline` is the same body as `DemoBuild.CreatePipeline`
and gets the same five lines in this story, so the read-back holds on the gate's
own asset. The gate's `Distance` and `DifferingPixels` stay where they are.

Run: `just unity-render` — Expected: `report.txt` carries the line and `PASS`.

- [ ] **Step 6: One parser for both lines, and a fixture that can fail**

`measure/android/frame-table.py` is the parser: its `read()` matches the lean
painter's `dashscene:` sample lines and the `dashscene-cpu` records and joins
them per interval, and its `SOURCES` dict is provenance text, not a parser key.
This story adds two regexes to `read()`, `SAMPLE_UNITY_FRAME` for
`[showcase] frame cost — <entry> at WxH over N frames — tick …` and
`SAMPLE_UNITY_THREAD` for
`[showcase] thread cost — <entry> at WxH over N
frames — main mean …`, each
producing a row joined to the CPU records the way the lean painter's rows are,
plus a `unity-showcase` provenance entry. `unity-frame-cost.sh` keeps the sweep,
the extent guard and the capture; it stops writing `unity-frames.md` itself and
calls `frame-table.py --source
unity-showcase` over its captures to write
`unity-frames.md` and `unity-threads.md`. A device-free test, registered where
`harness-tests` lists its scripts: `frame-table.py` over
`measure/android/fixtures/unity-frame-cost.log` — three real lines of each kind
from Step 7's run, two `dashscene-cpu` records, and **one hand-corrupted line**
with a missing field — diffed against a committed expected table in which the
corrupted line appears under the script's existing `Unreadable` report.

Run: `just harness-tests` — Expected: green; removing the new regex fails it,
and so does making the corrupted line parse.

- [ ] **Step 7: The device reading, before and after the asset change**

With the device attached and no other lane holding it: the "before" is the APK
of the last merged base, kept under the lanes shelf by the story before this one
(or built once here and kept for the next); the "after" is HEAD's; for each,
`just unity-demo-android
6000.3.23f1 install`,
`ADB=$(just _android-adb) ./measure/android/unity-frame-cost.sh`, and
`DS_GPU_WINDOW=10 ./measure/android/gpu-capture.sh <out>
com.driftsys.dashscene.showcase`
on `surfaces` — on the painter, and on the Canvas as well if story 2 has landed
by then, which the record says either way. Record both in
`docs/design/android-toolchain.md` under a new sub-heading "The thread-time
line, and the URP floor (2026-09-…)", dumps under
`driftsys/dashscene-v021-lanes/probe-1443/`.

- [ ] **Step 8: Commit and ship**

```bash
just test && git add -A && git commit -m "feat(measure): a thread-time line beside the frame-cost line, and the URP floor pinned" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 4: The settle path (story S4)

Worktree `v021-settle-path`, branch `story/v021-settle-path`. Waits for PRs
#1431 and #1433 and the opaque-core branch to land, since it edits
`BrgPainter.cs`.

**Files:**

- Create: `Runtime/SettleLoop.cs`, `unity/package-gate/tests/settle_path.rs`
- Modify: `Runtime/Engine/BrgPainter.cs` (`Draw`, `UploadHeap`, `BindHeap`,
  `HeapBindCount`), `Samples~/FrameLoop/DashsceneFrameLoop.cs`,
  `Samples~/Showcase/DashsceneShowcase.cs` (the loop body, with track O),
  `Samples~/Showcase/DashsceneCanvasBaseline.cs`,
  `unity/render-gate/DashsceneRenderGate.cs`, `unity/ffi-check/Program.cs`

**Interfaces:**

- Consumes: `DashsceneRuntime.Tick(float) -> bool` (true on the first tick after
  a load, so a load needs no forced redraw), `AcquireFrame()`,
  `FrameLease.MarkDrawn()`, `BrgPainter.SetAtlases`,
  `FrameLease.DocumentReplaced`, `BrgPainter.EdgeWidth`.
- Produces: `SettleLoop` (`ForceRedraw()`, `NoteExtent(int, int)`,
  `bool ShouldDraw(bool advanced)`, `Pending`, `FramesSkipped`, `FramesDrawn`),
  Unity-free in `Runtime/`; on `BrgPainter`:
  `public bool HeapBindingPending { get; }` — raised by a reallocation, an atlas
  change, or a change of the scalars the binding carries — and
  `public int HeapBindCount { get; }`, with `Draw` calling `BindHeap` only when
  pending.

- [ ] **Step 1: The package-gate scans (failing first)**

`unity/package-gate/tests/settle_path.rs`:

```rust
use package_gate::{cs_scan, painter_source, root};

fn sample(rel: &str) -> String {
    cs_scan::blank_comments_and_strings(&std::fs::read_to_string(root().join("unity/com.driftsys.dashscene").join(rel)).unwrap())
}

#[test]
fn draw_binds_the_heap_exactly_once_and_only_when_pending() {
    let scanned = painter_source();   // already blanked
    let (s, e) = cs_scan::member_body(&scanned, "public void Draw(FrameLease lease)");
    let body = &scanned[s..e];
    assert_eq!(body.matches("BindHeap()").count(), 1, "one call site in Draw: {body}");
    assert!(cs_scan::squeeze(body).contains("if (HeapBindingPending) BindHeap();"), "the one call is guarded: {body}");
}

#[test]
fn the_pending_flag_is_raised_by_every_reason_the_binding_can_go_stale() {
    let scanned = painter_source();
    // a reallocation, a new atlas set, and the scalars the binding carries
    for (member, needle) in [("private static bool Upload(", "return reallocated;"),
                             ("private void UploadHeap()", "_heapBindingPending |= Upload("),
                             ("public void SetAtlases(TextAtlasSet atlases)", "_heapBindingPending = true"),
                             ("private void ReleaseAtlases()", "_heapBindingPending = true"),
                             ("private void UploadHeap()", "if (scalars != _boundScalars) _heapBindingPending = true")] {
        let (s, e) = cs_scan::member_body(&scanned, member);
        assert!(cs_scan::squeeze(&scanned[s..e]).contains(needle), "{member} must raise the flag: {needle}");
    }
}

#[test]
fn every_host_loop_decides_through_settle_loop() {
    for rel in ["Samples~/FrameLoop/DashsceneFrameLoop.cs", "Samples~/Showcase/DashsceneShowcase.cs", "Samples~/Showcase/DashsceneCanvasBaseline.cs"] {
        let scanned = sample(rel);
        let (s, e) = cs_scan::member_body(&scanned, "private void Update()");
        let body = cs_scan::squeeze(&scanned[s..e]);
        assert!(body.contains("_settle.NoteExtent(Screen.width, Screen.height);"), "{rel}: the extent is noted");
        assert!(body.contains("var advanced = _runtime.Tick(dt);"), "{rel}: Tick's return is read");
        assert!(body.contains("if (!_settle.ShouldDraw(advanced)) return;"), "{rel}: the decision is SettleLoop's");
    }
}
```

Run: `cargo nextest run -p package-gate settle_path` — Expected: FAIL.

- [ ] **Step 2: `SettleLoop`, the decision, in `Runtime/` with no Unity
      dependency — and its ffi-check test (failing first)**

The runtime already reports the two things a first draft of this class carried
as reasons: a fresh load is `advanced` on its first tick (`LiveScene::advanced`
is true before the first `mark_shown`), and an atlas set changes only with a
load. What the runtime cannot know is host-side: the drawable's extent, and a
surface the host rebuilt. So the class carries a bool and an extent, nothing
else — the shape the three Rust hosts already share.

`Runtime/SettleLoop.cs`:

```csharp
namespace Driftsys.Dashscene
{
    /// The one decision a host loop makes per frame: draw, or skip everything
    /// after the tick. Skips only when the tick reported no advance AND no
    /// forced redraw is pending; a forced redraw is consumed by the draw it
    /// forces. No Unity types, so unity/ffi-check executes it.
    public sealed class SettleLoop
    {
        private bool _pending; private int _w = -1, _h = -1;
        public bool Pending => _pending;
        public int FramesSkipped { get; private set; }
        public int FramesDrawn { get; private set; }
        /// A host-side reason the runtime cannot report: a rebuilt surface, an embedder's request.
        public void ForceRedraw() { _pending = true; }
        public void NoteExtent(int w, int h) { if (w != _w || h != _h) { _w = w; _h = h; _pending = true; } }
        public bool ShouldDraw(bool advanced)
        {
            if (!advanced && !_pending) { FramesSkipped++; return false; }
            _pending = false; FramesDrawn++; return true;
        }
    }
}
```

`Program.cs`, compiled by the existing `Runtime/**` glob:

```csharp
Check("the settle loop skips only when nothing advanced and nothing is pending", () =>
{
    var loop = new SettleLoop();
    loop.NoteExtent(100, 50);
    Expect(loop.Pending && loop.ShouldDraw(false) && !loop.Pending, "a first extent forces one draw and is consumed");
    Expect(!loop.ShouldDraw(false) && loop.FramesSkipped == 1, "settled: skip");
    Expect(loop.ShouldDraw(true) && loop.FramesDrawn == 2, "advanced: draw");
    loop.ForceRedraw(); Expect(loop.ShouldDraw(false) && !loop.ShouldDraw(false), "a forced redraw is one draw");
    loop.NoteExtent(100, 50); Expect(!loop.ShouldDraw(false), "the same extent forces nothing");
    loop.NoteExtent(50, 100); Expect(loop.ShouldDraw(false), "a new extent forces one draw");
});
Check("a fresh runtime's first tick advances, so a load needs no forced redraw", () =>
{
    // load a document, tick once: Expect(advanced) — the C ABI's own test says the same
});
```

Run: `just unity-ffi` — Expected: FAIL (type missing), then PASS. Mutation:
`ShouldDraw` ignoring `_pending` — the forced and extent cases fail.

- [ ] **Step 3: `BrgPainter.HeapBindingPending`, raised by every reason, and
      `HeapBindCount`**

`BindHeap` binds three buffers **and the scalars**
`(EdgeWidth, SolidBase,
GradientBase)`, and the scalars move without any
reallocation: the anti-aliasing width on every resize, the gradient base
whenever the paint table interns a new solid. So the flag is raised in four
places. `Upload(...)` is `private static` over a `ref GraphicsBuffer`, so it
cannot set an instance field: it returns `true` when it (re)created the buffer,
and `UploadHeap` raises the flag on any `true` — the four call sites read
`_heapBindingPending |= Upload(...)`. The others are `SetAtlases`,
`ReleaseAtlases`, and `UploadHeap` when the scalars it would bind differ from
`_boundScalars` (a `Vector4` kept by `BindHeap`). `UploadHeap` also skips a
table's `SetData` when its packed floats equal the last uploaded copy, which is
most commits for most tables. `BindHeap()` increments
`public int HeapBindCount`, records `_boundScalars`, and clears the flag; `Draw`
reads:

```csharp
UploadHeap();
UploadInstances();
if (HeapBindingPending) BindHeap();
```

- [ ] **Step 4: The three host loops decide through `SettleLoop`**

`DashsceneFrameLoop.Update`, `DashsceneShowcase.Update` (track O's file; the
loop body only, named in the PR) and `DashsceneCanvasBaseline.Update`:

```csharp
_settle.NoteExtent(Screen.width, Screen.height);
var advanced = _runtime.Tick(dt);
if (!_settle.ShouldDraw(advanced)) return;
using (var frame = _runtime.AcquireFrame())
{
    if (frame.DocumentReplaced) { _painter.SetAtlases(_runtime.ReadAtlases()); }
    _painter.Draw(frame);
    frame.MarkDrawn();
}
```

`ForceRedraw()` has no caller in the samples: Unity rebuilds its own surface and
the painter's buffers survive it, and a host with a real surface event is what
the method is for. No focus-change mapping. The comment both loops carry today —
"a host that skipped would never mark a commit shown, so a settled scene would
keep reporting that it advanced" — describes acquiring without marking; this
loop skips the **acquire** only when the tick reports nothing unshown, and every
acquire it takes is marked, so that comment is replaced by one stating this
invariant, and the ffi-check's fresh-runtime case pins it.

- [ ] **Step 5: The render gate's four assertions**

A new plan step in `DashsceneRenderGate`, on the static document, driven through
`SettleLoop` — one `Render()` per `Update`, as the gate's steps run, so the
frames are Unity frames:

1. sixty frames — `Line($"settle — drew {drawn} of 60 frames")`, `Fail` unless
   `drawn == 1`;
2. the capture after frame 1 and the capture after frame 60 are pixel-identical
   (`DifferingPixels == 0`): a skipped frame leaves the batches registered and
   the callback re-emitting them — the positive pin the absence scan cannot
   give;
3. `GC.GetAllocatedBytesForCurrentThread()` equal before and after the 59
   skipped frames, **and** across one drawn partial-pack frame that follows (a
   pulse, then a draw): the ranged path allocates nothing either;
4. then a second **drawn** frame — force one through `_settle.ForceRedraw()` —
   with `HeapBindCount` still `1` and `!HeapBindingPending`; then resize the
   target by one pixel, draw again, and `Fail` unless `HeapBindCount == 2`: the
   anti-aliasing width changed and the scalars rebound.

Run: `just unity-render` — Expected: `PASS` with `settle — drew 1 of 60
frames`
and `heap bound 2 time(s) after the resize`.

- [ ] **Step 6: Mutations**

Mutate `Draw` to call `BindHeap()` unconditionally: `settle_path` fails on the
guard, and the gate's step 4 fails with a count of 2 before the resize. Mutate
`UploadHeap` to never compare the scalars: the gate's step 4 fails with a count
of 1 after the resize. Mutate `ShouldDraw` to ignore `_pending`:
`just unity-ffi` fails. Revert all three.

- [ ] **Step 7: Commit and ship**

```bash
just test && git add -A && git commit -m "feat(unity): the host idles when the tick reports no advance, and the heap binds on change" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

`just unity-editor`, `just unity-render`, `just build`, PR.

---

### Task 5: The dirty-range pack and upload (story S5)

Worktree `v021-dirty-range-upload`, branch `story/v021-dirty-range-upload`.
Depends on Task 4.

**Files:**

- Create: `Runtime/InstanceSpans.cs`, `Runtime/StreamLayout.cs`,
  `unity/package-gate/tests/dirty_range_upload.rs`
- Modify: `Runtime/FramePacker.cs`, `Runtime/Engine/BrgPainter.cs`,
  `unity/ffi-check/Program.cs`, `unity/render-gate/DashsceneRenderGate.cs`

**Interfaces:**

- Consumes: `FramePacker.Pack(DsFrame, MaterialClass, TextAtlasSet)`,
  `Emit(...)` (the one instance writer), `EmitRun`, `DsFrame.Dirty` (`uint` rect
  indices), `DsFrame.Generation`.
- Produces: `InstanceSpans` (`Span { Offset, Count }`, `Of(int rect)`,
  `RectCount`, `SameShapeAs`, `From((int, int)[])`,
  `Coalesce(uint* dirty, int count, List<(int Offset, int Count)> into)` — the
  port of `dashscene-gpu`'s `dirty_ranges` with its four cases, writing into a
  reused list) and `StreamLayout` (`HeadWords`, `Streams`,
  `Ranges(capacity, row, rows)`, `Cut(capacity, first, count)`), both Unity-free
  in `Runtime/`; on `FramePacker`: `Spans`, `LastPackWasPartial`, `DirtyRanges`,
  and the per-rect row assertion; on `BrgPainter`: `LastUpload` derived from the
  one upload wrapper.

- [ ] **Step 1: The package-gate scan (failing first)**

```rust
use package_gate::{cs_scan, painter_source};

#[test]
fn the_whole_array_upload_sits_inside_the_full_pack_branch_and_nowhere_else() {
    let scanned = painter_source();
    let (s, e) = cs_scan::member_body(&scanned, "private void UploadInstances()");
    let body = &scanned[s..e];
    let whole = "Upload(0, _staging.Length)";
    assert_eq!(body.matches(whole).count(), 1, "one whole-array upload in UploadInstances");
    // member_body brace-matches any substring's block, an `if` included
    let (bs, be) = cs_scan::member_body(body, "if (!_packer.LastPackWasPartial)");
    assert!(body[bs..be].contains(whole), "the whole-array upload is inside the full-pack branch: {}", &body[bs..be]);
    assert!(body.contains("foreach (var range in _packer.DirtyRanges)"), "a ranged upload loop exists");
    assert!(!body.contains("_instanceBuffer.SetData("), "every upload goes through the one wrapper that reports LastUpload");
}
```

Run: `cargo nextest run -p package-gate dirty_range_upload` — Expected: FAIL.

- [ ] **Step 2: `InstanceSpans.cs`**

```csharp
namespace Driftsys.Dashscene
{
    /// Which instance rows each rect packed to on the last full pack. The shape
    /// dashscene-gpu's InstanceSpan and dirty_ranges already have: a rect is not
    /// a row — it packs to a fill, stacked fills and a stroke, or to nothing when
    /// refused — so a dirty rect index maps to a span, and a commit that keeps
    /// every span's count can be applied in place.
    public sealed class InstanceSpans
    {
        public struct Span { public int Offset; public int Count; }
        private Span[] _spans = new Span[64];
        public int RectCount { get; private set; }
        public Span Of(int rect) => _spans[rect];
        internal void Begin(int rects) { if (_spans.Length < rects) System.Array.Resize(ref _spans, rects * 2); RectCount = rects; }
        internal void Set(int rect, int offset, int count) { _spans[rect] = new Span { Offset = offset, Count = count }; }
        /// dashscene-gpu's dirty_ranges: the dirty set's own order, a span merged into the
        /// previous emitted range when adjacent, a zero-count span skipped.
        public void Coalesce(uint[] dirty, int count, List<(int Offset, int Count)> into)
        {
            into.Clear();
            for (var d = 0; d < count; d++)
            {
                var span = _spans[dirty[d]];
                if (span.Count == 0) continue;
                if (into.Count > 0 && into[into.Count - 1].Offset + into[into.Count - 1].Count == span.Offset)
                    into[into.Count - 1] = (into[into.Count - 1].Offset, into[into.Count - 1].Count + span.Count);
                else into.Add((span.Offset, span.Count));
            }
        }
    }
}
```

- [ ] **Step 3: The packer's two paths — instances ranged, the heap whole**

In `FramePacker.Pack(frame, materialClass, atlases)`:

```csharp
var partial = _previousRectCount == (int)frame.Rects.CountAsLong
    && frame.Generation == _previousGeneration + 1
    && frame.Dirty.CountAsLong > 0
    && !frame.DocumentReplacedFlag;
LastPackWasPartial = false;
PackHeap(frame);                                   // always whole — see below
if (partial && TryPackDirty(frame, materialClass, atlases)) { LastPackWasPartial = true; }
else { PackAllInstances(frame, materialClass, atlases); }   // today's body, plus Spans.Set per rect in PackRect
_previousRectCount = (int)frame.Rects.CountAsLong; _previousGeneration = frame.Generation;   // the spans themselves stay in Spans; no copy
```

**The heap is repacked and uploaded whole on every commit**, as the lean painter
does (`render.rs`'s `upload` writes the paint heap in full every frame): a
changed paint earns a new interned row rather than rewriting one, the solids sit
before the gradients in the one heap array, so a new solid moves `GradientBase`
and every gradient row behind it. There is no stable heap slot to rewrite. The
heap is `rows × 16 B`; the instance buffer is what R-T4 bounds, and it alone
takes the ranged path.

`TryPackDirty` re-emits each dirty rect **into its previous span** (a `_writeAt`
cursor the `Emit` writer honours instead of appending) and returns `false` —
abandoning to the full pack — if the rect emits a different count than its span
holds. After it returns `true` the packer asserts, per dirty rect, that the rows
written equal the span's count, and throws `DashscenePainterException`
otherwise: a partial pack that wrote fewer rows than it claims is a wrong
picture with a green gate, and this is the guard the mutation in Step 6 hits.
Glyph runs (`EmitRun`) take the same path per run row.

`DirtyRanges` is `Spans.Coalesce(frame.Dirty, _dirtyRanges)`, written into the
packer's one reused list so a partial commit allocates nothing: the port of
`dashscene-gpu`'s `dirty_ranges`, in the dirty set's own order, merging a span
into the previous emitted range when it is adjacent, and **skipping a span whose
count is zero** (a refused rect, a layout-only container) so it neither emits an
empty range nor breaks the merge of its neighbours. Not sorted: the committed
dirty set is sorted already, and an unsorted one merges less, which is what the
lean painter's test pins.

- [ ] **Step 4: The painter's ranged upload — per batch and per stream — and
      what it reports**

`FillStaging` lays each batch out **stream-major**: the batch head (two matrices
and the zero `float4`, 28 words), then five streams — `Quad`, `Corners`,
`Shade`, `Pivot`, `Paint` — each `capacity × 4` words, so instance `i`'s five
properties sit at five disjoint offsets. A row range is therefore five uploads,
and a span can straddle a batch boundary because the packer never sees
`_instancesPerBatch`. Both facts live in one Unity-free class,
`Runtime/StreamLayout.cs`:

```csharp
public static class StreamLayout
{
    public const int HeadWords = 28;   // 112 bytes: unity_ObjectToWorld, unity_WorldToObject, the zero float4
    public const int Streams = 5;
    /// The five (first, count) word ranges holding rows [row, row + rows) of ONE batch.
    public static (int First, int Count)[] Ranges(int capacity, int row, int rows)
    {
        var r = new (int, int)[Streams];
        for (var s = 0; s < Streams; s++) r[s] = (HeadWords + s * capacity * 4 + row * 4, rows * 4);
        return r;
    }
    /// A global row range cut at batch boundaries: (batch, rowInBatch, rows) pieces.
    public static IEnumerable<(int Batch, int Row, int Rows)> Cut(int capacity, int first, int count)
    {
        while (count > 0)
        {
            var batch = first / capacity; var row = first % capacity; var take = Math.Min(count, capacity - row);
            yield return (batch, row, take); first += take; count -= take;
        }
    }
}
```

`BrgPainter.UploadInstances`, every upload through one wrapper so the report is
derived from the bytes sent and not hand-set:

```csharp
public enum UploadKind { None, Whole, Ranges }
public readonly struct InstanceUpload { public readonly UploadKind Kind; public readonly int Ranges; public readonly int Rows; }
public InstanceUpload LastUpload { get; private set; }

private void Upload(int first, int count) { _instanceBuffer.SetData(_staging, first, first, count); _uploadedWords += count; _uploads++; }

private void UploadInstances()
{
    _uploadedWords = 0; _uploads = 0;
    if (InstanceCount == 0) { LastUpload = default; return; }
    EnsureCapacity(InstanceCount);
    if (!_packer.LastPackWasPartial)
    {
        FillStaging();
        Upload(0, _staging.Length);
        LastUpload = new InstanceUpload(UploadKind.Whole, _uploads, InstanceCount);
        return;
    }
    foreach (var range in _packer.DirtyRanges)
        foreach (var (batch, row, rows) in StreamLayout.Cut(_instancesPerBatch, range.Offset, range.Count))
        {
            FillStaging(batch, row, rows);                                   // rewrites only those rows of that batch
            var head = batch * BatchWords;
            foreach (var (first, count) in StreamLayout.Ranges(_instancesPerBatch, row, rows)) Upload(head + first, count);
        }
    LastUpload = new InstanceUpload(UploadKind.Ranges, _uploads, _uploadedWords / (StreamLayout.Streams * 4));
}
```

`Rows` on the ranged path is the words sent divided by the words one row
occupies across its five streams, so an implementation that uploaded the whole
array on the partial path would report the whole row count and fail the render
gate's check in Step 5.

Run: `cargo nextest run -p package-gate dirty_range_upload` — Expected: PASS.

- [ ] **Step 5: The ffi-check drives — the layout, the coalescing, the packer's
      ranges — and the gate's row count**

`unity/ffi-check/Program.cs` (`InstanceSpans` and `StreamLayout` are in
`Runtime/`, compiled by the existing glob):

```csharp
Check("StreamLayout is FillStaging's layout, stream-major per batch", () =>
{
    var r = StreamLayout.Ranges(capacity: 8, row: 3, rows: 2);
    Expect(Same(r, new[] { (40, 8), (72, 8), (104, 8), (136, 8), (168, 8) }), "head 28 + stream s * 32 + row 3 * 4, two rows of four words");
    Expect(Same(StreamLayout.Cut(8, 7, 2), new[] { (0, 7, 1), (1, 0, 1) }), "a span straddling a batch boundary is cut in two");
    Expect(Same(StreamLayout.Cut(8, 2, 3), new[] { (0, 2, 3) }), "a span inside one batch is one piece");
});

Check("coalescing follows dashscene-gpu's dirty_ranges, all four cases", () =>
{
    var spans = InstanceSpans.From(new[] { (0, 2), (2, 3), (5, 1) });
    var into = new List<(int, int)>();
    spans.Coalesce(new uint[] { 0, 1, 2 }, 3, into); Expect(Same(into, new[] { (0, 6) }), "adjacent dirty rects merge into one range");
    spans.Coalesce(new uint[] { 0, 2 }, 2, into);    Expect(Same(into, new[] { (0, 2), (5, 1) }), "a clean rect between two dirty ones splits the range");
    spans.Coalesce(new uint[] { 2, 0, 1 }, 3, into); Expect(Same(into, new[] { (5, 1), (0, 5) }), "an unsorted set still writes every named rect, merging less");
    var withEmpty = InstanceSpans.From(new[] { (0, 2), (2, 0), (2, 1) });
    withEmpty.Coalesce(new uint[] { 1 }, 1, into);    Expect(into.Count == 0, "a rect that draws nothing contributes no range");
    withEmpty.Coalesce(new uint[] { 0, 1, 2 }, 3, into); Expect(Same(into, new[] { (0, 3) }), "and does not break the merge around it");
});

Check("a commit's dirty rects repack into their spans and the packer reports their coalesced ranges", () =>
{
    // the demo producer pass: build scene 0 (`surfaces`), tick, acquire, pack; pulse (any signal step: several
    // rects go dirty); tick, acquire, pack again
    Expect(packer.LastPackWasPartial, "the second commit keeps every rect's instance count, so it is partial");
    packer.Spans.Coalesce(DirtyOf(frame), DirtyCount(frame), expected); Expect(Same(packer.DirtyRanges, expected), "the ranges are the dirty rects' coalesced spans");
    // then the variant that adds a stroke to one rect: a changed per-rect instance count
    Expect(!packer.LastPackWasPartial, "a changed instance count is a whole pack");
});
```

The painter needs a graphics device, so `LastUpload` is pinned in
`DashsceneRenderGate`: draw the same two commits, `Fail` unless the second draw
reports `UploadKind.Ranges` with `Rows` equal to the sum of the dirty rects'
span counts, and unless the stroke variant's draw reports `Whole`.

Run: `just unity-ffi` and `just unity-render` — Expected: the checks counted and
green.

- [ ] **Step 6: Goldens, mutation, device reading**

`just unity-render` — the goldens must be unchanged (a partial pack that wrote a
wrong row shows as ink at a wrong centre). Mutate: make `TryPackDirty` emit one
row fewer than a span holds — the packer's own assertion throws and the gate
reports it. Mutate: upload the whole array on the partial path — the wrapper
counts the whole array's rows and the gate's `Rows` check fails. Revert both.

On the device (batched with Task 6's session if the lane allows): the frame-cost
`draw` term on `surfaces` during its transition, before and after, into
`docs/design/android-toolchain.md`.

- [ ] **Step 7: Commit and ship**

```bash
just test && git add -A && git commit -m "feat(unity): dirty rects repack into their spans and upload as ranges (R-T4)" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

Tell issue #1306 by comment; do not put a closing keyword in a sentence with its
number.

---

### Task 6: The per-command term (story S6)

Worktree `v021-per-command-term`, branch `story/v021-per-command-term`. Depends
on Task 3; needs the device.

**Files:**

- Create: `driftsys/dashscene-v021-lanes/probe-1406/` (outside the repo: two
  APKs, dumps, `RESULTS.md`)
- Modify: `docs/design/android-toolchain.md`, `docs/design/unity-csharp-host.md`

- [ ] **Step 1: The two builds**

Build A: HEAD. Build B: HEAD with `OnPerformCulling`'s one-command-per-instance
emission replaced by one command per contiguous `_instanceAtlas` run (the
pre-#1401 shape —
`git show d1129ea~30:unity/com.driftsys.dashscene/Runtime/Engine/BrgPainter.cs`
and read the commit that introduced the split, PR #1407, for the exact earlier
body), applied as an uncommitted patch kept under
`driftsys/dashscene-v021-lanes/probe-1406/` and named in its `RESULTS.md`. Both
`just unity-demo-android 6000.3.23f1 build` at profile `demo-release`; keep both
APKs.

- [ ] **Step 2: The readings**

For each APK: `adb install -r`, launch, `keyevent 93` to `typography`, then
`ADB=$(just _android-adb) ./measure/android/unity-frame-cost.sh` and
`DS_GPU_WINDOW=10 ./measure/android/gpu-capture.sh <out> com.driftsys.dashscene.showcase`,
plus `--latency` through `latency-cadence.py` beside `probe-1412`. Three windows
each. Build B drops frames (D5); note the band-frame count from the player's
`DROP` lines; the reading is the cost, not the picture.

- [ ] **Step 3: The record**

`docs/design/android-toolchain.md`, a sub-heading under the Unity host's
presented-rate section: the two rows (render-thread mean and p95, main-thread
mean, `frameReady` mean) and one sentence: whether the per-command term is at or
above 1 ms on either thread, which is the threshold for Task 7 being on the
parity path rather than R-T4's. `docs/design/unity-csharp-host.md`'s gaps list:
the "#1406 unmeasured" item becomes the reading. Comment on story S7 with the
sentence.

- [ ] **Step 4: Commit and ship**

```bash
just prim && just test && git add docs && git commit -m "docs(measure): the per-command term on the Pixel 5, one command per instance against one per run" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 7: In-order instanced draws for the two blended classes (story S7)

Worktree `v021-in-order-draws`, branch `story/v021-in-order-draws`. Depends on
Task 6's reading; the order gate issue #1402 asked for landed with PR #1433 and
is on `main`.

**Files:**

- Create: `Runtime/Engine/OrderedDrawFeature.cs`,
  `Runtime/Engine/OrderedDrawPass.cs`
- Modify: `Runtime/Engine/BrgPainter.cs`,
  `Runtime/Shaders/DashsceneInstance.hlsl`,
  `Runtime/Resources/Dashscene/UnlitOverlay.shader` and `Text.shader`,
  `unity/demo/DemoBuild.cs`, `unity/render-gate/RenderGateBuild.cs`,
  `docs/decisions/unity-painter-uses-brg.md`,
  `docs/decisions/brg-draw-command-order-is-not-guaranteed.md`,
  `unity/package-gate/tests/single_instance_commands.rs`

**Interfaces:**

- Consumes:
  `RasterCommandBuffer.DrawProcedural(Matrix4x4, Material, int shaderPass, MeshTopology, int vertexCount, int instanceCount, MaterialPropertyBlock)`
  (SRP core, verified present beside the six-argument form); `FramePacker`'s
  streams and `FramePacker.InstanceAtlas` (the material per instance, read by
  the painter as `_packer.InstanceAtlas[i]`); the sheet matrix.
- Produces: `OrderedDrawFeature : ScriptableRendererFeature` with
  `public static void Register(BrgPainter painter)` /
  `Unregister(BrgPainter painter)` — `BrgPainter.Dispose` calls `Unregister`, so
  the renderer switch in Task 2 (destroy the painter's component, then add the
  other) leaves no stale reference, and the pass draws nothing when no painter
  is registered; `OrderedDrawPass : ScriptableRenderPass` recorded at
  `RenderPassEvent.AfterRenderingTransparents` whose `RecordRenderGraph` issues,
  per contiguous same-material run in instance order, one
  `DrawProcedural(sheet, material, 0, MeshTopology.Triangles, 6, run.Count, block)`
  after `block.SetInteger(FirstInstanceId, run.Offset)` — `SetInteger`, the
  integer setter; `SetInt` is the deprecated float path and would store `5.0f`'s
  bits; on `BrgPainter`: `public static DrawPath PathFor(MaterialClass c)`
  returning `Ordered` for `UnlitOverlay` and `Brg` for the two lit classes, and
  `public const DrawPath TextPath = DrawPath.Ordered` — text is not a
  `MaterialClass` and is always blended, so it always takes the ordered path;
  `private void CutRuns()`, called from `Draw`, cutting runs where
  `_packer.InstanceAtlas[i]` changes, in instance order; a
  `StructuredBuffer<DsInstance> _DsInstances` (the five streams interleaved, 80
  bytes) bound to the two procedural materials; the shaders read
  `_DsInstances[_DsFirstInstance + instanceId]` with
  `#pragma instancing_options procedural:DsSetup` and `SV_InstanceID`.

- [ ] **Step 1: The scans (failing first)**

In `unity/package-gate/tests/single_instance_commands.rs`, scope the existing D5
assertions to the lit path (`PathFor` returns `Brg`), and add:

```rust
#[test]
fn the_ordered_pass_issues_one_draw_per_material_run_in_run_order() {
    let src = std::fs::read_to_string(package_gate::root().join("unity/com.driftsys.dashscene/Runtime/Engine/OrderedDrawPass.cs")).unwrap();
    let scanned = package_gate::cs_scan::blank_comments_and_strings(&src);
    let (s, e) = package_gate::cs_scan::member_body(&scanned, "public override void RecordRenderGraph(RenderGraph renderGraph, ContextContainer frameData)");
    let body = &scanned[s..e];
        assert!(body.contains("foreach (var run in d.Runs)"), "runs are iterated in order inside the render function");
    assert!(body.contains("d.Block.SetInteger(FirstInstanceId, run.Offset)"), "the first instance is an integer set from the run's offset, never a float");
    assert!(body.contains("DrawProcedural("), "the draw is procedural");
    assert!(!body.contains("Sort("), "nothing re-sorts the runs in the pass");
    let painter = package_gate::painter_source();
    let (s, e) = package_gate::cs_scan::member_body(&painter, "private void CutRuns()");
    assert!(!painter[s..e].contains("Sort(") && !painter[s..e].contains("OrderBy("), "the runs are cut in instance order, never sorted");
}
```

Run: `cargo nextest run -p package-gate single_instance_commands` — Expected:
FAIL.

- [ ] **Step 2: The feature and the pass**

`OrderedDrawPass.RecordRenderGraph`:
`using var builder = renderGraph.AddRasterRenderPass<PassData>("Dashscene ordered draws", out var data)`;
`builder.SetRenderAttachment(resourceData.activeColorTexture, 0)`;
`builder.AllowPassCulling(false)`;
`builder.SetRenderFunc((PassData d, RasterGraphContext ctx) => { foreach (var run in d.Runs) { d.Block.SetInteger(FirstInstanceId, run.Offset); ctx.cmd.DrawProcedural(d.Sheet, run.Material, 0, MeshTopology.Triangles, 6, run.Count, d.Block); } })`.
Runs are cut by `BrgPainter.CutRuns()` at `Draw`, in instance order, where
`_packer.InstanceAtlas[i]` changes, and handed to the feature through
`Register`.

- [ ] **Step 3: The shaders — one keyword, no new files**

The two blended classes' shaders gain
`#pragma multi_compile _
DASHSCENE_PROCEDURAL` beside their `DOTS_INSTANCING_ON`
line, so `UnlitOverlay.shader` and `Text.shader` each compile a procedural
variant and no shader file is added (a file per path would double every later
shader change for those two classes, and would meet Task 8's kind keywords as
six files instead of four). In `DashsceneInstance.hlsl`, under
`#ifdef DASHSCENE_PROCEDURAL`:
`StructuredBuffer<DsInstance> _DsInstances;
int _DsFirstInstance;` and a
`DsLoad(uint id)` that fills the same five properties
`UNITY_ACCESS_DOTS_INSTANCED_PROP` fills today, so `DsVertexStage` and `DsShade`
are unchanged. The procedural materials are the class material and the text
materials with the keyword enabled.

- [ ] **Step 4: Route the classes**

In `BrgPainter`: `PathFor(_materialClass)` for the class, `TextPath` for the
glyph runs; under `Ordered`, `Draw` uploads the interleaved `_DsInstances`
buffer (a second `GraphicsBuffer`, `Target.Structured`, stride 80), through Task
5's ranged path on the partial commit (the same `StreamLayout` cut, one stream),
and `CutRuns()`; no batches are registered and `OnPerformCulling` emits nothing
for those classes. `DemoBuild.CreatePipeline` and `RenderGateBuild` add
`OrderedDrawFeature` to `renderer.rendererFeatures`.

Run: `cargo nextest run -p package-gate single_instance_commands` — Expected:
PASS. `just unity-editor` — Expected: both new shaders compile every variant.

- [ ] **Step 5: The order gate, the goldens, the mutation**

`just unity-render` — the order gate issue #1402 landed with PR #1433 (its
probes composite pixels where document order decides the picture) must pass
unchanged on the ordered path. Mutate: iterate `d.Runs` in reverse — the gate
must fail. Mutate: `SetInt` in place of `SetInteger` — every run past the first
vanishes and the gate's ink predicate fails. Revert both.

- [ ] **Step 6: Records and the device**

Amend `unity-painter-uses-brg.md` honestly: D1 chose BRG for the bulk of the
SDF-quad UI, lit included, on one path — this story **reverses that for the two
blended classes**, and the record says so and why (the sorted-transparent path's
one-command-per-instance cost, D5 of the order record, measured in Task 6),
keeping D1's lit half; D3's rung 3 is no longer "nothing is built for it". Amend
`brg-draw-command-order-is-not-guaranteed.md`'s scope, D1, D4 and D5 to the
classes that stay on BRG: the ordered path has no keys.
`docs/design/unity-csharp-host.md`: a "Two draw paths" paragraph. Device: the
typography cadence and render-thread line before and after, into
`docs/design/android-toolchain.md`.

- [ ] **Step 7: Commit and ship**

```bash
just test && git add -A && git commit -m "feat(unity): the overlay and text classes draw in document order through a render-graph pass" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 8: Fast paths and per-document specialisation (story S8)

Worktree `v021-fast-paths`, branch `story/v021-fast-paths`. Depends on Task 3
and on issue #1413's per-kind sweep, which this task runs first.

**Files:**

- Create: `crates/dashpaint/src/gradient_strip.rs`,
  `crates/dashpaint/src/kind_set.rs`, `crates/dashscene-gpu/tests/kind_set.rs`
- Modify: `crates/dashscene-gpu/src/shaders/paint.wgsl`,
  `crates/dashscene-gpu/src/render.rs`,
  `crates/dashscene-gpu/tests/layer2_conformance.rs` (`reference_ramp` moves
  into `dashpaint`), `crates/dashscene-core/src/committed.rs` and `arena.rs`
  (the kind set and the strip, per commit), `crates/dashscene-ffi/src/lib.rs`
  (`ds_runtime_kind_set`, `ds_runtime_gradient_strip`), `Runtime/Native.cs`,
  `Runtime/DashsceneRuntime.cs`, `Runtime/Shaders/DashsceneInstance.hlsl`, every
  `.shader` under `Runtime/Resources/Dashscene/`,
  `Runtime/Engine/BrgPainter.cs`, `unity/ffi-check/Program.cs`,
  `conformance/layer2-probes.json`, `docs/design/android-toolchain.md`

**Interfaces:**

- Consumes: `gradient_colour(row, bounds, p)` and `gradient_ramp` in the WGSL;
  `reference_ramp` in `crates/dashscene-gpu/tests/layer2_conformance.rs`;
  `PaintTable::all_gradients()`, `all_stops()`, `all_strokes()`,
  `ClipTable::all_boxes()` and `push(&[ClipBox])`,
  `GradientStop { offset, color }`, `MAX_GRADIENT_STOPS = 8`;
  `Renderer::new_async`'s `create_render_pipeline` with
  `PipelineCompilationOptions::default()`, whose `constants` is `&[(&str, f64)]`
  in wgpu 30; the bind group layout's 11 entries; `ds_runtime_atlas` as the
  shape of a per-load call.
- Produces: in `crates/dashpaint`, `gradient_strip::ramp(stops, t) -> Color`
  (the CPU ramp the conformance test carried as `reference_ramp`, moved here and
  re-exported to it),
  `bake(gradients, stops) -> StripImage { width: 256, rows, rgba8: Vec<u8> }`
  and `bake_row(stops, out: &mut [u8; 1024])`; on `CommittedScene`,
  `gradient_strip() -> &StripImage` and `strip_generation() -> u64`, re-baked at
  a commit whose gradient rows changed;
  `ds_runtime_gradient_strip(runtime, out DsSlice)` handing the rows out as
  `rows × 1024` bytes with the generation; a texture binding `11`
  (`texture_2d<f32>`) and sampler `12` (filtering) in the paint bind group; in
  `paint.wgsl`:
  `override HAS_CLIPS: bool = true; override HAS_STROKES: bool = true;` — two,
  because only the clip loop and the stroke arm exist to be compiled out — and
  `fn gradient_colour` sampling
  `textureSampleLevel(gradient_strip, strip_sampler, vec2f(t, (f32(row) + 0.5) / f32(strip_rows)), 0.0)`;
  `dashpaint::kind_set::KindSet { clips, strokes }` with
  `KindSet::of(&PaintTable, &ClipTable)`, `bits() -> u32` and
  `constants() -> [(&'static str, f64); 2]` — the shape wgpu 30's
  `PipelineCompilationOptions.constants: &[(&str, f64)]` takes;
  `CommittedScene::kind_set()`, computed at every commit; `ds_runtime_kind_set`,
  read by the Unity painter on every drawn frame;
  `Renderer::pipeline_kind_set() -> KindSet`, the cache key of the pipeline
  bound at the last draw; on the Unity side the strip read through
  `ds_runtime_gradient_strip` into a `Texture2D(256, rows, RGBA32, linear)` with
  `LoadRawTextureData`, re-uploaded when the generation moves, bound as
  `_DsGradientStrip`; the keywords `DS_HAS_CLIPS` on every shader and
  `DS_HAS_STROKES` on the non-text shaders only (the text arm reaches no
  stroke), selected by `BrgPainter.ApplyKindSet(uint bits)` on every drawn frame
  from `ds_runtime_kind_set`, toggling only when the bits change;
  `BrgPainter.KeywordsFor(uint bits) -> string[]` as a pure static.

- [ ] **Step 1: The sweep first (device)**

Per issue #1413: a variant sweep on the Pixel 5 at fixed shaded area, per kind.
The instrument is `crates/dashscene-gpu/examples/gpu_time.rs`'s shape with a
per-kind scene (a scratch example under `demo-web/examples/`, kept uncommitted
under `driftsys/dashscene-v021-lanes/probe-1449/`, as `overdraw.rs` was under
`probe-1403/`): solid, linear gradient, radial gradient, stroke, glyph run, each
over the same 2 Mpx. Record in `docs/design/android-toolchain.md` beside the Q-6
table. The kinds above twice the solid rate are the ones the fast paths below
must move; if gradients are not, the strip is still built (it is what makes a
gradient pixel one sample) but the record says the sweep did not require it.

- [ ] **Step 2: The strip baker, with its quantisation test (failing first)**

`crates/dashpaint/src/gradient_strip.rs`:

```rust
pub const STRIP_WIDTH: usize = 256;

pub struct StripImage { pub rows: usize, pub rgba8: Vec<u8> }

/// One row per gradient row of the paint heap: the ramp evaluated at 256
/// positions, straight alpha, RGBA8. The fragment samples with bilinear
/// filtering, so between two texels the result is the linear interpolation
/// the ramp itself is; the only error is a stop that falls between texels,
/// bounded by 1/512 of t.
pub fn bake(gradients: &[Gradient], stops: &[GradientStop]) -> StripImage {
    let mut rgba8 = vec![0u8; STRIP_WIDTH * 4 * gradients.len()];
    for (i, g) in gradients.iter().enumerate() {
        let s = &stops[g.stops.offset as usize..(g.stops.offset + g.stops.count) as usize];
        bake_row(s, (&mut rgba8[i * STRIP_WIDTH * 4..(i + 1) * STRIP_WIDTH * 4]).try_into().unwrap());
    }
    StripImage { rows: gradients.len(), rgba8 }
}

pub fn bake_row(stops: &[GradientStop], out: &mut [u8; STRIP_WIDTH * 4]) {
    for x in 0..STRIP_WIDTH {
        let t = (x as f32 + 0.5) / STRIP_WIDTH as f32;
        let c = ramp(stops, t);            // the same piecewise-linear ramp gradient_ramp evaluates
        out[x * 4..x * 4 + 4].copy_from_slice(&[q(c.r), q(c.g), q(c.b), q(c.a)]);
    }
}
fn q(v: f32) -> u8 { (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8 }
```

Tests in the same file, against hand-computed values, all four channels:

```rust
#[test]
fn a_three_stop_ramp_bakes_the_hand_computed_texels() {
    let stops = [GradientStop { offset: 0.0, color: Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 } },
                 GradientStop { offset: 0.37, color: Color { r: 1.0, g: 0.5, b: 0.0, a: 1.0 } },
                 GradientStop { offset: 1.0, color: Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 } }];
    let mut row = [0u8; STRIP_WIDTH * 4];
    bake_row(&stops, &mut row);
    let texel = |t: f32| { let x = ((t * STRIP_WIDTH as f32) as usize).min(STRIP_WIDTH - 1); &row[x * 4..x * 4 + 4] };
    // midway through the first segment: (0.5, 0.25, 0, 1); through the second: (0.5, 0.25, 0.5, 1)
    assert!(texel(0.185).iter().zip([128u8, 64, 0, 255]).all(|(a, e)| (*a as i32 - e as i32).abs() <= 1), "{:?}", texel(0.185));
    assert!(texel(0.685).iter().zip([128u8, 64, 128, 255]).all(|(a, e)| (*a as i32 - e as i32).abs() <= 1), "{:?}", texel(0.685));
    // the end texels are sampled at their centres, 1/512 in from t = 0 and t = 1
    assert!(texel(0.001).iter().zip([1u8, 1, 0, 255]).all(|(a, e)| (*a as i32 - e as i32).abs() <= 1), "{:?}", texel(0.001));
    assert!(texel(0.999).iter().zip([1u8, 0, 254, 255]).all(|(a, e)| (*a as i32 - e as i32).abs() <= 1), "{:?}", texel(0.999));
}

#[test]
fn a_two_stop_ramp_is_exact_at_every_texel_centre() {
    // a different ramp per channel, so a channel swap or a dropped channel fails
    let stops = [GradientStop { offset: 0.0, color: Color { r: 0.0, g: 1.0, b: 0.0, a: 1.0 } },
                 GradientStop { offset: 1.0, color: Color { r: 1.0, g: 0.0, b: 0.5, a: 1.0 } }];
    let mut row = [0u8; STRIP_WIDTH * 4];
    bake_row(&stops, &mut row);
    for x in 0..STRIP_WIDTH {
        let t = (x as f32 + 0.5) / STRIP_WIDTH as f32;
        let q = |v: f32| (v * 255.0 + 0.5) as u8;
        assert_eq!(&row[x * 4..x * 4 + 4], &[q(t), q(1.0 - t), q(0.5 * t), 255], "texel {x}");
    }
}

#[test]
fn a_stop_between_texel_centres_lands_on_the_nearer_texel() {
    // a hard step (two stops at one offset) at t = 0.5 + 1/1024: texel 128's centre is
    // at 0.50195, past the step, so texel 127 is black and 128 is white
    let stops = [GradientStop { offset: 0.0, color: Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 } },
                 GradientStop { offset: 0.5 + 1.0 / 1024.0, color: Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 } },
                 GradientStop { offset: 0.5 + 1.0 / 1024.0, color: Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 } },
                 GradientStop { offset: 1.0, color: Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 } }];
    let mut row = [0u8; STRIP_WIDTH * 4];
    bake_row(&stops, &mut row);
    assert_eq!(row[127 * 4], 0); assert_eq!(row[128 * 4], 255);
    // and the exact step offset divides by nothing: gradient_segment_t's hard-stop guard
    let at_step = ramp(&stops, 0.5 + 1.0 / 1024.0);
    assert!(at_step.r.is_finite() && at_step.r == 1.0, "a hard stop takes the later colour, never NaN");
}

#[test]
fn stops_inside_the_range_clamp_to_the_end_colours() {
    // the ordinary case a producer authors by moving a handle: stops at 0.25 and 0.75
    let stops = [GradientStop { offset: 0.25, color: Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 } },
                 GradientStop { offset: 0.75, color: Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 } }];
    let mut row = [0u8; STRIP_WIDTH * 4];
    bake_row(&stops, &mut row);
    assert_eq!(&row[0..4], &[0, 0, 0, 255], "below the first stop, the first colour");
    assert_eq!(&row[255 * 4..256 * 4], &[255, 255, 255, 255], "above the last stop, the last colour");
    assert_eq!(row[128 * 4], ((0.5015625f32 - 0.25) / 0.5 * 255.0 + 0.5) as u8, "the midpoint texel sits on the ramp");
}
```

Mutation: swap the green and blue writes in `bake_row` — both tests fail; drop
the `+ 0.5` rounding — the second fails from texel 128 on. `ramp` is the one CPU
ramp: the conformance test's `reference_ramp` moves into `dashpaint` and that
test imports it, so the hard-stop rule has one statement.

Run: `cargo nextest run -p dashscene-gpu gradient_strip` — Expected: FAIL
(module absent), then PASS after the code.

- [ ] **Step 3: The WGSL: early-out and the strip sample**

In `paint.wgsl`, after `let kind = in.rows.x;` in `fs_main`:

```wgsl
// A plain fill: no corner radius, no stroke, no clip. Only the box edge's
// anti-aliasing and the fill colour. Compiled out of the general path when
// the document's kind set has neither clips nor strokes (the overrides below).
if (kind == KIND_FILL_SOLID && in.shape == 0u && in.rows.w == 0u && all(in.corners == vec4f(0.0))) {   // a baked-vector node's silhouette is its mask, never its box
    let d = rounded_box_sdf(in.local - 0.5 * in.bounds.zw, 0.5 * in.bounds.zw, vec4f(0.0));   // the same distance, radii zero
    let cover = coverage(d, globals.aa) * in.opacity;
    if (cover <= 0.0) { discard; }
    let colour = paints[in.rows.y];
    return vec4f(colour.rgb * colour.a * cover, colour.a * cover);
}
```

`gradient_colour` keeps its parameter arithmetic (the four `gradient_*_t` calls)
and replaces the stop loop and `gradient_ramp` with
`textureSampleLevel(gradient_strip, strip_sampler, vec2f(t, (f32(row) + 0.5) / f32(globals.strip_rows)), 0.0)`.
`clip_coverage` is wrapped: `if (!HAS_CLIPS) { return 1.0; }`; the stroke arm
`if (HAS_STROKES && kind == KIND_STROKE)`. The two `override` declarations go at
the top of `paint.wgsl`; nothing else is compiled out, so nothing else gets a
constant. `globals` gains `strip_rows: u32`.

- [ ] **Step 4: The Rust side: binding, pipeline constants, the kind set**

In `render.rs`: bind group layout entries `11` (texture, FRAGMENT,
`Float { filterable: true }`, `D2`) and `12` (sampler, `Filtering`); the strip
texture is written from `scene.gradient_strip()` whenever
`scene.strip_generation()` moved since the last upload. The pipeline:
`compilation_options: PipelineCompilationOptions { constants: &kind_set.constants(), ..Default::default() }`
on the fragment stage, the pipeline cached per `KindSet`
(`HashMap<KindSet, RenderPipeline>`) and **re-selected on every paint from
`scene.kind_set()`**, because the tables it is a census of grow when a paint or
a stroke is interned mid-run — the atlas precedent does not transfer, an atlas
set changes only with a load.

**The kind set is one function, in `crates/dashpaint/src/kind_set.rs`**, over
the committed tables, with the two bits the shading compiles out and no more:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct KindSet { pub clips: bool, pub strokes: bool }

impl KindSet {
    pub fn of(paints: &PaintTable, clips: &ClipTable) -> Self {
        Self { clips: !clips.all_boxes().is_empty(), strokes: !paints.all_strokes().is_empty() }
    }
    pub fn bits(&self) -> u32 { (self.clips as u32) | (self.strokes as u32) << 1 }
    pub fn from_bits(bits: u32) -> Self { Self { clips: bits & 1 != 0, strokes: bits & 2 != 0 } }
    pub fn constants(&self) -> [(&'static str, f64); 2] {
        [("HAS_CLIPS", self.clips as u8 as f64), ("HAS_STROKES", self.strokes as u8 as f64)]
    }
}
```

`CommittedScene` holds it (`pub fn kind_set(&self) -> KindSet`, computed at
every commit), and `dashscene-ffi` exposes
`ds_runtime_kind_set(runtime, out
u32)` returning the front scene's `bits()` — a
call the Unity painter makes on every drawn frame, after the acquire. Tests, in
`kind_set.rs`, concrete and failing first:

```rust
#[test]
fn an_empty_table_has_no_kind() {
    let k = KindSet::of(&PaintTable::default(), &ClipTable::default());
    assert_eq!(k, KindSet::default());
    assert_eq!(k.constants(), [("HAS_CLIPS", 0.0), ("HAS_STROKES", 0.0)]);
    assert_eq!(k.bits(), 0);
}

#[test]
fn each_bit_is_its_own_and_round_trips() {
    let mut clips = ClipTable::default();
    clips.push(&[ClipBox { x: 0.0, y: 0.0, w: 1.0, h: 1.0, corners: CornerRadii::default() }]);
    let k = KindSet::of(&PaintTable::default(), &clips);
    assert!(k.clips && !k.strokes);
    assert_eq!(k.constants(), [("HAS_CLIPS", 1.0), ("HAS_STROKES", 0.0)]);
    assert_eq!(k.bits(), 1);
    let both = KindSet { clips: true, strokes: true };
    assert_eq!(both.bits(), 3);
    assert_eq!(KindSet::from_bits(2), KindSet { clips: false, strokes: true });
    assert_eq!(KindSet::from_bits(both.bits()), both);
}
```

And in `crates/dashscene-gpu/tests/kind_set.rs`, the selection **and what it
compiles out**, on the offscreen renderer `layer2_conformance.rs` builds:

```rust
#[test]
fn a_document_with_no_clip_boxes_selects_the_clip_free_pipeline_and_a_clipped_one_does_not() {
    // the scene goldens/tooling/tests/v01.rs builds (no clip box), through the same tooling helpers: paint once
    assert!(!renderer.pipeline_kind_set().clips);
    // the scene goldens/tooling/tests/v03_clips.rs builds (clip boxes): paint once
    assert!(renderer.pipeline_kind_set().clips);
}

#[test]
fn the_clip_free_pipeline_really_has_no_clip_loop() {
    // the v03_clips scene, painted through the pipeline for KindSet { clips: false, .. } by the test-only
    // `Renderer::force_kind_set`, must ink a pixel outside a clip box that the selected pipeline
    // leaves clear — the compiled-out branch is observed, not the cache key
}

#[test]
fn a_commit_that_interns_a_stroke_reselects_the_pipeline() {
    // a scene with no stroke, painted; then set_variant to the arm that adds one, tick, paint:
    assert!(renderer.pipeline_kind_set().strokes);
}
```

Mutation: `constants()` always returning `HAS_CLIPS = 1.0` fails the first unit
test; `KindSet::of` ignoring the clip table fails the second and the selection
test; selecting the pipeline at load only fails the third.

- [ ] **Step 5: The Unity side**

The strip is not baked twice: `BrgPainter` reads it through
`ds_runtime_gradient_strip` on a drawn frame whose generation moved, into a
`Texture2D(256, rows, TextureFormat.RGBA32, mipChain: false, linear: true)` with
`LoadRawTextureData`, bound as `_DsGradientStrip` on the class and text
materials. `unity/ffi-check` asserts the slice's stride is 1024, that the row
count equals the paint table's gradient count, and that row 0's bytes equal
`dashpaint`'s `bake_row` for the same stops, read back through the ABI.
`FramePacker` bakes it beside the heap; `BrgPainter` binds `_DsGradientStrip` on
the class material and text materials in `BindHeap`. `DashsceneInstance.hlsl`:
`DsGradientColour` samples `_DsGradientStrip` with
`SamplerState sampler_DsGradientStrip` in place of `gradient_ramp`; `DsShade`
gains the plain-fill early-out; `#if DS_HAS_CLIPS` around `DsClipCoverage`'s
body. The four `.shader` files: `#pragma multi_compile _ DS_HAS_CLIPS`,
`#pragma multi_compile _ DS_HAS_STROKES`,
`#pragma multi_compile _ DS_HAS_GRADIENTS`. `ds_runtime_kind_set`'s and
`ds_runtime_gradient_strip`'s C# imports with the guarded forwarders `Native`
prescribes; `BrgPainter.KeywordsFor(uint bits)
-> string[]` as a pure static
(`DS_HAS_CLIPS` for bit 0, `DS_HAS_STROKES` for bit 1) and
`ApplyKindSet(uint bits)`, called on every drawn frame after the acquire and
toggling the keywords only when the bits changed. `unity/ffi-check` executes
both halves: `ds_runtime_kind_set` on `goldens/dsb/v07-negative-gap.dsb` (no
clip) and a demo scene that clips returns bit 0 clear and set respectively, a
second commit that interns a stroke sets bit 1, and `KeywordsFor(0)` is empty,
`KeywordsFor(1)` is `["DS_HAS_CLIPS"]`, `KeywordsFor(3)` is both in that order —
a mutation that enables every keyword fails it. The keywords, **local** — a
per-material keyword needs no slot in Unity's global keyword budget:
`#pragma multi_compile_local _ DS_HAS_CLIPS` on every `.shader` under
`Runtime/Resources/Dashscene/`, `#pragma
multi_compile_local _ DS_HAS_STROKES`
on the non-text ones — the text arm reaches no stroke, and a keyword that
removes nothing only multiplies the variant set `KeepAll` keeps.
`unity/package-gate` gains a scan that every `#if DS_HAS_*` in the HLSL has its
`multi_compile` in every shader whose class reaches that arm.

`just sdf-hlsl` is not needed: `sdf.wgsl` is unchanged. `just unity-editor`,
`just unity-conformance`, `just unity-render` — the render gate draws a second
commit that interns a stroke and `Fail`s unless the painter's keyword set
changed.

- [ ] **Step 6: Conformance and goldens**

`conformance/layer2-probes.json`: probes for `box_sdf` (if new) and for the
plain-fill coverage at the edge; re-record with `record_the_probe_table`, review
the diff, commit. `cargo nextest run -p goldens` — within tolerance on the lean
painter; `just unity-render` on the Unity painter. Record the largest golden
movement in the PR.

- [ ] **Step 7: The sweep after, commit and ship**

Repeat Step 1's sweep on HEAD; record beside the before. Commit:

```bash
just test && git add -A && git commit -m "feat(paint): a plain-fill path, a baked gradient strip, and pipelines specialised by the document's kind set (R-T5)" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 9: Occlusion on the shared side (story S9)

Worktree `v021-occlusion-pieces`, branch `story/v021-occlusion-pieces`. Depends
on Task 5 (the Unity packer's spans) and on the C ABI gates.

**Files:**

- Create: `crates/dashscene-core/src/occlusion.rs` (commit-time geometry, beside
  the clip resolution core already does at commit)
- Modify: `crates/dashpaint/src/lib.rs` (the `Piece` row, with the other
  boundary-B rows), `crates/dashpaint-abi/src/lib.rs` (`Piece` on the surface,
  with its measured layout), `unity/abi-check` (its round trip),
  `Runtime/BoundaryB.cs` (the `Piece` mirror),
  `crates/dashscene-core/src/committed.rs`,
  `crates/dashscene-core/src/arena.rs`, `crates/dashscene-ffi/src/lib.rs`,
  `crates/dashscene-ffi/include/dashscene.h`,
  `crates/dashscene-ffi/tests/abi.c`, `Runtime/Native.cs`,
  `Runtime/FrameLease.cs`, `Runtime/FramePacker.cs`,
  `crates/dashscene-gpu/src/render.rs` (the instance packer),
  `unity/ffi-check/Program.cs`, `crates/dashscene-gpu/tests/shaded_area.rs` (PR
  #1417's instrument for issue #1296)

**Interfaces:**

- Consumes:
  `RectEntry { x, y, w, h, paint: PaintIndex, clip: ClipIndex, opacity, rotation, rotation_anchor }`,
  `ClipIndex::UNCLIPPED`, `ClipBox { x, y, w, h, corners }`,
  `PaintEntry { fill, extra_fills, stroke, corners: CornerRadii, .. }`,
  `PaintTable::fill(kind) -> Fill`, `Color.a`, `GradientView` stops,
  `ClipTable::resolve(ClipIndex) -> ClipView` (boxes), `CommittedScene`'s
  `pub(crate)` fields, `commit_with` in `arena.rs`,
  `frame_of(scene, document_replaced)` and `DsSlice`/`slice_of` in
  `dashscene-ffi`, `FrameLease.RowSizes`, `Native.DsFrame`.
- Produces:

```rust
// crates/dashpaint/src/lib.rs — a boundary-B row, declared where RectEntry is
#[repr(C)] #[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece { pub rect: u32, pub x: f32, pub y: f32, pub w: f32, pub h: f32 }   // 20 bytes, 4-aligned

// crates/dashscene-core/src/occlusion.rs — the pass
pub const MAX_PIECES_PER_RECT: usize = 8;   // past this a rect keeps its whole quad and is not occluded

pub struct Occluder { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }   // an interior, axis-aligned

/// Which rects are opaque cores, and the interior each occludes.
pub fn cores(rects: &[RectEntry], paints: &PaintTable, clips: &ClipTable, groups: &[GroupComposite], aa: f32) -> Vec<(u32, Occluder)>
pub fn occludable(entry: &PaintEntry, paints: &PaintTable) -> bool   // false for a shape, an outside or centred stroke, or a shadow;

/// For every rect, the visible sub-rectangles after subtracting every LATER
/// core's interior. A rect with no later occluder yields one piece, itself.
/// Rotated rects (rotation != 0) are never occluded and never occlude.
pub fn pieces(rects: &[RectEntry], cores: &[(u32, Occluder)]) -> Vec<Piece>;

/// rectangle minus rectangle: 0 to 4 pieces
pub fn subtract(a: Occluder, b: Occluder, out: &mut Vec<Occluder>);
```

On `CommittedScene`: `pieces: Vec<Piece>` and
`pub fn pieces(&self) -> &[Piece]`; on `DsFrame`: `pub pieces: DsSlice` appended
after `glyph_quads`; `DS_ABI_VERSION: u32 = 3`; C#: `public DsSlice Pieces;`
last in `DsFrame`, `("pieces", 20)` last in `RowSizes`; both packers iterate
pieces and, for each, emit the rect's instances with the quad extent = the piece
and the box = the rect (the `Instance` already carries `bounds` for the SDF; the
vertex stage's quad corners come from a new `extent: vec4f` on `Instance` — 96
bytes — or from `outset`-style packing; the story picks the smaller change and
the ffi/abi gates hold the stride).

- [ ] **Step 1: The API split for testability, and the unit tests (failing
      first)**

The classification takes the resolved fill's alpha as a value, so a test
constructs no `PaintTable`:

```rust
pub enum FillAlpha { None, Opaque, Translucent }
pub fn solid_alpha(c: Color) -> FillAlpha
pub fn stops_alpha(stops: &[GradientStop]) -> FillAlpha        // Opaque only if every stop has a == 1
pub fn fill_alpha(paints: &PaintTable, kind: PaintKind) -> FillAlpha   // dispatches on PaintTable::fill; Image is Translucent
pub fn classify(rect: &RectEntry, corners: CornerRadii, alpha: FillAlpha, clip_boxes: &[ClipBox], aa: f32) -> Option<Occluder>   // a clip box's interior is inset by its own largest radius plus aa, like the rect's
// Refused as a core: a rect with a shape range (its silhouette is a field, not its box) and a rect inside a
// group composited below alpha 1. Kept whole, never occluded: a rect whose stroke outset or shadow reaches
// past its box — its ink is not where its box says.
pub fn cores(rects: &[RectEntry], paints: &PaintTable, clips: &ClipTable, groups: &[GroupComposite], aa: f32) -> Vec<(u32, Occluder)>
pub fn occludable(entry: &PaintEntry, paints: &PaintTable) -> bool   // false for a shape, an outside or centred stroke, or a shadow
pub fn subtract(a: Occluder, b: Occluder, out: &mut Vec<Occluder>)   // top, bottom, left, right — in that order
pub fn pieces(rects: &[RectEntry], cores: &[(u32, Occluder)]) -> Vec<Piece>   // capped by MAX_PIECES_PER_RECT
pub fn changed_rects(previous: &[Piece], current: &[Piece], out: &mut Vec<u32>)   // rects whose piece list differs — the commit dirties them
pub fn affects(dirty: &[u32], cores: &[(u32, Occluder)], pieces: &[Piece]) -> bool   // a dirty rect is a core, or has more than one piece
```

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn o(x: f32, y: f32, w: f32, h: f32) -> Occluder { Occluder { x, y, w, h } }
    fn rect(x: f32, y: f32, w: f32, h: f32, opacity: f32, rotation: f32) -> RectEntry {
        RectEntry { x, y, w, h, paint: PaintIndex(0), clip: UNCLIPPED, opacity, rotation, rotation_anchor: Vec2 { x: 0.0, y: 0.0 } }
    }
    const SQUARE: CornerRadii = CornerRadii { top_left: 0.0, top_right: 0.0, bottom_right: 0.0, bottom_left: 0.0 };
    const R8: CornerRadii = CornerRadii { top_left: 8.0, top_right: 8.0, bottom_right: 8.0, bottom_left: 8.0 };

    #[test] fn subtracting_a_disjoint_rect_keeps_the_whole() {
        let mut out = vec![]; subtract(o(0., 0., 10., 10.), o(20., 20., 5., 5.), &mut out);
        assert_eq!(out, vec![o(0., 0., 10., 10.)]);
    }
    #[test] fn subtracting_a_full_cover_leaves_nothing() {
        let mut out = vec![]; subtract(o(2., 2., 4., 4.), o(0., 0., 10., 10.), &mut out); assert!(out.is_empty());
    }
    #[test] fn subtracting_an_off_centre_rect_leaves_exactly_these_four_pieces() {
        // outer 10x7, hole at (2,1) 3x2: strips are top, bottom (full width), then left and right beside the hole
        let mut out = vec![]; subtract(o(0., 0., 10., 7.), o(2., 1., 3., 2.), &mut out);
        assert_eq!(out, vec![o(0., 0., 10., 1.), o(0., 3., 10., 4.), o(0., 1., 2., 2.), o(5., 1., 5., 2.)]);
    }
    #[test] fn an_occluder_over_a_corner_leaves_two_pieces() {
        // b covers a's top-left 3x3: no top strip, bottom (0,3,10,7), no left, right (3,0,7,3)
        let mut out = vec![]; subtract(o(0., 0., 10., 10.), o(-5., -5., 8., 8.), &mut out);
        assert_eq!(out, vec![o(0., 3., 10., 7.), o(3., 0., 7., 3.)]);
    }
    #[test] fn an_occluder_spanning_the_full_width_leaves_a_top_and_a_bottom_strip() {
        let mut out = vec![]; subtract(o(0., 0., 10., 10.), o(-1., 4., 12., 2.), &mut out);
        assert_eq!(out, vec![o(0., 0., 10., 4.), o(0., 6., 10., 4.)]);
    }
    #[test] fn an_occluder_touching_an_edge_leaves_one_strip() {
        let mut out = vec![]; subtract(o(0., 0., 10., 10.), o(0., 0., 10., 4.), &mut out);
        assert_eq!(out, vec![o(0., 4., 10., 6.)]);
    }
    #[test] fn a_translucent_fill_is_not_a_core_and_an_opaque_one_is() {
        assert!(classify(&rect(0., 0., 10., 10., 1.0, 0.0), SQUARE, FillAlpha::Translucent, &[], 1.0).is_none());
        assert!(matches!(solid_alpha(Color { r: 1.0, g: 1.0, b: 1.0, a: 0.5 }), FillAlpha::Translucent));
        assert!(matches!(solid_alpha(Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }), FillAlpha::Opaque));
        assert!(matches!(fill_alpha(&PaintTable::default(), PaintKind::NONE), FillAlpha::None));
    }
    #[test] fn a_gradient_is_a_core_only_when_every_stop_is_opaque() {
        let opaque = [GradientStop { offset: 0.0, color: Color { r: 0., g: 0., b: 0., a: 1.0 } },
                      GradientStop { offset: 1.0, color: Color { r: 1., g: 1., b: 1., a: 1.0 } }];
        assert!(matches!(stops_alpha(&opaque), FillAlpha::Opaque));
        let one_translucent = [opaque[0], GradientStop { offset: 1.0, color: Color { r: 1., g: 1., b: 1., a: 0.99 } }];
        assert!(matches!(stops_alpha(&one_translucent), FillAlpha::Translucent));
    }
    #[test] fn a_rect_with_a_shape_or_under_a_translucent_group_is_not_a_core() {
        // cores() with a PaintEntry whose shape range is non-empty → not a core;
        // cores() with a GroupComposite { start: 0, end: 1, alpha: 0.5 } over rect 0 → not a core
    }
    #[test] fn a_rect_whose_ink_reaches_past_its_box_is_never_occluded() {
        // occludable() is false for an outside stroke, a centred stroke and a drop shadow, true for an inside stroke;
        // pieces() keeps such a rect whole under a later core
    }
    #[test] fn a_node_below_opacity_one_is_not_a_core() {
        assert!(classify(&rect(0., 0., 10., 10., 0.5, 0.0), SQUARE, FillAlpha::Opaque, &[], 1.0).is_none());
    }
    #[test] fn a_square_core_occludes_its_box_shrunk_by_the_band() {
        assert_eq!(classify(&rect(0., 0., 40., 40., 1.0, 0.0), SQUARE, FillAlpha::Opaque, &[], 1.0), Some(o(1., 1., 38., 38.)));
    }
    #[test] fn a_rounded_core_occludes_its_box_shrunk_by_the_radius_and_the_band() {
        assert_eq!(classify(&rect(0., 0., 40., 40., 1.0, 0.0), R8, FillAlpha::Opaque, &[], 1.0), Some(o(9., 9., 22., 22.)));
    }
    #[test] fn a_clipped_core_occludes_only_inside_its_clip_inset_like_a_rect() {
        // a clip edge is anti-aliased and a clip box has corners of its own: inset by its largest radius plus aa
        let square = [ClipBox { x: 5., y: 5., w: 20., h: 20., corners: SQUARE }];
        assert_eq!(classify(&rect(0., 0., 40., 40., 1.0, 0.0), SQUARE, FillAlpha::Opaque, &square, 1.0), Some(o(6., 6., 18., 18.)));
        let rounded = [ClipBox { x: 5., y: 5., w: 20., h: 20., corners: CornerRadii { top_left: 4.0, top_right: 4.0, bottom_right: 4.0, bottom_left: 4.0 } }];
        assert_eq!(classify(&rect(0., 0., 40., 40., 1.0, 0.0), SQUARE, FillAlpha::Opaque, &rounded, 1.0), Some(o(10., 10., 10., 10.)));
    }
    #[test] fn a_core_too_small_for_its_own_band_occludes_nothing() {
        assert!(classify(&rect(0., 0., 2., 2., 1.0, 0.0), SQUARE, FillAlpha::Opaque, &[], 1.0).is_none());
    }
    #[test] fn a_rotated_rect_neither_occludes_nor_is_occluded() {
        assert!(classify(&rect(0., 0., 40., 40., 1.0, 0.3), SQUARE, FillAlpha::Opaque, &[], 1.0).is_none());
        let rects = [rect(0., 0., 10., 10., 1.0, 0.3), rect(0., 0., 10., 10., 1.0, 0.0)];
        let p = pieces(&rects, &[(1, o(1., 1., 8., 8.))]);
        assert_eq!(p, vec![Piece { rect: 0, x: 0., y: 0., w: 10., h: 10. }, Piece { rect: 1, x: 0., y: 0., w: 10., h: 10. }], "the rotated rect stays whole, and so does the last rect");
    }
    #[test] fn a_rect_that_would_split_past_the_cap_keeps_its_whole_quad() {
        // nine later cores in a row across a wide rect: nine subtractions would leave ten pieces
        let mut rects = vec![rect(0., 0., 100., 10., 1.0, 0.0)];
        let mut cores = vec![];
        for i in 0..9 { rects.push(rect(10. * i as f32 + 2., 2., 4., 6., 1.0, 0.0)); cores.push((i as u32 + 1, o(10. * i as f32 + 3., 3., 2., 4.))); }
        let p = pieces(&rects, &cores);
        assert_eq!(p.iter().filter(|p| p.rect == 0).collect::<Vec<_>>(), vec![&Piece { rect: 0, x: 0., y: 0., w: 100., h: 10. }]);
    }
    #[test] fn the_commit_dirties_a_rect_whose_pieces_changed() {
        let before = vec![Piece { rect: 0, x: 0., y: 0., w: 10., h: 1. }, Piece { rect: 1, x: 2., y: 1., w: 3., h: 2. }];
        let after  = vec![Piece { rect: 0, x: 0., y: 0., w: 10., h: 2. }, Piece { rect: 1, x: 2., y: 2., w: 3., h: 2. }];
        let mut out = vec![]; changed_rects(&before, &after, &mut out);
        assert_eq!(out, vec![0, 1]);
        let mut same = vec![]; changed_rects(&before, &before, &mut same);
        assert!(same.is_empty());
    }
    #[test] fn an_earlier_core_never_occludes_a_later_rect() {
        let rects = [rect(0., 0., 10., 10., 1.0, 0.0), rect(0., 0., 10., 10., 1.0, 0.0)];
        let p = pieces(&rects, &[(0, o(1., 1., 8., 8.))]);
        assert_eq!(p, vec![Piece { rect: 0, x: 0., y: 0., w: 10., h: 10. }, Piece { rect: 1, x: 0., y: 0., w: 10., h: 10. }]);
    }
    #[test] fn a_later_core_cuts_the_rect_beneath_it_into_pieces() {
        let rects = [rect(0., 0., 10., 7., 1.0, 0.0), rect(2., 1., 3., 2., 1.0, 0.0)];
        let p = pieces(&rects, &[(1, o(2., 1., 3., 2.))]);
        let under: Vec<_> = p.iter().filter(|p| p.rect == 0).map(|p| o(p.x, p.y, p.w, p.h)).collect();
        assert_eq!(under, vec![o(0., 0., 10., 1.), o(0., 3., 10., 4.), o(0., 1., 2., 2.), o(5., 1., 5., 2.)]);
        assert_eq!(p.iter().filter(|p| p.rect == 1).collect::<Vec<_>>(), vec![&Piece { rect: 1, x: 2., y: 1., w: 3., h: 2. }], "the core itself is whole");
    }
}
```

Run: `cargo nextest run -p dashscene-core occlusion` — Expected: FAIL (module
absent). Implement `subtract` (the four-strip split), `classify` (opacity 1,
alpha opaque, rotation 0, inset by the largest radius plus `aa`, intersected
with every clip box's own inset interior, `None` when the interior is empty),
`cores`, `pieces` (for each rect start with itself, subtract each **later**
core's interior, `Vec` reuse, and keep the whole rect when the count would pass
the cap), `changed_rects`, then PASS. Mutation: skip the radius inset — the
rounded test fails; subtract earlier cores too — the ordering test fails; drop
the cap — the cap test fails.

- [ ] **Step 1b: The piece count per scene, derived with no device, before the
      ABI moves**

A test beside `crates/dashscene-gpu/tests/shaded_area.rs`, on its shape: build
each showcase scene, pulse, tick, commit; call `occlusion::pieces` over
`committed.rects()` with `cores` from the committed tables; pin the piece count
and the summed piece area per scene, and record both in the PR beside the
derivation's rect count and area. This is the number that says whether the extra
instances are worth the area they save on each scene — and on the BRG path each
piece is one more draw command under D5 — so it is read here, before Step 2
changes the frame. If a scene's count is not worth it, the cap or the alpha rule
moves and this test moves with it.

- [ ] **Step 2: The commit and the frame**

`arena.rs` `commit_with`: after the rect table is built and before
`CommittedScene { … }`:

```rust
// recomputed only when a dirty rect is a core or lies under one — otherwise the
// previous commit's pieces stand, re-indexed if the table was renumbered
let pieces = if occlusion::affects(&dirty, previous.cores(), previous.pieces()) {
    let cores = occlusion::cores(&rects, &paints, &clips, AA_BAND);
    occlusion::pieces(&rects, &cores)
} else { previous.pieces().to_vec() };
// a moving core changes the pieces of the rects beneath it, which the
// entry-bit compare above cannot see: those rects are dirty too
occlusion::changed_rects(previous.pieces(), &pieces, &mut dirty);
dirty.sort_unstable(); dirty.dedup();
```

A core test in `arena.rs`'s own tests: two commits, an opaque core moved by
`set_prop` between them, the rect beneath it in the second commit's `dirty`
though its own entry is unchanged.

The band is the host's, not a constant: a painter's anti-aliasing width in
document units is one device pixel at the host's scale (`BrgPainter.EdgeWidth`
is set from the camera on every commit; `render.rs`'s `AA_WIDTH` is 1.0 at scale
1), so the commit takes it from the runtime —
`ds_runtime_set_aa_band(runtime,
float)`, a symbol addition, default 1.0, which
each host calls where it sets its extent — and insets every core by it. A band
set too small leaves a ring of background inside a fringe; the render gate draws
at a scale below 1:1 and `Fail`s on ink outside the occluder's silhouette.
`CommittedScene` gains `pieces`. `dashpaint-abi`: `Piece` on `abi_surface!` with
its measured row `("Piece", …, 20, 4)`, and `unity/abi-check` round-trips it — a
stride check sees size and not field order, which is how the reversed stroke row
survived that gate's first version. `dashscene-ffi`: `pieces: DsSlice` appended;
`frame_of` fills it with `slice_of(scene.pieces())`; `DsFrame::empty` with
`DsSlice::none::<Piece>()`; `DS_ABI_VERSION = 3` with the doc comment amended (a
`DsFrame` member changes the bytes `acquire` writes into the host's buffer,
which is the "signature changed" case);
`the_header_declares_the_frame_exactly_as_this_build_lays_it_out`'s `members`
array gains `("pieces", offset_of!(DsFrame, pieces))`;
`crates/dashscene-ffi/include/dashscene.h` gains the member; `tests/abi.c`'s
last-member assertion moves to `pieces`.

Run: `just test && just c-abi` — Expected: green, with the census test's count
of strides moved (read its message and update the "nineteen" it names).

- [ ] **Step 3: The C# side**

`Runtime/BoundaryB.cs`:
`public struct Piece { public uint Rect; public float X, Y, W, H; }` beside
`RectEntry`. `Native.DsFrame`: `public DsSlice Pieces;` last.
`FrameLease.RowSizes`: `("pieces", Marshal.SizeOf<Piece>())` last, as every
other row; `StridesOf` gains the line. `unity/ffi-check`'s `expectedOrder`
literal gains `"pieces"`; a new check:

```csharp
Check("the pieces slice reports this build's stride when empty and the rect index when full", () =>
{
    Expect(emptyFrame.Pieces.StrideAsLong == Marshal.SizeOf<Piece>() && emptyFrame.Pieces.CountAsLong == 0, "an empty table still names the row size");
    var pieces = (Piece*)frame.Pieces.Ptr;
    for (var i = 0; i < frame.Pieces.CountAsLong; i++)
        Expect(pieces[i].Rect < frame.Rects.CountAsLong && pieces[i].W > 0 && pieces[i].H > 0, $"piece {i} names a rect and has area");
    Expect(frame.Pieces.CountAsLong >= frame.Rects.CountAsLong - refused, "every drawn rect has at least one piece");
});
```

`EnsureAbiCompatible` expects 3.

Run: `just unity-ffi` — Expected: green, including the R-E17 mutation loop over
the new row.

- [ ] **Step 4: Both packers pack pieces**

`FramePacker.PackRect` becomes `PackPiece(piece, rect, entry, …)`: the loop is
over `frame.Pieces`, each piece's `rect` index fetching the rect and entry;
`Emit` takes the quad extent (the piece) separately from the shape box (the
rect). The span bookkeeping of Task 5 keys spans by **rect**, and a rect's span
now holds all of its pieces' instances; since the commit dirties every rect
whose pieces changed (Step 2), `TryPackDirty` sees them, and falls back to the
full pack only when a dirty rect's piece count changed — an edge crossing — not
on every transition frame. `render.rs`'s instance packer does the same over
`scene.pieces()`. Glyph runs are not occluded in this story (a glyph is never a
core, and text under a later core is rare; Step 1b says what it would be worth).

The quad extent does not lengthen the row: a piece whose extent equals its
rect's box carries nothing new, and the majority of rows (every glyph, every
unoccluded rect) are that case. A small extents table, one `vec4f` per piece
that differs, is indexed from the `Instance` row's spare `_pad` word (0 = no
extent, the box is the quad); the vertex stage places the quad from the extent
when present and evaluates `local` against `bounds` either way. The 80-byte row,
its stride tests and `InstancesPerBatch` are unchanged.

Run: `cargo nextest run -p goldens` (lean painter goldens within tolerance: a
piece boundary inside an anti-aliased edge is the case to watch; the band inset
in `cores` is what keeps it out) and `just unity-render`.

- [ ] **Step 5: The shaded-area instrument, extended to pieces (failing first)**

`crates/dashscene-gpu/tests/shaded_area.rs` exists since PR #1417: it builds
each showcase scene, pulses, ticks, runs `GpuPainter::paint` on the committed
tables and sums every instance's `bounds` grown by `outset`, clipped to the
extent, pinning one number per scene in the sanity tier. Once `render.rs` packs
pieces, that sum moves by construction, and
`every_showcase_scene_shades_what_it_did` goes red — that red is the failing
test, and the new pinned figures are the after-column. Add beside it a test that
pins the **after-sum per scene** exactly, as the before-sums are pinned today,
and bounds the **reduction**: on each scene the sum over rects minus the sum
over pieces is greater than zero and at most the derivation's figure (1.364 Mpx
on `surfaces`, 0.450 on `typography`, 2.841 on `layout`,
`probe-1412/rejected-2340x1080.txt`), which is an upper bound — that derivation
inset cores by one pixel and applied no radius or clip, and its base area is its
own, not this instrument's. The exact figure is the after-sum; the derivation is
context, recorded beside it.

Run before Step 2's commit change: FAIL (no pieces; or areas equal the before
figure). After: PASS. Mutation: make `pieces()` return one piece per rect (no
subtraction) — the after-figures fail.

- [ ] **Step 6: The device**

Three scenes, `frameReady` cadence before and after, `dumpsys` dumps under
`driftsys/dashscene-v021-lanes/probe-1450/`, recorded in
`docs/design/android-toolchain.md` beside the R-T2 reading with one sentence
relating the measured saving to the derived area.

- [ ] **Step 7: Commit and ship**

```bash
just test && just c-abi && git add -A && git commit -m "feat(paint): an occlusion pass at commit hands both painters the visible pieces of each rect" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

`just unity-abi`, `just unity-ffi`, `just unity-editor`, `just unity-render`,
`just build`; the PR names the ABI bump in its first line.

---

### Task 10: The comparison table and the close (story S10)

Worktree `v021-canvas-parity-table`, branch `story/v021-canvas-parity-table`.
Depends on every other story being on `main`.

**Files:**

- Modify: `docs/design/android-toolchain.md`, `docs/features.md`,
  `docs/roadmap.md`, `docs/wip/README.md`; move
  `docs/wip/2026-09-05-unity-painter-beside-a-faithful-canvas.md` and
  `…-plan.md` to `docs/archive/`.

- [ ] **Step 1: One session on the device**

One player at `main`, kept as the last of the shelf's APKs so no story before it
rebuilt a "before". For each of `surfaces`, `typography`, `layout`, and each
renderer `painter`, `canvas`, `none`: at rest (wait for the transition to settle
— the thread-cost line's main-thread value stops moving — then a 10 s window)
and during a transition (alternate `keyevent 21` and `keyevent 22` — the
showcase binds the arrows to `DriveSignal(0)` and `DriveSignal(1)` — every
second across the window, so a spring is in flight throughout; `keyevent 22`
alone drives the signal to the top once and the scene settles):
`unity-frame-cost.sh`, `gpu-capture.sh` with `DS_GPU_WINDOW=10`, the `--latency`
dump through `latency-cadence.py`, the `dashscene-cpu` records. Then
`demo-android` on the same device, same day,
`ADB=$(just _android-adb) ./measure/android/run.sh` for the lean painter's row.
Everything under `driftsys/dashscene-v021-lanes/probe-1451/`.

- [ ] **Step 2: The table**

In `docs/design/android-toolchain.md`, a section "The Unity painter beside a
faithful Canvas (date)": one row per scene × state × renderer with `frameReady`
mean, `averageFPS`, main-thread mean above the floor, render-thread mean,
process CPU per presented frame; the floor row; the lean painter's row; and
under it the criterion's three clauses from the record's D1 read as met or not
met per scene, each citing the dump. A row that fails names it; the owner rules.

- [ ] **Step 3: The records and the archive**

`docs/features.md`: the painter's row says what is measured. `docs/roadmap.md`:
the epic's close line under v0.21. `docs/wip/README.md`: the two rows leave, the
count is re-derived from
`git ls-files docs/wip/ | grep -v 'README.md$' | wc -l`, and the narrative
paragraph says the spec and plan archived with the epic's last story. `git mv`
both files to `docs/archive/`.

- [ ] **Step 4: Commit and ship**

```bash
just prim && just test && git add -A docs && git commit -m "docs(measure): the Unity painter beside a faithful Canvas on the Pixel 5, and the epic's close" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

Tell #1347 by comment with the table's row. The epic closes on the owner's
reading of the table; the phase-end revision records the closed count beside the
planned ten.

---

## Self-review

**Spec coverage.** §2 → Task 1 D1, Task 10. §3 → Task 1 D2, Task 2. §4 → Task 3
(instrument, floor), Task 2 (one player, one key), Task 4 (zero-alloc in the
gate). §5.1 → Task 4. §5.2 → Task 5. §6.1 → Task 6. §6.2 → Task 7. §6.3 → Task
8, including the specialisation. §6.4 → Task 9, including the shaded-area
instrument. §6.5 → Task 3. §7 → nothing to build; Task 0's epic body states it.
§8 → Task 0 and the ordering above. §9, §10 → Task 1's record and the R-T2
paragraph; the `DsFrame` risk is Task 9's Step 2 and 3.

**Placeholders.** None: Task 0 has run and every number in prose is the one read
back from GitHub; no step says "TBD". Task 2's `CanvasScene` shows the two
gate-read methods and names the four remaining methods with their inputs; Task
7's `DsLoad` is named with its contract. Each is written against the tree at
story start, per the rule in "How this plan is organised".

**Type consistency.** `InstanceSpans.Span { Offset, Count }` and `StreamLayout`
(Task 5) are what Task 7's ordered path and Task 9's `TryPackDirty` fallback
reuse. `Piece { rect, x, y, w, h }` (Task 9) is declared once in `dashpaint`,
mirrored in `Runtime/BoundaryB.cs`, and measured by `dashpaint-abi`.
`KindSet
{ clips, strokes }` (Task 8) is one function in `dashpaint`, read per
commit by both painters. `DashsceneThreadCost.Push(string, int, int)` (Task 3)
is what Task 2's baseline and the showcase call from the same phase.
`SettleLoop` (Task 4) is a bool and an extent, consumed by the three host loops.
