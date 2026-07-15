# dashlang flex builder + Scene::build_with(solver) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `dashlang`'s `Node` builder the v0.2 flex vocabulary and a
`Scene::build_with(arena, solver)` entry point, with `build`/`build_with`
returning a small `Built` handle instead of a bare `u64` so issue #166's
reactive layer can extend it later without reshaping the builder twice.

**Architecture:** `Node` embeds a `dashscene_core::Layout` (the same
struct core already uses) instead of four separate geometry fields, and
gains one chainable setter per `Prop` variant (bundling only where
`Prop` itself bundles, or where an existing `at`/`size` precedent
already bundles a 2D pair). `Scene::build`/`build_with` share a private
`stage` helper that opens a `Txn`, walks the value tree, and commits —
`build` via `Txn::commit()`, `build_with` via `Txn::commit_with(solver)`
— both wrapping the resulting generation in `Built`.

**Tech Stack:** Rust 2024 edition, Cargo workspace. No new crate
dependencies anywhere in this plan.

## Global Constraints

- `dashlang` keeps its core-only dependency: `crates/dashlang/Cargo.toml`
  must not gain `dashscene-engine`, as a `[dependencies]` or
  `[dev-dependencies]` entry. Verify with `cargo tree -p dashlang`.
- `just build` must stay green after every task (it runs the full
  assemble + check CI runs).
- Vocabulary: the IR is called "the dashscene document"; `.dsb` is its
  file extension. Never write "DSB"/"SCD".
- Every task's diff should trace directly to this plan — no unrelated
  refactoring or formatting drift.
- `build`/`build_with` do not call `Txn::lower_negative_gaps`
  automatically — that stays a separate, explicit producer step against
  the arena `dashlang` built into (design doc D5). None of the ported
  scenes need it; do not add it.

---

## File Map

- Modify: `crates/dashlang/src/lib.rs` — `Node`'s flex vocabulary,
  `Built`, `Scene::build`/`build_with`.
- Modify: `crates/dashlang/tests/builder.rs` — flex-vocabulary unit
  test, `build_with` unit test, fixed call sites for the new `Built`
  return type.
- Modify: `goldens/tooling/tests/v02_flex.rs` — DSL-built assertions
  added to all four existing tests; module doc corrected.
- Modify: `docs/design/dashlang.md` — as-built update (Task 4).

---

### Task 1: Flex vocabulary on `Node`

**Files:**

- Modify: `crates/dashlang/src/lib.rs`
- Test: `crates/dashlang/tests/builder.rs`

**Interfaces:**

- Produces: `Node::mode(LayoutMode)`, `Node::gap(f32)`,
  `Node::padding(f32, f32, f32, f32)`, `Node::margin(f32, f32, f32,
  f32)`, `Node::main_align(MainAxisAlign)`,
  `Node::cross_align(CrossAxisAlign)`, `Node::sizing_h(AxisSizing)`,
  `Node::sizing_v(AxisSizing)`, `Node::min_width(f32)`,
  `Node::max_width(f32)`, `Node::min_height(f32)`,
  `Node::max_height(f32)` — all consuming, chainable (`mut self ->
  Self`), matching `at`/`size`/`fill`. Re-exports `LayoutMode`,
  `AxisSizing`, `MainAxisAlign`, `CrossAxisAlign` from `dashlang`
  itself.

- [ ] **Step 1: Write the failing test**

  Add to `crates/dashlang/tests/builder.rs` (append as a new `#[test]`
  function; add `AxisSizing, CrossAxisAlign, LayoutMode, MainAxisAlign`
  to the existing `use dashlang::{...}` import line at the top of the
  file):

  ```rust
  #[test]
  fn flex_vocabulary_reaches_the_arena_layout() {
      let mut dsl = Arena::new();
      scene([node("row")
          .mode(LayoutMode::Horizontal)
          .gap(8.0)
          .padding(1.0, 2.0, 3.0, 4.0)
          .margin(5.0, 6.0, 7.0, 8.0)
          .main_align(MainAxisAlign::Center)
          .cross_align(CrossAxisAlign::End)
          .sizing_h(AxisSizing::Hug)
          .sizing_v(AxisSizing::Fill)
          .min_width(10.0)
          .max_width(100.0)
          .min_height(20.0)
          .max_height(200.0)])
      .build(&mut dsl);

      let root = dsl.roots()[0];
      let layout = dsl.layout(root);
      assert_eq!(layout.mode, LayoutMode::Horizontal);
      assert_eq!(layout.gap, 8.0);
      assert_eq!(layout.padding.left, 1.0);
      assert_eq!(layout.padding.top, 2.0);
      assert_eq!(layout.padding.right, 3.0);
      assert_eq!(layout.padding.bottom, 4.0);
      assert_eq!(layout.margin.left, 5.0);
      assert_eq!(layout.margin.top, 6.0);
      assert_eq!(layout.margin.right, 7.0);
      assert_eq!(layout.margin.bottom, 8.0);
      assert_eq!(layout.main_align, MainAxisAlign::Center);
      assert_eq!(layout.cross_align, CrossAxisAlign::End);
      assert_eq!(layout.sizing_h, AxisSizing::Hug);
      assert_eq!(layout.sizing_v, AxisSizing::Fill);
      assert_eq!(layout.min_width, Some(10.0));
      assert_eq!(layout.max_width, Some(100.0));
      assert_eq!(layout.min_height, Some(20.0));
      assert_eq!(layout.max_height, Some(200.0));
  }
  ```

- [ ] **Step 2: Run test to verify it fails**

  Run: `cargo test -p dashlang --test builder flex_vocabulary_reaches_the_arena_layout`
  Expected: FAIL to compile — `mode`/`gap`/`padding`/etc. are not
  methods on `Node`, and `AxisSizing`/`CrossAxisAlign`/`LayoutMode`/
  `MainAxisAlign` are not exported from `dashlang`.

- [ ] **Step 3: Write the implementation**

  In `crates/dashlang/src/lib.rs`, replace the import block:

  ```rust
  use dashscene_core::{NodeId, Prop, Txn};

  // A DSL consumer needs an `Arena` to build into and a `Color` to fill
  // with; re-exporting both keeps `dashlang` a one-import-path surface
  // (no direct `dashscene-core` dependency required downstream).
  pub use dashscene_core::{Arena, Color};
  ```

  with:

  ```rust
  use dashscene_core::{EdgeInsets, Layout, NodeId, Prop, Txn};

  // A DSL consumer needs an `Arena` to build into, a `Color` to fill
  // with, and the v0.2 flex vocabulary's enums; re-exporting all of
  // them keeps `dashlang` a one-import-path surface (no direct
  // `dashscene-core` dependency required downstream).
  pub use dashscene_core::{Arena, AxisSizing, Color, CrossAxisAlign, LayoutMode, MainAxisAlign};
  ```

  Replace the `Node` struct:

  ```rust
  #[derive(Debug, Default)]
  pub struct Node {
      name: Option<String>,
      x: f32,
      y: f32,
      width: f32,
      height: f32,
      fill: Option<Color>,
      children: Vec<Node>,
  }
  ```

  with:

  ```rust
  #[derive(Debug, Default)]
  pub struct Node {
      name: Option<String>,
      layout: Layout,
      fill: Option<Color>,
      children: Vec<Node>,
  }
  ```

  Replace the `at`/`size` methods:

  ```rust
  pub fn at(mut self, x: f32, y: f32) -> Self {
      self.x = x;
      self.y = y;
      self
  }

  pub fn size(mut self, width: f32, height: f32) -> Self {
      self.width = width;
      self.height = height;
      self
  }
  ```

  with:

  ```rust
  pub fn at(mut self, x: f32, y: f32) -> Self {
      self.layout.x = x;
      self.layout.y = y;
      self
  }

  pub fn size(mut self, width: f32, height: f32) -> Self {
      self.layout.width = width;
      self.layout.height = height;
      self
  }

  /// Container layout mode: `None` (passthrough — children place by
  /// their authored offsets), or `Horizontal`/`Vertical` flex.
  pub fn mode(mut self, mode: LayoutMode) -> Self {
      self.layout.mode = mode;
      self
  }

  /// Gap between children along the main axis, under a flex mode.
  pub fn gap(mut self, gap: f32) -> Self {
      self.layout.gap = gap;
      self
  }

  /// Inner padding, all four edges.
  pub fn padding(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
      self.layout.padding = EdgeInsets { left, top, right, bottom };
      self
  }

  /// Outer margin in the parent's flow, all four edges. Flex-flow
  /// vocabulary only: inert on a root or under a mode-`None` parent.
  pub fn margin(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
      self.layout.margin = EdgeInsets { left, top, right, bottom };
      self
  }

  /// Alignment along the container's main axis.
  pub fn main_align(mut self, align: MainAxisAlign) -> Self {
      self.layout.main_align = align;
      self
  }

  /// Alignment along the container's cross axis.
  pub fn cross_align(mut self, align: CrossAxisAlign) -> Self {
      self.layout.cross_align = align;
      self
  }

  /// How this node sizes itself horizontally under a flex parent.
  pub fn sizing_h(mut self, sizing: AxisSizing) -> Self {
      self.layout.sizing_h = sizing;
      self
  }

  /// How this node sizes itself vertically under a flex parent.
  pub fn sizing_v(mut self, sizing: AxisSizing) -> Self {
      self.layout.sizing_v = sizing;
      self
  }

  /// Minimum width clamp. Cannot be unset once set — core has no clear
  /// operation for it (same gap as `fill`).
  pub fn min_width(mut self, v: f32) -> Self {
      self.layout.min_width = Some(v);
      self
  }

  /// Maximum width clamp. Cannot be unset once set.
  pub fn max_width(mut self, v: f32) -> Self {
      self.layout.max_width = Some(v);
      self
  }

  /// Minimum height clamp. Cannot be unset once set.
  pub fn min_height(mut self, v: f32) -> Self {
      self.layout.min_height = Some(v);
      self
  }

  /// Maximum height clamp. Cannot be unset once set.
  pub fn max_height(mut self, v: f32) -> Self {
      self.layout.max_height = Some(v);
      self
  }
  ```

  Replace the `add` function body's X/Y/Width/Height sets:

  ```rust
  fn add(txn: &mut Txn<'_>, parent: Option<NodeId>, node: &Node) {
      let id = txn.add_node(parent, node.name.as_deref());
      txn.set_prop(id, Prop::X(node.x));
      txn.set_prop(id, Prop::Y(node.y));
      txn.set_prop(id, Prop::Width(node.width));
      txn.set_prop(id, Prop::Height(node.height));
      if let Some(color) = node.fill {
          txn.set_prop(id, Prop::Fill(color));
      }
      for child in &node.children {
          add(txn, Some(id), child);
      }
  }
  ```

  with:

  ```rust
  fn add(txn: &mut Txn<'_>, parent: Option<NodeId>, node: &Node) {
      let id = txn.add_node(parent, node.name.as_deref());
      txn.set_prop(id, Prop::X(node.layout.x));
      txn.set_prop(id, Prop::Y(node.layout.y));
      txn.set_prop(id, Prop::Width(node.layout.width));
      txn.set_prop(id, Prop::Height(node.layout.height));
      txn.set_prop(id, Prop::Mode(node.layout.mode));
      txn.set_prop(id, Prop::Gap(node.layout.gap));
      txn.set_prop(
          id,
          Prop::Padding {
              left: node.layout.padding.left,
              top: node.layout.padding.top,
              right: node.layout.padding.right,
              bottom: node.layout.padding.bottom,
          },
      );
      txn.set_prop(
          id,
          Prop::Margin {
              left: node.layout.margin.left,
              top: node.layout.margin.top,
              right: node.layout.margin.right,
              bottom: node.layout.margin.bottom,
          },
      );
      txn.set_prop(id, Prop::MainAlign(node.layout.main_align));
      txn.set_prop(id, Prop::CrossAlign(node.layout.cross_align));
      txn.set_prop(id, Prop::SizingH(node.layout.sizing_h));
      txn.set_prop(id, Prop::SizingV(node.layout.sizing_v));
      if let Some(v) = node.layout.min_width {
          txn.set_prop(id, Prop::MinWidth(v));
      }
      if let Some(v) = node.layout.max_width {
          txn.set_prop(id, Prop::MaxWidth(v));
      }
      if let Some(v) = node.layout.min_height {
          txn.set_prop(id, Prop::MinHeight(v));
      }
      if let Some(v) = node.layout.max_height {
          txn.set_prop(id, Prop::MaxHeight(v));
      }
      if let Some(color) = node.fill {
          txn.set_prop(id, Prop::Fill(color));
      }
      for child in &node.children {
          add(txn, Some(id), child);
      }
  }
  ```

- [ ] **Step 4: Run test to verify it passes**

  Run: `cargo test -p dashlang`
  Expected: PASS, all tests including the new one and the existing
  v0.1 acceptance tests (`the_dsl_scene_matches_the_hand_built_scene`,
  `repeater_children_come_from_an_iterator_in_order`,
  `multiple_roots_keep_declaration_order`,
  `build_appends_to_a_non_empty_arena_and_commits_exactly_once`,
  `unset_fill_and_geometry_keep_core_defaults`).

- [ ] **Step 5: Commit**

  ```bash
  git add crates/dashlang/src/lib.rs crates/dashlang/tests/builder.rs
  git commit -m "feat(dashlang): add the v0.2 flex vocabulary to Node"
  ```

---

### Task 2: `Built` and `Scene::build_with`

**Files:**

- Modify: `crates/dashlang/src/lib.rs`
- Test: `crates/dashlang/tests/builder.rs`

**Interfaces:**

- Consumes: `Node`, `Scene` from Task 1 (unchanged shape).
- Produces: `pub struct Built { generation: u64 }` with `pub fn
  generation(self) -> u64`; `Scene::build(&self, arena: &mut Arena) ->
  Built` (changed from `-> u64`); `Scene::build_with(&self, arena: &mut
  Arena, solver: &mut dyn LayoutSolver) -> Built`.

- [ ] **Step 1: Write the failing test**

  Add to `crates/dashlang/tests/builder.rs`. Add `LayoutSolver, NodeId,
  SolvedRect` to the existing `use dashscene_core::{PaintEntry, Prop};`
  line (making it `use dashscene_core::{LayoutSolver, NodeId, PaintEntry,
  Prop, SolvedRect};`):

  ```rust
  struct DoubleWidthSolver;

  impl LayoutSolver for DoubleWidthSolver {
      fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
          arena
              .roots()
              .iter()
              .copied()
              .map(|id| {
                  let layout = arena.layout(id);
                  (
                      id,
                      SolvedRect {
                          x: layout.x,
                          y: layout.y,
                          w: layout.width * 2.0,
                          h: layout.height,
                      },
                  )
              })
              .collect()
      }
  }

  #[test]
  fn build_with_routes_through_the_injected_solver() {
      let mut arena = Arena::new();
      let built = scene([node("only").size(10.0, 20.0)])
          .build_with(&mut arena, &mut DoubleWidthSolver);

      assert_eq!(built.generation(), 1);
      assert_eq!(arena.committed().rects()[0].w, 20.0);
  }
  ```

- [ ] **Step 2: Run test to verify it fails**

  Run: `cargo test -p dashlang --test builder build_with_routes_through_the_injected_solver`
  Expected: FAIL to compile — `build_with` and `Built::generation` do
  not exist yet.

- [ ] **Step 3: Write the implementation**

  In `crates/dashlang/src/lib.rs`, add `LayoutSolver` to the private
  import line (`use dashscene_core::{EdgeInsets, Layout, LayoutSolver,
  NodeId, Prop, Txn};`).

  Add, above `impl Scene`:

  ```rust
  /// The result of one [`Scene::build`]/[`Scene::build_with`] commit. A
  /// thin wrapper around the commit generation today — the seam issue
  /// #166's reactive layer extends into a live, bindable scene handle
  /// without a second change to `build`'s return type
  /// (`docs/wip/2026-07-15-flex-builder-design.md` D3).
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub struct Built {
      generation: u64,
  }

  impl Built {
      /// The commit's generation (`CommittedScene::generation`).
      pub fn generation(self) -> u64 {
          self.generation
      }
  }
  ```

  Replace `impl Scene`'s `build` method:

  ```rust
  impl Scene {
      /// Adds this description's roots to `arena` (appending to whatever
      /// the arena already holds — the DSL is a producer, not an owner)
      /// and publishes them in exactly one commit. Returns the commit's
      /// result.
      ///
      /// An empty scene still commits: the generation increments, and
      /// changes staged by a previously dropped `Txn` publish (core's
      /// batched-publish staging).
      pub fn build(&self, arena: &mut Arena) -> u64 {
          let mut txn = arena.open();
          for root in &self.roots {
              add(&mut txn, None, root);
          }
          txn.commit()
      }
  }
  ```

  with:

  ```rust
  impl Scene {
      /// Adds this description's roots to `arena` (appending to whatever
      /// the arena already holds — the DSL is a producer, not an owner)
      /// and publishes them in exactly one commit, using core's internal
      /// fixed-geometry resolution (flex intent ignored). A scene with
      /// flex intent commits through [`Scene::build_with`] and a real
      /// solver.
      ///
      /// An empty scene still commits: the generation increments, and
      /// changes staged by a previously dropped `Txn` publish (core's
      /// batched-publish staging).
      pub fn build(&self, arena: &mut Arena) -> Built {
          Built { generation: self.stage(arena).commit() }
      }

      /// Adds this description's roots to `arena` and publishes them in
      /// exactly one commit, using `solver` for every node's geometry —
      /// the entry point a flex scene needs (`dashscene-engine`'s
      /// `TaffySolver`, injected by the caller so `dashlang` itself never
      /// depends on the engine).
      pub fn build_with(&self, arena: &mut Arena, solver: &mut dyn LayoutSolver) -> Built {
          Built { generation: self.stage(arena).commit_with(solver) }
      }

      fn stage<'a>(&self, arena: &'a mut Arena) -> Txn<'a> {
          let mut txn = arena.open();
          for root in &self.roots {
              add(&mut txn, None, root);
          }
          txn
      }
  }
  ```

  Update the crate-doc example at the top of `lib.rs`:

  ```rust
  //! let generation = scene([
  //!     node("bg")
  //!         .size(320.0, 240.0)
  //!         .fill(rgba(0.1, 0.2, 0.3, 1.0))
  //!         .children((0..3).map(|i| {
  //!             node("badge")
  //!                 .at(10.0 + 30.0 * i as f32, 10.0)
  //!                 .size(24.0, 24.0)
  //!                 .fill(rgba(1.0, 0.0, 0.0, 1.0))
  //!         })),
  //! ])
  //! .build(&mut arena);
  //!
  //! assert_eq!(arena.committed().generation(), generation);
  ```

  with:

  ```rust
  //! let built = scene([
  //!     node("bg")
  //!         .size(320.0, 240.0)
  //!         .fill(rgba(0.1, 0.2, 0.3, 1.0))
  //!         .children((0..3).map(|i| {
  //!             node("badge")
  //!                 .at(10.0 + 30.0 * i as f32, 10.0)
  //!                 .size(24.0, 24.0)
  //!                 .fill(rgba(1.0, 0.0, 0.0, 1.0))
  //!         })),
  //! ])
  //! .build(&mut arena);
  //!
  //! assert_eq!(arena.committed().generation(), built.generation());
  ```

  In `crates/dashlang/tests/builder.rs`, update
  `build_appends_to_a_non_empty_arena_and_commits_exactly_once`:

  ```rust
  let generation = scene([node("second").size(2.0, 2.0)]).build(&mut arena);

  assert_eq!(generation, 2);
  ```

  with:

  ```rust
  let built = scene([node("second").size(2.0, 2.0)]).build(&mut arena);

  assert_eq!(built.generation(), 2);
  ```

- [ ] **Step 4: Run test to verify it passes**

  Run: `cargo test -p dashlang`
  Expected: PASS, all tests.

- [ ] **Step 5: Run the doc test**

  Run: `cargo test -p dashlang --doc`
  Expected: PASS.

- [ ] **Step 6: Commit**

  ```bash
  git add crates/dashlang/src/lib.rs crates/dashlang/tests/builder.rs
  git commit -m "feat(dashlang): add Built and Scene::build_with(solver)"
  ```

---

### Task 3: Port the four v0.2 flex goldens onto the DSL

**Files:**

- Modify: `goldens/tooling/tests/v02_flex.rs`

**Interfaces:**

- Consumes: `Node::{mode, gap, padding, margin, main_align,
  cross_align, sizing_h, sizing_v, min_width, max_width, min_height,
  max_height, at, size, fill, child, children}`, `Scene::build_with`,
  `Built` (Tasks 1–2); `dashlang::{Node, anon, node, scene}`;
  `dashscene_engine::TaffySolver::new()`.

- [ ] **Step 1: Update the module doc and imports**

  Replace the module doc comment's second paragraph:

  ```rust
  //! Scenes are authored against dashscene-core's `Txn` and solved by
  //! dashscene-engine's `TaffySolver`. dashlang is not used: its builder
  //! has no flex vocabulary and `Scene::build` commits through the fixed
  //! solver, which ignores flex (`docs/decisions/negative-gap-lowering.md`
  //! D3).
  ```

  with:

  ```rust
  //! Scenes are authored against dashscene-core's `Txn` and solved by
  //! dashscene-engine's `TaffySolver`. Each test also builds the same
  //! scene through `dashlang`'s flex vocabulary and `Scene::build_with`,
  //! and asserts the two commits produce identical rects (issue #118;
  //! `docs/decisions/negative-gap-lowering.md` D3 recorded the original
  //! deferral).
  ```

  Add `dashlang` to the imports:

  ```rust
  use dashlang::{Node, anon, node, scene};
  use dashpaint::{ImageTable, Painter};
  use dashscene_core::{
      Arena, AxisSizing, Color, CrossAxisAlign, LayoutMode, MainAxisAlign, NodeId, Prop, Txn,
  };
  use dashscene_engine::TaffySolver;
  use dashscene_skia::SkiaPainter;
  ```

- [ ] **Step 2: Add the DSL side to `nesting_matches_its_golden`**

  Immediately before the `render_and_compare(&arena, "v02-nesting");`
  line at the end of the test, insert:

  ```rust
  let mut dsl = Arena::new();
  let dsl_column = |fill: Color, cells: [Color; 2]| {
      node("column")
          .size(50.0, 70.0)
          .mode(LayoutMode::Vertical)
          .gap(10.0)
          .fill(fill)
          .children(cells.into_iter().map(|cell| anon().size(50.0, 30.0).fill(cell)))
  };
  scene([node("root")
      .size(120.0, 80.0)
      .mode(LayoutMode::Horizontal)
      .gap(10.0)
      .padding(5.0, 5.0, 5.0, 5.0)
      .fill(NAVY)
      .child(dsl_column(RED, [GOLD, GREEN]))
      .child(dsl_column(BLUE, [GREEN, GOLD]))])
  .build_with(&mut dsl, &mut TaffySolver::new());

  assert_eq!(dsl.committed().rects(), arena.committed().rects());
  ```

- [ ] **Step 3: Add the DSL side to `sizing_matches_its_golden`**

  Immediately before `render_and_compare(&arena, "v02-sizing");`,
  insert:

  ```rust
  let mut dsl = Arena::new();
  scene([node("root")
      .size(120.0, 60.0)
      .mode(LayoutMode::Horizontal)
      .fill(NAVY)
      .child(
          node("hug")
              .mode(LayoutMode::Horizontal)
              .sizing_h(AxisSizing::Hug)
              .size(0.0, 60.0)
              .fill(RED)
              .child(anon().size(30.0, 40.0).fill(GOLD)),
      )
      .children(
          [GREEN, BLUE]
              .into_iter()
              .map(|color| anon().sizing_h(AxisSizing::Fill).size(0.0, 60.0).fill(color)),
      )])
  .build_with(&mut dsl, &mut TaffySolver::new());

  assert_eq!(dsl.committed().rects(), arena.committed().rects());
  ```

- [ ] **Step 4: Add the DSL side to `clamping_matches_its_golden`**

  Add this helper function at module level, next to `clamped_row`:

  ```rust
  fn dsl_clamped_row(clamp: impl FnOnce(Node) -> Node, first: Color, second: Color) -> Node {
      node("row")
          .size(120.0, 30.0)
          .mode(LayoutMode::Horizontal)
          .child(clamp(anon().sizing_h(AxisSizing::Fill).size(0.0, 30.0).fill(first)))
          .child(anon().sizing_h(AxisSizing::Fill).size(0.0, 30.0).fill(second))
  }
  ```

  Immediately before `render_and_compare(&arena, "v02-clamping");`,
  insert:

  ```rust
  let mut dsl = Arena::new();
  scene([node("root")
      .size(120.0, 60.0)
      .mode(LayoutMode::Vertical)
      .fill(NAVY)
      .child(dsl_clamped_row(|n| n.max_width(40.0), RED, GREEN))
      .child(dsl_clamped_row(|n| n.min_width(100.0), GOLD, BLUE))])
  .build_with(&mut dsl, &mut TaffySolver::new());

  assert_eq!(dsl.committed().rects(), arena.committed().rects());
  ```

- [ ] **Step 5: Add the DSL side to `alignment_matches_its_golden`**

  Add this helper function at module level, next to `align_row`:

  ```rust
  fn dsl_align_row(
      main: MainAxisAlign,
      cross: CrossAxisAlign,
      padding: Option<(f32, f32, f32, f32)>,
      colors: [Color; 2],
  ) -> Node {
      let row = node("row")
          .size(160.0, 20.0)
          .mode(LayoutMode::Horizontal)
          .gap(10.0)
          .main_align(main)
          .cross_align(cross)
          .children(colors.into_iter().map(|c| anon().size(30.0, 10.0).fill(c)));
      match padding {
          Some((left, top, right, bottom)) => row.padding(left, top, right, bottom),
          None => row,
      }
  }
  ```

  Immediately before `render_and_compare(&arena, "v02-alignment");`,
  insert:

  ```rust
  let mut dsl = Arena::new();
  scene([node("root")
      .size(160.0, 80.0)
      .mode(LayoutMode::Vertical)
      .fill(NAVY)
      .child(dsl_align_row(
          MainAxisAlign::Start,
          CrossAxisAlign::Start,
          Some((10.0, 2.0, 10.0, 2.0)),
          [RED, GOLD],
      ))
      .child(dsl_align_row(MainAxisAlign::Center, CrossAxisAlign::Center, None, [GREEN, BLUE]))
      .child(dsl_align_row(MainAxisAlign::End, CrossAxisAlign::End, None, [GOLD, RED]))
      .child(dsl_align_row(
          MainAxisAlign::SpaceBetween,
          CrossAxisAlign::Center,
          None,
          [BLUE, GREEN],
      ))])
  .build_with(&mut dsl, &mut TaffySolver::new());

  assert_eq!(dsl.committed().rects(), arena.committed().rects());
  ```

- [ ] **Step 6: Run the tests to verify they pass**

  Run: `cargo test -p goldens --test v02_flex`
  Expected: PASS, all four tests. If a `dsl.committed().rects()`
  assertion fails, the mismatch is between Task 1/2's vocabulary
  mapping and this scene's construction — compare the failing rect
  index against the corresponding hand-built `assert_eq!` a few lines
  above it in the same test to localize which prop diverged.

- [ ] **Step 7: Verify `dashlang` still has no engine dependency**

  Run: `cargo tree -p dashlang`
  Expected: only `dashscene-core` (and its own transitive deps:
  `dashbuf`, `dashpaint`) — no `dashscene-engine` anywhere in the
  output.

- [ ] **Step 8: Commit**

  ```bash
  git add goldens/tooling/tests/v02_flex.rs
  git commit -m "test(goldens): port the v0.2 flex goldens onto the dashlang DSL"
  ```

---

### Task 4: Update `docs/design/dashlang.md`

**Files:**

- Modify: `docs/design/dashlang.md`

**Interfaces:**

- Consumes: the shipped API from Tasks 1–3 (no new interfaces
  produced — this is a documentation-only task).

- [ ] **Step 1: Update the "Value-tree surface" section**

  Replace the `Node` bullet:

  ```markdown
  - `Node` — consuming, chainable setters: `at(x, y)` (authored offset,
    parent-relative), `size(w, h)`, `fill(Color)`, `child(Node)` (append
    one), `children(impl IntoIterator<Item = Node>)` (append from an
    iterator). Declaration order is document (DFS) order — core pins
    sibling order to creation order.
  ```

  with:

  ```markdown
  - `Node` — consuming, chainable setters: `at(x, y)` (authored offset,
    parent-relative), `size(w, h)`, `fill(Color)`, `child(Node)` (append
    one), `children(impl IntoIterator<Item = Node>)` (append from an
    iterator). Declaration order is document (DFS) order — core pins
    sibling order to creation order. The v0.2 flex vocabulary (issue
    #118): `mode(LayoutMode)`, `gap(f32)`, `padding(left, top, right,
    bottom: f32)`, `margin(left, top, right, bottom: f32)`,
    `main_align(MainAxisAlign)`, `cross_align(CrossAxisAlign)`,
    `sizing_h(AxisSizing)`, `sizing_v(AxisSizing)`, `min_width(f32)`,
    `max_width(f32)`, `min_height(f32)`, `max_height(f32)` — one method
    per `Prop` variant, mirroring `dashscene_core::Layout`, which `Node`
    embeds directly rather than duplicating its fields.
  ```

- [ ] **Step 2: Update the "Build/commit mapping" section**

  Replace:

  ```markdown
  ## Build/commit mapping

  `Scene::build(&mut Arena) -> u64` is the DSL's only point of contact
  with `dashscene-core`: it opens one `Txn`, walks the value tree
  depth-first (a private recursive `add` — `add_node` then `set_prop`
  for `X`/`Y`/`Width`/`Height` and, if set, `Fill`, then recurse into
  children), and commits exactly once, returning the commit's
  generation. `build` _adds_ its roots to whatever the arena already
  holds — the DSL is a producer, not an owner — matching the one-commit
  model the future C# describe-buffer skin will use across its FFI
  seam.
  ```

  with:

  ```markdown
  ## Build/commit mapping

  `Scene::build(&mut Arena) -> Built` and `Scene::build_with(&mut Arena,
  &mut dyn LayoutSolver) -> Built` are the DSL's points of contact with
  `dashscene-core`: both open one `Txn`, walk the value tree depth-first
  (a private recursive `add` — `add_node` then `set_prop` for every
  `Layout` field and, if set, `Fill`, then recurse into children), and
  commit exactly once — `build` via `Txn::commit()` (the fixed solver,
  flex intent ignored), `build_with` via `Txn::commit_with(solver)` (a
  real solver, `dashscene-engine`'s `TaffySolver` being the product
  case). Both _add_ their roots to whatever the arena already holds —
  the DSL is a producer, not an owner — matching the one-commit model
  the future C# describe-buffer skin will use across its FFI seam.

  `Built` wraps the commit's generation (`Built::generation() -> u64`).
  It is deliberately a named type rather than a bare `u64`: issue #166's
  reactive layer extends it into a live, bindable scene handle without a
  second change to `build`'s signature
  (`docs/wip/2026-07-15-flex-builder-design.md` D3, or its gardened
  decision-record home once archived).
  ```

- [ ] **Step 3: Update the "Module layout" section**

  Replace:

  ```markdown
  crates/dashlang/tests/builder.rs acceptance (issue #5): DSL output
  == hand-built output; repeater
  children; multi-root; append to
  an existing arena; unset-value
  defaults
  ```

  with:

  ```markdown
  crates/dashlang/tests/builder.rs acceptance (issues #5, #118): DSL
  output == hand-built output;
  repeater children; multi-root;
  append to an existing arena;
  unset-value defaults; the flex
  vocabulary reaches the arena;
  build_with routes through an
  injected solver
  ```

- [ ] **Step 4: Update the "Trace" section**

  Replace:

  ```markdown
  ## Trace

  - Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §6.2 (Rust DSL
    skin); issue #5 acceptance criteria.
  - Blocks: #6 (golden harness); later DSL slices (the stress-corpus
    generator; v0.4 variants, once `dashcue` enters the graph).
  - Related decisions: `docs/decisions/dashlang-value-tree-builder.md`
    (this crate's surface shape); `docs/decisions/staged-mutation-v01-scope.md`
    (the `open`/`set_prop`/`commit` API this crate consumes).
  ```

  with:

  ```markdown
  ## Trace

  - Satisfies: `docs/archive/2026-07-14-design-1-seed.md` §6.2 (Rust DSL
    skin); issue #5 acceptance criteria; issue #118 acceptance criteria
    (flex vocabulary, `build_with`, the SCOPE §23 return-type seam).
  - Blocks: #6 (golden harness, done); #46 (the DSL-generated stress
    corpus, unblocked by the flex vocabulary); #166 (reactive bindings,
    which extends `Built` rather than reshaping `build`'s signature).
  - Related decisions: `docs/decisions/dashlang-value-tree-builder.md`
    (this crate's surface shape); `docs/decisions/staged-mutation-v01-scope.md`
    (the `open`/`set_prop`/`commit` API this crate consumes);
    `docs/decisions/flex-vocabulary-shape.md` (the core vocabulary this
    mirrors); `docs/decisions/negative-gap-lowering.md` D3 (the
    deferral #118 resolves).
  ```

- [ ] **Step 5: Run the full build**

  Run: `just build`
  Expected: green — this is what CI runs, covering every crate this
  plan touched plus lint and formatting.

- [ ] **Step 6: Commit**

  ```bash
  git add docs/design/dashlang.md
  git commit -m "docs(dashlang): describe the flex vocabulary and Built seam"
  ```

---

## After this plan

Not part of this plan's tasks, but required before the PR can be marked
ready per `AGENTS.md`'s story workflow:

- Run the `sdd-gardening` skill to move
  `docs/wip/2026-07-15-flex-builder-design.md` and
  `docs/wip/2026-07-15-flex-builder-plan.md` to `docs/archive/`, and
  fold D3's decision (the `Built` seam) into a durable
  `docs/decisions/` record — `docs/design/dashlang.md` (Task 4) already
  carries the as-built description, so gardening's job is the decision
  record and the archive move, not re-describing the API.
- `just build` green, draft PR opened, `/code-review` run, findings
  resolved or filed as `debt` issues, PR marked ready only once CI is
  green and the review pass is complete.
