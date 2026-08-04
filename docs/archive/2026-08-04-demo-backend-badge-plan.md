# Demo Backend Badge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Draw the name of the painter currently in use into the demo
window, so a screenshot of the showcase carries evidence of which painter
produced it.

**Architecture:** `corpus/showcase/` grows a badge module that returns a
second root node, which each of the three scenes appends to its root list.
One named scalar signal drives both the badge's text and its group
opacity. `demo/` writes that signal when it builds a scene and when the
`P` key swaps the painter. The signal's `0.0` default renders the badge
empty and fully transparent, which is what keeps it out of the still-image
example that produces the repository README's picture.

**Tech Stack:** Rust 2024, `dashlang` (the scene builder and its reactive
bindings), `dashscene-core` (the arena and `Channel`), `winit` (the demo
host's event loop). Task runner is `just`; tests run under
`cargo nextest`.

## Global Constraints

- The design record this implements is
  `docs/wip/2026-08-04-demo-backend-badge-design.md`. Read it first.
- Work in the worktree
  `/Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge`, on
  branch `story/demo-backend-badge`. Never edit the primary checkout at
  `.../dashscene-staging`.
- Run `just test` before every commit. It takes about 5 seconds.
- Run `just build` before opening the pull request. The diff touches
  `demo/` and `corpus/showcase/` only, which are outside the `packer`
  path filter, so the calibration tier is not required.
- Prose — comments, commit messages, documentation — is plain, literal
  English. No idioms.
- Commit messages are conventional commits, scoped to the crate:
  `feat(showcase): ...`, `feat(demo): ...`, `docs(docs): ...`.
- `corpus/showcase/` owns content. `demo/` owns the host. The host must
  not author nodes, colours or strings into a scene — that split is the
  reason this badge lives in `corpus/showcase/` at all
  (`corpus/showcase/src/lib.rs`, "What a scene tells the host about
  input").
- The two painter values are `1.0` for Skia and `2.0` for the GPU
  painter. `0.0` means no painter has been announced. These exact numbers
  appear in two crates and must agree.

---

### Task 1: The badge module in `corpus/showcase`

**Files:**

- Create: `corpus/showcase/src/badge.rs`
- Modify: `corpus/showcase/src/lib.rs` (add `pub mod badge;` beside the
  other module declarations, around line 72)
- Test: `corpus/showcase/tests/badge.rs`

**Interfaces:**

- Consumes: `dashlang::{Channel, Node, Scene, node}`,
  `dashscene_core::{TextAlign, TextAlignV}`,
  `crate::vocabulary::{palette, text_style}`,
  `crate::resources::LATIN_FAMILY`.
- Produces:
  - `showcase::badge::BACKEND: &str` — the signal name, value `"backend"`.
  - `showcase::badge::SKIA: f32` — `1.0`.
  - `showcase::badge::GPU: f32` — `2.0`.
  - `showcase::badge::label(value: f32) -> String` — the value-to-text
    mapping, public so a test can assert it without building a scene.
  - `showcase::badge::badge(scene: &mut Scene, width: f32, height: f32) -> Node`
    — declares the signal on `scene` and returns the badge root.

- [ ] **Step 1: Write the failing test**

Create `corpus/showcase/tests/badge.rs`:

```rust
//! The painter badge: the value-to-text mapping, and the badge's place in
//! a scene's root list.

use dashlang::{Arena, Scene};
use dashscene_engine::TaffySolver;
use showcase::badge;

/// The mapping the host drives. `0.0` is the unannounced state and must
/// render nothing, which is what keeps the badge out of the still that
/// produces the repository README's picture.
#[test]
fn each_value_names_its_painter_and_zero_names_nothing() {
    assert_eq!(badge::label(0.0), "");
    assert_eq!(badge::label(badge::SKIA), "dashscene-skia");
    assert_eq!(badge::label(badge::GPU), "dashscene-gpu");
}

/// The two announced values must differ, or the host cannot distinguish
/// the painters through the one signal it writes.
#[test]
fn the_two_painters_have_distinct_values() {
    assert_ne!(badge::SKIA, badge::GPU);
    assert_ne!(badge::label(badge::SKIA), badge::label(badge::GPU));
}

/// Built into a scene, the badge is committed empty and transparent, and
/// it is the last root — which is what paints it above the content.
#[test]
fn the_badge_builds_invisible_and_last() {
    let mut arena = Arena::new();
    let mut scene = Scene::new();
    let content = dashlang::node("content")
        .mode(dashlang::LayoutMode::None)
        .size(400.0, 300.0);
    let label = badge::badge(&mut scene, 400.0, 300.0);
    scene.roots([content, label]);
    let live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));
    drop(live);

    let roots = arena.roots();
    assert_eq!(roots.len(), 2, "the badge is a second root");
    let badge_id = roots[1];
    assert_eq!(arena.name(badge_id), Some("backend-badge"));
    assert_eq!(arena.text(badge_id), Some(""), "no painter announced yet");
    assert_eq!(arena.opacity(badge_id), 0.0, "invisible until announced");
}

/// Writing the signal changes the text and raises the badge, with no
/// rebuild — this is the path the `P` swap key takes.
#[test]
fn announcing_a_painter_shows_it_without_a_rebuild() {
    let mut arena = Arena::new();
    let mut scene = Scene::new();
    let content = dashlang::node("content")
        .mode(dashlang::LayoutMode::None)
        .size(400.0, 300.0);
    let label = badge::badge(&mut scene, 400.0, 300.0);
    scene.roots([content, label]);
    let mut live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));

    let signal = live
        .signal_named(badge::BACKEND)
        .expect("the badge declares its signal under this name");
    live.set(signal, badge::GPU);
    live.tick(0.016, &mut arena);

    let badge_id = arena.roots()[1];
    assert_eq!(arena.text(badge_id), Some("dashscene-gpu"));
    assert_eq!(arena.opacity(badge_id), 1.0);

    live.set(signal, badge::SKIA);
    live.tick(0.016, &mut arena);
    assert_eq!(arena.text(badge_id), Some("dashscene-skia"));
    assert_eq!(arena.opacity(badge_id), 1.0);
}

/// The badge is placed by its own offset rather than laid out under the
/// content root, so it overlaps the scene instead of stacking below it.
#[test]
fn the_badge_overlaps_the_content_rather_than_stacking_below_it() {
    let mut arena = Arena::new();
    let mut scene = Scene::new();
    let content = dashlang::node("content")
        .mode(dashlang::LayoutMode::None)
        .size(400.0, 300.0);
    let label = badge::badge(&mut scene, 400.0, 300.0);
    scene.roots([content, label]);
    let live = scene.build_live(&mut arena, Box::new(TaffySolver::new()));
    drop(live);

    let placed = arena.layout(arena.roots()[1]);
    assert!(placed.x > 0.0, "inset from the left edge");
    assert!(placed.y > 0.0, "inset from the top edge");
    assert!(placed.y < 300.0, "inside the content, not below it");
    assert!(placed.width > 0.0 && placed.height > 0.0);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
cargo test -p showcase --test badge
```

Expected: FAIL to compile, with `unresolved import showcase::badge` —
the module does not exist yet.

- [ ] **Step 3: Write the badge module**

Create `corpus/showcase/src/badge.rs`:

````rust
//! The painter badge: a label naming the painter that drew the frame.
//!
//! # Why this is content and not host code
//!
//! The label is text, and text is staged as glyph runs by the solver the
//! scene injects. Only `crate::solver::ShowcaseSolver` carries a
//! typesetter and the atlas list, so a label authored by the host — which
//! holds no solver handle — would commit through a text-incapable path
//! and stage no glyph runs at all. It lives here for the same reason the
//! variant switch does (`crate::lib`, "What a scene tells the host about
//! input"): the crate that owns the arena owns the content.
//!
//! # Why a second root
//!
//! The badge is appended to a scene's root list rather than added to its
//! tree, so it takes no part in the scene's layout and cannot move
//! anything the scene is demonstrating. Roots stage in list order and the
//! last one's rects come last in the committed table, which is what
//! draws the badge above the content.

use dashlang::{Channel, Node, Scene, node};
use dashscene_core::{TextAlign, TextAlignV};

use crate::resources::LATIN_FAMILY;
use crate::vocabulary::{palette, text_style};

/// The name the badge declares its signal under, so the host can find it
/// in a scene it did not build.
pub const BACKEND: &str = "backend";

/// The value naming `dashscene-skia`.
pub const SKIA: f32 = 1.0;

/// The value naming `dashscene-gpu`.
pub const GPU: f32 = 2.0;

/// The design extent every measurement below is expressed against, the
/// same convention the three scenes use.
const DESIGN: (f32, f32) = (960.0, 600.0);

/// The text a signal value names. `0.0` is the state before any painter
/// has been announced, and renders nothing: the still-image example never
/// writes the signal, so this is what keeps the badge out of
/// `docs/images/showcase-surfaces.png`.
///
/// Public so the mapping can be asserted without building a scene, and so
/// the host's own values can be checked against it.
pub fn label(value: f32) -> String {
    if value == SKIA {
        "dashscene-skia".to_owned()
    } else if value == GPU {
        "dashscene-gpu".to_owned()
    } else {
        String::new()
    }
}

/// Declares the badge's signal on `scene` and returns the root that draws
/// it, sized against the drawable `width` and `height`.
///
/// The caller appends the returned node to its root list:
///
/// ```ignore
/// let label = badge::badge(&mut scene, width, height);
/// scene.roots([root, label]);
/// ```
pub fn badge(scene: &mut Scene, width: f32, height: f32) -> Node {
    let unit = (width / DESIGN.0).min(height / DESIGN.1);
    let backend = scene.signal_named(BACKEND, 0.0);

    node("backend-badge")
        .at(20.0 * unit, 16.0 * unit)
        .size(190.0 * unit, 30.0 * unit)
        .fill(palette::PANEL)
        .corners(8.0 * unit)
        .text_style({
            let mut style = text_style(LATIN_FAMILY, 14.0 * unit, 600, palette::NEAR_WHITE);
            style.text_align = TextAlign::Center;
            style.text_align_v = TextAlignV::Center;
            style
        })
        // Both bindings are closures rather than the declarative
        // transforms: `Signal::map_range` and `Signal::clamp` are each
        // methods on `Signal<f32>` returning a `ScalarExpr` that carries
        // neither, so they do not compose into a clamped remap.
        .bind_text(backend.map(label))
        // Group opacity is paint-only, so raising and lowering the badge
        // reflows nothing.
        .bind(
            Channel::Opacity,
            backend.map(|value| if value > 0.0 { 1.0 } else { 0.0 }),
        )
}
````

Then add the module to `corpus/showcase/src/lib.rs`, in the existing
declaration block so the list stays alphabetical:

```rust
pub mod badge;
pub mod layout;
pub mod resources;
pub mod solver;
pub mod surfaces;
pub mod typography;
pub mod vocabulary;
```

- [ ] **Step 4: Run the test to verify it passes**

Run:

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
cargo test -p showcase --test badge
```

Expected: PASS, 5 tests.

If `the_badge_builds_invisible_and_last` fails on the name, check that
`node("backend-badge")` matches the string the test asserts.

- [ ] **Step 5: Mutation-test the invisible default**

The `0.0` default is the only thing keeping the badge out of the README
picture, so confirm the test actually holds it. Temporarily change
`label` so the fallback returns `"dashscene-skia"` instead of
`String::new()`, and change the opacity closure to return `1.0`
unconditionally.

Run:

```bash
cargo test -p showcase --test badge
```

Expected: `the_badge_builds_invisible_and_last` FAILS on both the text
and the opacity assertion. If it passes, the test is not holding the
default and must be fixed before continuing.

Revert both mutations and re-run to confirm PASS again.

- [ ] **Step 6: Commit**

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
just test
git add corpus/showcase/src/badge.rs corpus/showcase/src/lib.rs corpus/showcase/tests/badge.rs
git commit -m "feat(showcase): add the painter badge and its signal"
```

---

### Task 2: The three scenes carry the badge

**Files:**

- Modify: `corpus/showcase/src/surfaces.rs:367` (the `scene.roots([root]);`
  call)
- Modify: `corpus/showcase/src/typography.rs:234` (the same call)
- Modify: `corpus/showcase/src/layout.rs:298` (the same call)
- Test: `corpus/showcase/tests/badge.rs` (add one test)

**Interfaces:**

- Consumes: `crate::badge::{self, badge}` from Task 1.
- Produces: every scene in `showcase::SCENES` has exactly two roots, the
  second named `backend-badge`.

- [ ] **Step 1: Write the failing test**

Append to `corpus/showcase/tests/badge.rs`:

```rust
/// Every showcase scene carries the badge, as its last root. A scene
/// added later without one is a scene whose frames cannot be attributed
/// to a painter, so this asserts over the registry rather than over a
/// list repeated here.
#[test]
fn every_showcase_scene_carries_the_badge_as_its_last_root() {
    for scene in showcase::SCENES {
        let mut arena = Arena::new();
        let live = (scene.build)(&mut arena, 960, 600);
        drop(live);

        let roots = arena.roots();
        let last = *roots.last().expect("a scene has at least one root");
        assert_eq!(
            arena.name(last),
            Some("backend-badge"),
            "scene {} must carry the badge as its last root",
            scene.name
        );
        assert_eq!(
            arena.text(last),
            Some(""),
            "scene {} must build with no painter announced",
            scene.name
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
cargo test -p showcase --test badge every_showcase_scene
```

Expected: FAIL on the first scene, because its last root is named
something else — `surfaces`, not `backend-badge`.

- [ ] **Step 3: Add the badge to each scene**

In `corpus/showcase/src/surfaces.rs`, add the import beside the existing
`crate::` imports:

```rust
use crate::badge;
```

and replace `scene.roots([root]);` with:

```rust
let label = badge::badge(&mut scene, width, height);
scene.roots([root, label]);
```

`width` and `height` are already `f32` locals in each `build` — every
scene shadows its `u32` parameters with `let (width, height) = (width as
f32, height as f32);` near the top. Use those, not the parameters.

Make the identical change in `corpus/showcase/src/typography.rs` and
`corpus/showcase/src/layout.rs`.

- [ ] **Step 4: Run the test to verify it passes**

Run:

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
cargo test -p showcase --test badge
```

Expected: PASS, 6 tests.

- [ ] **Step 5: Confirm the existing scene tests still pass**

The scenes gained a root, and `corpus/showcase/tests/migration.rs`
asserts over scene structure.

Run:

```bash
cargo test -p showcase
```

Expected: PASS. If a migration test asserts a root count or a rect index,
it is asserting over a scene whose shape has legitimately changed —
update the expectation and say so in the commit message. Do not delete
the assertion.

- [ ] **Step 6: Commit**

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
just test
git add corpus/showcase/src corpus/showcase/tests/badge.rs
git commit -m "feat(showcase): carry the painter badge in every scene"
```

---

### Task 3: The host announces its painter

**Files:**

- Modify: `demo/src/painter.rs` (add a method to `impl Choice`, and a
  test to the existing `mod tests`)
- Modify: `demo/src/shell.rs` (`Host::rebuild` near line 571,
  `Host::swap_painter` near line 507)
- Test: `demo/src/painter.rs` (the inline `mod tests`)

**Interfaces:**

- Consumes: `showcase::badge` from Task 1;
  `dashlang::LiveScene::{signal_named, set}`.
- Produces: `Choice::badge_value(self) -> f32`;
  `Host::announce_painter(&mut self)`.

- [ ] **Step 1: Write the failing test**

Add to the existing `mod tests` at the bottom of `demo/src/painter.rs`:

```rust
    /// The host and the showcase must agree on what each value means.
    /// They are separate crates, and this is the one seam where they can
    /// drift apart without either failing to compile.
    #[test]
    fn each_painter_announces_the_name_the_showcase_gives_that_value() {
        assert_eq!(
            showcase::badge::label(Choice::Skia.badge_value()),
            "dashscene-skia"
        );
        assert_eq!(
            showcase::badge::label(Choice::Gpu.badge_value()),
            "dashscene-gpu"
        );
    }

    /// A value the badge does not recognise renders as nothing, so a
    /// painter announcing one would go unnamed on screen rather than
    /// loudly wrong.
    #[test]
    fn no_painter_announces_the_unannounced_value() {
        for painter in [Choice::Skia, Choice::Gpu] {
            assert_ne!(painter.badge_value(), 0.0);
            assert!(!showcase::badge::label(painter.badge_value()).is_empty());
        }
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
cargo test -p demo painter::tests
```

Expected: FAIL to compile, with `no method named badge_value found for
enum Choice`.

- [ ] **Step 3: Add `badge_value` to `Choice`**

In `demo/src/painter.rs`, add to `impl Choice`, after `other`:

```rust
/// The value this painter announces through the showcase's badge
/// signal, so the running window names the painter that drew it.
///
/// The numbers are `showcase::badge`'s, not this module's. They are
/// taken from there rather than written again so the two crates
/// cannot be given different values in two places.
pub fn badge_value(self) -> f32 {
    match self {
        Choice::Skia => showcase::badge::SKIA,
        Choice::Gpu => showcase::badge::GPU,
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run:

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
cargo test -p demo painter::tests
```

Expected: PASS.

- [ ] **Step 5: Mutation-test the cross-crate seam**

This test exists because two crates hold one convention. Confirm it
actually catches a disagreement: temporarily change `badge_value` so
`Choice::Gpu` returns `showcase::badge::SKIA`.

Run:

```bash
cargo test -p demo painter::tests
```

Expected: `each_painter_announces_the_name_the_showcase_gives_that_value`
FAILS. Revert and re-run to confirm PASS.

- [ ] **Step 6: Wire the host to write the signal**

In `demo/src/shell.rs`, add a method to `impl Host`, next to
`Host::force`:

```rust
/// Tells the running scene which painter is drawing it, so the badge
/// names the painter on screen.
///
/// A scene that declares no badge signal takes no write. That is what
/// makes the `--dsb` run need no special case here: a loaded document
/// carries no such signal, and its solver holds no typesetter, so a
/// label could not be staged there anyway.
///
/// Writing the signal is also what makes a swap re-solve. The write
/// marks a binding dirty, so the next tick commits through the
/// scene's own solver and stages the incoming name's glyph run —
/// without a rebuild, which is what keeps a swap showing the
/// difference between the two painters rather than between two runs.
fn announce_painter(&mut self) {
    let value = self.painter.badge_value();
    let Some(live) = self.live.as_mut() else {
        return;
    };
    if let Some(signal) = live.signal_named(showcase::badge::BACKEND) {
        live.set(signal, value);
    }
}
```

At the end of `Host::rebuild`, after `self.shown = None;`, add:

```rust
self.announce_painter();
```

At the end of the `Ok(presenter)` arm of `Host::swap_painter`, after
`self.force("a painter swap");`, add:

```rust
self.announce_painter();
```

- [ ] **Step 7: Verify the whole crate builds and the sanity tier passes**

Run:

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
just test
```

Expected: PASS. `just test` runs the sanity tier in about 5 seconds. If
`demo` fails to compile on `showcase::badge`, confirm `demo/Cargo.toml`
already lists the `showcase` dependency — it does, as
`showcase = { path = "../corpus/showcase" }`.

- [ ] **Step 8: Commit**

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
git add demo/src/painter.rs demo/src/shell.rs
git commit -m "feat(demo): announce the drawing painter to the scene badge"
```

---

### Task 4: Confirm it on screen, and that the still is unchanged

**Files:**

- Modify: `corpus/showcase/README.md` (the coverage checklist)
- No source changes expected. If a check below fails, fix the cause and
  note it in the commit.

**Interfaces:**

- Consumes: everything from Tasks 1 to 3.
- Produces: nothing further depends on this task.

- [ ] **Step 1: Confirm the still-image example produces no badge**

This is the check that protects the repository README's picture.

Run:

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
cargo run -p showcase --example still -- surfaces /tmp/badge-check.png 1600 1000 0 0
```

Open `/tmp/badge-check.png`. Expected: the sixteen tiles, and **no badge
anywhere**. The example never writes the signal, so the badge is at zero
opacity with empty text.

- [ ] **Step 2: Confirm the committed README picture is byte-identical**

Regenerate the committed image with the exact command
`corpus/showcase/README.md` records, then check git sees no change:

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
cargo run -p showcase --example still -- surfaces docs/images/showcase-surfaces.png 1600 1000 0 0
git status --short docs/images/showcase-surfaces.png
```

Expected: **no output** from `git status` — the file is unchanged. If it
reports a modification, the badge has reached the still and Task 1's
default is wrong. Restore the file with
`git checkout -- docs/images/showcase-surfaces.png` and fix the cause
before continuing.

- [ ] **Step 3: Confirm the badge appears under the reference painter**

```bash
cargo run -p demo -- surfaces
```

Expected: the window shows the scene with a dark pill in the top-left
corner reading `dashscene-skia`. Confirm it is legible and does not cover
anything the scene is demonstrating.

- [ ] **Step 4: Confirm the swap key changes the name without restarting the scene**

With the window from Step 3 still open, press `P`.

Expected: the badge reads `dashscene-gpu`, the stderr line
`demo: painter is now dashscene-gpu (...)` appears, and **the scene does
not restart** — the animation continues from where it was rather than
snapping back to its starting values. Press `P` again and confirm it
returns to `dashscene-skia`.

This is the check that the swap path re-solves without a rebuild. If the
scene restarts, `announce_painter` has been wired into a rebuild rather
than beside the swap.

- [ ] **Step 5: Confirm the badge draws under the GPU painter too**

```bash
cargo run -p demo -- --painter gpu typography
```

Expected: the badge reads `dashscene-gpu` and its glyphs are drawn by the
lean painter. The label is drawn by the painter under test, so this is
the check that the GPU text path renders it.

- [ ] **Step 6: Confirm the document run is unaffected**

```bash
cargo run -p demo -- --dsb
```

Expected: the compiled document draws exactly as before, with **no
badge** and no panic. The signal lookup finds nothing and nothing is
written.

- [ ] **Step 7: Record the badge in the coverage checklist**

`corpus/showcase/README.md` holds a 33-row table under "Coverage is a
checklist a person walks, not a test". Add row 34, immediately after row
33, matching the existing column format:

```markdown
| 34 | painter badge | all three | a dark pill in the top-left corner naming the painter that drew the frame: `dashscene-skia` by default, `dashscene-gpu` after pressing **P**. It is empty and fully transparent until the host announces a painter, which is why the still-image example renders nothing in its place |
```

- [ ] **Step 8: Run the regression tier and commit**

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
just build
git add corpus/showcase/README.md
git commit -m "docs(showcase): record the painter badge in the coverage checklist"
```

Expected: `just build` green. Read its final exit status rather than the
test summary it prints — a green test summary above a non-zero exit means
a later step, usually clippy, failed.

---

### Task 5: Garden the working memory and open the pull request

**Files:**

- Move: `docs/wip/2026-08-04-demo-backend-badge-design.md` and
  `docs/wip/2026-08-04-demo-backend-badge-plan.md` to `docs/archive/`
- Possibly modify: `docs/design/` — see Step 2

**Interfaces:**

- Consumes: the completed work from Tasks 1 to 4.
- Produces: an empty `docs/wip/` for this branch's files, and a pull
  request.

- [ ] **Step 1: Run the gardening skill**

`docs/wip/` must be empty of this branch's files before a
`main`-targeting pull request merges. Invoke the `sdd-gardening` skill,
which decides what of the design record belongs in a durable home.

- [ ] **Step 2: Decide the durable home**

The badge is a property of the demonstration host and the showcase
corpus, both of which are already described in `docs/design/`. The
durable record is a short addition to the showcase's own design record
saying that scenes carry a painter badge as a second root and that the
host drives it through one named signal. It is not a decision record:
nothing downstream is bound by it.

Move the raw design and plan to `docs/archive/` rather than deleting
them.

- [ ] **Step 3: Verify `docs/wip/` is clean for this branch**

Run:

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
ls docs/wip/
```

Expected: neither `2026-08-04-demo-backend-badge-design.md` nor
`2026-08-04-demo-backend-badge-plan.md` remains. Other files predating
this branch may still be there and are not this branch's business.

- [ ] **Step 4: Run the full pre-pull-request gate**

```bash
just verify
```

Expected: commit-message lint over the branch range passes, then
`just build` passes.

- [ ] **Step 5: Squash and push**

The branch lands as one commit per the repository's merge convention.
Rebase onto the latest `main`, squash into one conventional commit, and
force-push.

```bash
cd /Users/sebastientasson/Workspace/driftsys/dashscene-backend-badge
git fetch origin
git rebase -i origin/main
git push --force-with-lease -u origin story/demo-backend-badge
```

Before fetching, confirm `git config --get remote.origin.url` is
`https://github.com/driftsys/dashscene-staging.git`. A concurrent session
has been observed repointing it, and a fetch against the wrong remote
prunes every remote-tracking reference.

- [ ] **Step 6: Open the pull request**

Open it as an ordinary pull request, **never a draft** — a draft
prevents review, which is the opposite of why it is being opened.

Write the body to a file first and pass it with `--body-file`. Passing
prose inline to `--body` inside double quotes lets any backtick run as a
command substitution, which has posted blank sections before.

```bash
cat > /tmp/badge-pr-body.md <<'BODY'
The showcase host reported which painter drew a frame only to stderr, so
a screenshot carried no evidence of it. Every scene now draws a badge
naming the painter in the top-left corner.

## What changed

- `corpus/showcase/src/badge.rs` builds the badge as a second root, which
  each of the three scenes appends to its root list. It lives in the
  showcase crate because only `ShowcaseSolver` carries the typesetter
  that stages its glyphs.
- One named scalar signal drives both the badge's text and its group
  opacity. `0.0` renders it empty and transparent, `1.0` names
  `dashscene-skia`, `2.0` names `dashscene-gpu`.
- `demo` writes that signal when it builds a scene and when the `P` key
  swaps the painter. The swap needs no rebuild, so it still shows the
  difference between the two painters rather than between two runs.
- The `--dsb` run carries no badge and needs no special case: it declares
  no such signal, so nothing is written.

## What the default protects

The still-image example never writes the signal, so the badge renders as
nothing there. `docs/images/showcase-surfaces.png`, the picture in the
repository README, was regenerated and is byte-identical.

## Tests

Regression tier, via `just build`. The diff touches `demo/` and
`corpus/showcase/` only, both outside the `packer` path filter, so the
calibration tier was not required.

The cross-crate seam — `demo` decides the values, `corpus/showcase`
decides what they mean — is held by a test that was mutation-tested by
pointing `Choice::Gpu` at the Skia value and confirming the assertion
fails.

## Review findings

<!-- filled in from /code-review before this merges -->
BODY

gh pr create \
  --title "feat(demo): name the drawing painter on screen" \
  --body-file /tmp/badge-pr-body.md
```

The body names the tier that was actually run, and contains no closing
keyword (`closes`, `fixes`, `resolves`) — GitHub acts on one anywhere in
the body, including mid-sentence. Write `Refs #N` when referring to an
issue.

- [ ] **Step 7: Review, then merge**

Run `/code-review` on the pull request and capture every finding as a
checklist in the description. Fix all critical findings. File one
`debt`-labeled issue per minor finding rather than fixing it inline.

Merge with `gh pr merge --merge` once the review is complete and CI is
green on the exact commit being merged. Name the method explicitly; the
merge button preselects whichever method was last used.
