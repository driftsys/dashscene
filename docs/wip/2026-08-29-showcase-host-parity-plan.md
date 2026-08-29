# Showcase host parity implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** `demo-android` and the Unity showcase draw the same three showcase
scenes, respond to the same input, are measured under the same conditions, and
have their frames compared on the device.

**Architecture:** The scene registry is already single-sourced in
`corpus/showcase`. This work single-sources the _input semantics_ next to it, in
a new `showcase::input` module that names no scene and depends on no windowing
library, and then gives each host a thin binding layer over it — winit for the
desktop, `MotionEvent` and `KeyEvent` for `demo-android`, `Input` for Unity,
which reaches the same semantics through new `ds_demo_*` entry points. A capture
mode pins scene, phase and signal so two hosts can be photographed in the same
state and compared.

**Tech Stack:** Rust 2024 (workspace, `resolver = "3"`), Java (Android
activity), C# (Unity 6000.3.23f1, IL2CPP), bash + python for the measurement
harness, `skia-safe` for image comparison.

**Spec:** `docs/wip/2026-08-29-showcase-host-parity-design.md`

## Global Constraints

Copied from the spec. Every task's requirements implicitly include these.

- **Vocabulary — the signal.** A horizontal drag drives the scene's scalar
  signal from the pointer's normalised horizontal position, clamped to
  `0.0..=1.0`. `DPAD_LEFT` (21) sets it to `0.0`, `DPAD_RIGHT` (22) sets it to
  `1.0`.
- **Vocabulary — four commands.** `next` / `previous`: swipe up / swipe down,
  `PAGE_DOWN` (93) / `PAGE_UP` (92). `action`: tap, `SPACE` (62). `orientation`:
  two-finger tap, `DPAD_UP` (19). `readout`: long press, `R` (46).
- **The left and right keys mean the signal, never navigation.** This is
  `demo/src/input.rs`'s existing binding, and it is the one that wins.
- **Extent policy.** Both Android hosts run edge to edge, at the full display
  extent.
- **The readout is suppressible on both hosts**, and suppressed by a flag, never
  by disabling the behaviour that draws it.
- **Capture mode:** `capture <scene> <phase> <signal>`, delivered as an intent
  extra. It builds the scene, pins the phase, sets the signal, suppresses the
  readout, draws a fixed number of frames and holds.
- **Entry order.** The first three entries are the scenes in
  `corpus/showcase::SCENES` order on both hosts. Unity's `.dsb` documents are
  appended after them.
- **No threshold is invented.** Task 10 measures the pixel-difference baseline;
  nothing before it hard-codes a tolerance.
- **Between edits and before every commit:** `just test`. Before pushing:
  `just build`.

## File structure

**Created**

- `corpus/showcase/src/input.rs` — the scene-agnostic input semantics: normalise
  a pointer x into a signal value, write a signal, run a scene's action. No
  windowing library, no scene named.
- `measure/android/host-parity.sh` — drives both hosts through capture mode and
  compares the captures.
- `measure/android/host-parity-test.sh` — the device-free test for it.
- `docs/decisions/the-showcase-hosts-share-one-surface.md` — the contract.

**Modified**

- `corpus/showcase/src/lib.rs` — declare `pub mod input`.
- `demo/src/input.rs` — keeps only the winit `KeyCode` mapping; the semantics
  move to `showcase::input`.
- `unity/demo-producer/src/lib.rs` — add `ds_demo_signal`.
- `goldens/tooling/src/lib.rs` — extract `Comparison` / `compare_rgba` /
  `compare_pngs`; re-express `compare_against` on them.
- `demo-android/src/lib.rs`, `demo-android/src/host.rs` — commands, scene
  switching, capture mode, the readout string.
- `demo-android/android/java/dev/driftsys/dashscene/demo/DemoActivity.java` —
  edge to edge, `FrameLayout` + `TextView`, touch and key handling.
- `unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneShowcase.cs` — the
  signal, touch, page-key navigation, readout suppression, capture mode.
- `measure/android/unity-frame-cost.sh:135`, `justfile:4039`, `justfile:3979` —
  the three call sites that send keyevent 22 for "next entry".
- `docs/design/android-toolchain.md` — Task 10's readings.
- `docs/design/host-integration.md` — links the contract.

---

### Task 1: `showcase::input` — the scene-agnostic input semantics

The desktop host already has these semantics, but they sit in a binary crate and
`key()` takes a `winit::keyboard::KeyCode`, so no other host can call them. Move
the part that names no windowing library into `corpus/showcase`, which `demo`,
`demo-android` and `unity/demo-producer` all already depend on.

**Files:**

- Create: `corpus/showcase/src/input.rs`
- Modify: `corpus/showcase/src/lib.rs` (declare the module)
- Modify: `demo/src/input.rs` (call the new module instead of its own bodies)

**Interfaces:**

- Consumes: `dashlang::LiveScene`, `dashscene_core::Arena`,
  `showcase::SceneAction` — all already dependencies of `corpus/showcase`.
- Produces:
  - `pub fn signal_from_x(x_physical: f64, width: u32) -> Option<f32>`
  - `pub fn set_signal(live: &mut LiveScene, signal: &str, value: f32) -> bool`
  - `pub fn run_action(live: &mut LiveScene, arena: &mut Arena, action: Option<SceneAction>) -> bool`

- [ ] **Step 1: Write the failing test**

Append to `corpus/showcase/src/input.rs` (the file does not exist yet; write the
tests first and let the module fail to compile):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_width_drawable_yields_no_signal_value() {
        // A minimised or not-yet-configured surface: there is no width to
        // normalise against, and no frame to show the result in either.
        assert_eq!(signal_from_x(100.0, 0), None);
    }

    #[test]
    fn the_pointer_normalises_over_the_drawable_width() {
        assert_eq!(signal_from_x(0.0, 1000), Some(0.0));
        assert_eq!(signal_from_x(500.0, 1000), Some(0.5));
        assert_eq!(signal_from_x(1000.0, 1000), Some(1.0));
    }

    #[test]
    fn a_pointer_outside_the_drawable_clamps_into_range() {
        // Every showcase signal is authored over 0.0..=1.0, so a pointer
        // dragged past the edge saturates rather than writing out of range.
        assert_eq!(signal_from_x(-40.0, 1000), Some(0.0));
        assert_eq!(signal_from_x(1400.0, 1000), Some(1.0));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo nextest run -p showcase input::` Expected: FAIL — the module does
not exist, so the crate does not compile.

- [ ] **Step 3: Write the module**

Create `corpus/showcase/src/input.rs`:

```rust
//! Input semantics shared by every host that draws these scenes.
//!
//! Names no scene, no node, no signal name and no colour, and depends on no
//! windowing library: a host maps its own events onto these three functions
//! and passes the signal name and optional action it was handed on
//! [`crate::Showcase`]. That is what lets `demo` (winit), `demo-android`
//! (`MotionEvent`) and the Unity sample (`Input`, through
//! `unity/demo-producer`) share one vocabulary instead of three.
//!
//! This module is the second home of these bodies. They were
//! `demo/src/input.rs`'s, where story #573 wrote them scene-agnostic
//! deliberately; what kept them from being shared was the crate, not the
//! design — `demo` is a binary and its `key()` takes a `winit` type.

use dashlang::LiveScene;
use dashscene_core::Arena;

use crate::SceneAction;

/// The signal value a pointer at `x_physical` names, normalised to
/// `width` and clamped to the `0.0..=1.0` range every showcase signal is
/// authored over.
///
/// `None` for a zero-width drawable: there is nothing to normalise against.
pub fn signal_from_x(x_physical: f64, width: u32) -> Option<f32> {
    if width == 0 {
        return None;
    }
    Some((x_physical as f32 / width as f32).clamp(0.0, 1.0))
}

/// Writes `value` to the scene's named signal.
///
/// Returns whether anything was written, so a caller knows whether to force a
/// redraw. `false` rather than a panic when the scene does not declare the
/// name: the name is the scene's to choose, and a document loaded from a
/// `.dsb` can present exactly that case.
pub fn set_signal(live: &mut LiveScene, signal: &str, value: f32) -> bool {
    match live.signal_named(signal) {
        Some(handle) => {
            live.set_signal(handle, value);
            true
        }
        None => false,
    }
}

/// Runs the scene's own variant switch, if it declares one.
///
/// Returns whether anything ran. A scene with no variant set is not an error:
/// the command does nothing rather than the host inventing a fallback.
pub fn run_action(
    live: &mut LiveScene,
    arena: &mut Arena,
    action: Option<SceneAction>,
) -> bool {
    match action {
        Some(action) => {
            action(live, arena);
            true
        }
        None => false,
    }
}
```

Declare it in `corpus/showcase/src/lib.rs`, beside the existing module
declarations:

```rust
pub mod input;
```

- [ ] **Step 4: Reconcile `set_signal` with `LiveScene`'s real API**

`demo/src/input.rs` already contains a working `set_signal`. Copy its body
verbatim rather than the sketch above if the two differ — `LiveScene`'s signal
API is the authority, not this plan.

Run: `sed -n "/fn set_signal/,/^}/p" demo/src/input.rs`

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo nextest run -p showcase input::` Expected: PASS, 3 tests.

- [ ] **Step 6: Point `demo/src/input.rs` at the new module**

`cursor_moved` and the `Space` arm now delegate. The winit `KeyCode` match stays
in `demo/`, because `KeyCode` is winit's type and `corpus/showcase` must not
depend on a windowing library.

```rust
pub fn cursor_moved(live: &mut LiveScene, signal: &str, x_physical: f64, width: u32) -> bool {
    match showcase::input::signal_from_x(x_physical, width) {
        Some(value) => showcase::input::set_signal(live, signal, value),
        None => false,
    }
}

pub fn key(
    code: KeyCode,
    signal: &str,
    action: Option<SceneAction>,
    live: &mut LiveScene,
    arena: &mut Arena,
) -> bool {
    match code {
        KeyCode::ArrowLeft => showcase::input::set_signal(live, signal, 0.0),
        KeyCode::ArrowRight => showcase::input::set_signal(live, signal, 1.0),
        KeyCode::Space => showcase::input::run_action(live, arena, action),
        _ => false,
    }
}
```

- [ ] **Step 7: Run the desktop host's own tests**

`demo/src/input.rs` carries a large existing test module. It must pass unchanged
— that is what proves the move changed no behaviour.

Run: `cargo nextest run -p demo` Expected: PASS, with no test edited.

- [ ] **Step 8: Mutate to prove the tests bite**

Change the clamp in `signal_from_x` to `.clamp(0.0, 0.9)` and re-run
`cargo nextest run -p showcase input::`. Expected: the normalisation and
clamping tests FAIL. Revert the mutation.

- [ ] **Step 9: `just test`, then commit**

```bash
just test
git add corpus/showcase/src/input.rs corpus/showcase/src/lib.rs demo/src/input.rs
git commit -m "refactor(corpus/showcase): the input semantics move where every host can call them"
```

---

### Task 2: `ds_demo_signal` — the signal crosses the C ABI

The Unity showcase cannot drive or pin the signal at all today.
`unity/demo-producer` exports `ds_demo_pulse`, which applies a scene's
_scripted_ phase, and nothing that sets the signal to a value. Capture mode
needs the second.

**Files:**

- Modify: `unity/demo-producer/src/lib.rs`
- Modify: `unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneShowcase.cs`
  (the `DemoScenes` P/Invoke block only; the behaviour is Task 6)

**Interfaces:**

- Consumes: `showcase::input::set_signal` from Task 1; `DsRuntime`, `DsStatus`
  from `dashscene-ffi`, already used throughout this file.
- Produces:
  `pub extern "C" fn ds_demo_signal(runtime: DsRuntime, value: f32) -> DsStatus`

- [ ] **Step 1: Read how `ds_demo_pulse` is written and tested**

Run: `sed -n '185,235p' unity/demo-producer/src/lib.rs`

The new entry point copies its runtime lookup, its status mapping and its
comment discipline exactly. Do not invent a second convention in one file.

- [ ] **Step 2: Write the failing test**

In `unity/demo-producer/src/lib.rs`'s test module, beside the existing
`ds_demo_pulse` tests:

```rust
#[test]
fn the_signal_reaches_the_installed_scene() {
    let runtime = install_scene_for_test("layout");
    assert_eq!(ds_demo_signal(runtime, 1.0), DsStatus::Ok);
    assert_eq!(ds_demo_signal(runtime, 0.0), DsStatus::Ok);
}

#[test]
fn a_signal_on_no_installed_scene_reports_rather_than_panics() {
    // The same shape ds_demo_pulse uses for this case, and for the same
    // reason: a host calling in the wrong order gets a status, not a crash
    // across the ABI.
    assert_ne!(ds_demo_signal(DsRuntime::null_for_test(), 0.5), DsStatus::Ok);
}
```

Use whatever the file's existing tests use to build a runtime and to name a null
one — read them first and match; the two helper names above are placeholders for
that file's real ones and must be replaced by them.

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo nextest run -p demo-producer signal` Expected: FAIL —
`ds_demo_signal` is not defined.

- [ ] **Step 4: Implement the entry point**

```rust
/// Sets the installed scene's own scalar signal to `value`.
///
/// The value is clamped to `0.0..=1.0` by `showcase::input`, which is the
/// range every showcase signal is authored over. A scene that does not
/// declare its signal name is not an error — the write reports that nothing
/// happened, the way every other host treats it.
///
/// Distinct from [`ds_demo_pulse`], which applies the scene's *scripted*
/// phase. A host driving the signal by hand, and a capture pinning it to a
/// known value, both need this one.
#[unsafe(no_mangle)]
pub extern "C" fn ds_demo_signal(runtime: DsRuntime, value: f32) -> DsStatus {
    // Body mirrors ds_demo_pulse: look the runtime up, refuse when no scene
    // is installed, then call showcase::input::set_signal with the scene's
    // own signal name.
}
```

Write the body against `ds_demo_pulse`'s, not against this sketch.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo nextest run -p demo-producer` Expected: PASS.

- [ ] **Step 6: Mutate to prove the test bites**

Make the body return `DsStatus::Ok` without writing anything. Expected: the
first test still passes — which is the point. Add an assertion that reads the
signal back, or that a variant-free scene reports the write, so the test fails
under that mutation. A test that cannot see the write pins nothing.

- [ ] **Step 7: Declare it on the C# side**

Add the `DllImport` beside the existing `ds_demo_*` declarations in
`DashsceneShowcase.cs`, and a `DemoScenes.Signal(float)` wrapper next to
`DemoScenes.Count` / `Name` / `Summary`. No behaviour yet — Task 6 binds it.

- [ ] **Step 8: `just test`, then commit**

```bash
just test
git add unity/demo-producer/src/lib.rs \
        unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneShowcase.cs
git commit -m "feat(unity): the scene's signal crosses the demo ABI"
```

---

### Task 3: a two-image comparison in `goldens/tooling`

`compare_against` walks two decoded buffers and counts differing pixels. The
parity harness needs that arithmetic over two arbitrary PNGs, and needs two
numbers it does not currently produce: the bounding box of the differing pixels,
and the maximum per-channel delta. A whole-frame differing fraction passes a
systematic shift confined to one region, which is exactly the failure the
harness must catch.

**Files:**

- Modify: `goldens/tooling/src/lib.rs`
- Test: `goldens/tooling/src/lib.rs` (its own `#[cfg(test)]` module)

**Interfaces:**

- Produces:
  - `pub struct Comparison { pub width: i32, pub height: i32, pub total: usize, pub differing: usize, pub first: Option<(i32, i32)>, pub bounds: Option<(i32, i32, i32, i32)>, pub max_channel_delta: u8 }`
  - `pub fn fraction(&self) -> f64` on `Comparison`
  - `pub fn compare_rgba(left: &[u8], right: &[u8], width: i32) -> Comparison`
  - `pub fn compare_pngs(left: &[u8], right: &[u8]) -> Result<Comparison, String>`

**Do not touch `decode_rgba`.** Two `#[should_panic(expected = ...)]` tests in
this file depend on its message (`"not a decodable PNG"`) and on the
`UPDATE_GOLDENS` message. `compare_against` keeps calling it; only the pixel
walk is extracted.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn identical_buffers_compare_equal() {
    let a = vec![0u8; 4 * 4 * 4]; // 4x4 RGBA
    let c = compare_rgba(&a, &a, 4);
    assert_eq!(c.differing, 0);
    assert_eq!(c.fraction(), 0.0);
    assert_eq!(c.bounds, None);
    assert_eq!(c.max_channel_delta, 0);
}

#[test]
fn one_differing_pixel_is_located_and_bounded() {
    let a = vec![0u8; 4 * 4 * 4];
    let mut b = a.clone();
    // pixel (2, 1) — offset (1 * 4 + 2) * 4
    b[(1 * 4 + 2) * 4 + 1] = 9;
    let c = compare_rgba(&a, &b, 4);
    assert_eq!(c.differing, 1);
    assert_eq!(c.first, Some((2, 1)));
    assert_eq!(c.bounds, Some((2, 1, 2, 1)));
    assert_eq!(c.max_channel_delta, 9);
}

/// The case a differing fraction alone cannot see: a small, dense,
/// systematic difference in one region of an otherwise identical frame.
/// The fraction stays low; the bounding box and the channel delta are what
/// report it.
#[test]
fn a_region_shifted_image_is_reported_by_its_bounds_not_its_fraction() {
    let width = 100;
    let a = vec![0u8; width * width * 4];
    let mut b = a.clone();
    for y in 10..20 {
        for x in 30..40 {
            b[(y * width + x) * 4] = 255;
        }
    }
    let c = compare_rgba(&a, &b, width as i32);
    assert_eq!(c.differing, 100);
    assert!(c.fraction() < 0.02, "the fraction alone looks like noise");
    assert_eq!(c.bounds, Some((30, 10, 39, 19)));
    assert_eq!(c.max_channel_delta, 255);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo nextest run -p goldens compare_` Expected: FAIL — `compare_rgba` is
not defined.

- [ ] **Step 3: Implement `Comparison` and `compare_rgba`**

```rust
/// What comparing two same-sized RGBA8888 buffers found.
///
/// Carries three numbers rather than one because a differing fraction alone
/// passes a systematic difference confined to one region: `bounds` and
/// `max_channel_delta` are what see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    pub total: usize,
    pub differing: usize,
    /// The first differing pixel in row-major order, as `(x, y)`.
    pub first: Option<(i32, i32)>,
    /// The differing pixels' bounding box, as `(min_x, min_y, max_x, max_y)`.
    pub bounds: Option<(i32, i32, i32, i32)>,
    /// The largest absolute single-channel difference anywhere in the frame.
    pub max_channel_delta: u8,
}

impl Comparison {
    /// The differing pixels as a fraction of the frame.
    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.differing as f64 / self.total as f64
    }
}

/// Compares two tightly packed RGBA8888 buffers of the same width.
///
/// The caller has already established that the two are the same size:
/// `compare_pngs` does it after decoding, and `compare_against` does it
/// against the golden's dimensions.
pub fn compare_rgba(left: &[u8], right: &[u8], width: i32) -> Comparison {
    let total = left.len() / 4;
    let mut differing = 0usize;
    let mut first = None;
    let mut bounds: Option<(i32, i32, i32, i32)> = None;
    let mut max_channel_delta = 0u8;

    for (i, (a, b)) in left.chunks_exact(4).zip(right.chunks_exact(4)).enumerate() {
        if a == b {
            continue;
        }
        differing += 1;
        let (x, y) = (i as i32 % width, i as i32 / width);
        if first.is_none() {
            first = Some((x, y));
        }
        bounds = Some(match bounds {
            None => (x, y, x, y),
            Some((min_x, min_y, max_x, max_y)) => {
                (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
            }
        });
        for (channel_a, channel_b) in a.iter().zip(b.iter()) {
            max_channel_delta = max_channel_delta.max(channel_a.abs_diff(*channel_b));
        }
    }

    Comparison { total, differing, first, bounds, max_channel_delta }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo nextest run -p goldens compare_` Expected: PASS, 3 tests.

- [ ] **Step 5: Re-express `compare_against` on top of it**

Replace the inline loop with a `compare_rgba` call and format the existing panic
and stderr messages from the returned `Comparison`. **The message text does not
change** — `{differing}/{total} pixel(s) differ ({:.3}%, ...)`, first at
`({x}, {y})`, and the dimension-mismatch message, all stay byte-identical.

- [ ] **Step 6: Prove the extraction changed no behaviour**

Run: `cargo nextest run -p goldens` Expected: PASS — every committed golden
test, unedited. That is the regression gate for this refactor.

- [ ] **Step 7: Add `compare_pngs`**

Decodes both buffers without panicking, returns `Err(String)` on an undecodable
buffer or a dimension mismatch, and calls `compare_rgba` otherwise. Test both
error arms.

- [ ] **Step 8: Mutate to prove the tests bite**

Change `max_channel_delta` to accumulate a minimum instead of a maximum.
Expected: the region-shift and single-pixel tests FAIL. Revert.

- [ ] **Step 9: `just test`, then commit**

```bash
just test
git add goldens/tooling/src/lib.rs
git commit -m "feat(goldens): compare two images, with the bounds a fraction cannot see"
```

---

### Task 4: `demo-android` — the commands, the scenes and capture mode (native half)

**Files:**

- Modify: `demo-android/src/lib.rs`
- Modify: `demo-android/src/host.rs`
- Test: `demo-android/src/lib.rs` (its existing `#[cfg(test)]` module)

**Interfaces:**

- Consumes: `showcase::input::{signal_from_x, set_signal, run_action}` (Task 1),
  `showcase::SCENES`, `showcase::Showcase`.
- Produces, for the Java half to call over JNI:
  - `nativeCommand(handle: jlong, command: jint) -> jboolean`
  - `nativeDrag(handle: jlong, x_physical: jfloat) -> jboolean`
  - `nativeReadout(handle: jlong) -> jstring`
  - a `Command` enum with the discriminants Java sends: `0` next, `1` previous,
    `2` action, `3` orientation, `4` readout.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn next_walks_the_scene_registry_and_wraps() {
    let mut index = 0usize;
    index = super::advance(index, 1);
    assert_eq!(showcase::SCENES[index].name, "typography");
    index = super::advance(index, 1);
    assert_eq!(showcase::SCENES[index].name, "layout");
    index = super::advance(index, 1);
    assert_eq!(showcase::SCENES[index].name, "surfaces", "next wraps");
}

#[test]
fn previous_walks_the_other_way_and_wraps() {
    assert_eq!(showcase::SCENES[super::advance(0, -1)].name, "layout");
}

/// The registry is the entry order both hosts walk, so a host that hard-coded
/// three would silently stop walking a fourth scene.
#[test]
fn the_walk_covers_every_scene_the_registry_declares() {
    let mut seen = std::collections::BTreeSet::new();
    let mut index = 0usize;
    for _ in 0..showcase::SCENES.len() {
        seen.insert(showcase::SCENES[index].name);
        index = super::advance(index, 1);
    }
    assert_eq!(seen.len(), showcase::SCENES.len());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo nextest run -p demo-android` Expected: FAIL — `advance` is not
defined.

- [ ] **Step 3: Implement `advance` and the command dispatch**

`advance(index, delta)` wraps over `SCENES.len()` with no hard-coded 3. The
command dispatch calls `showcase::input::run_action` for `action` and rebuilds
the arena for `next` / `previous` at the current extent, reusing `Scene::build`
as `host.rs` already does on a resize.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo nextest run -p demo-android` Expected: PASS.

- [ ] **Step 5: Add capture mode**

`capture <scene> <phase> <signal>` arrives as three intent extras and is plumbed
through `select()`'s neighbourhood in `lib.rs`. In capture mode the host: builds
the named scene, calls the scene's `pulse` to the named phase index, calls
`set_signal` with the named value, suppresses the readout, draws
`CAPTURE_FRAMES` frames and then stops advancing the phase.

Add a test that a capture-mode launch parses all three extras and that a missing
one is refused rather than defaulted — a capture with a defaulted signal is a
capture of the wrong state, and it must fail loudly.

- [ ] **Step 6: Add the readout string**

One function turning `timing::Sample` into the readout's lines. The fields and
their order are the contract's, and Task 6 renders the same ones from C#. Test
it against a constructed `Sample`, not against a device.

- [ ] **Step 7: Mutate to prove the tests bite**

Change `advance` to `(index + 1) % 3`. Expected: `the_walk_covers_every_scene`
still passes — three scenes exist. Then add a fourth entry to a test-local
registry, or assert against `SCENES.len()` directly, so the mutation fails.
Revert.

- [ ] **Step 8: `just test`, then commit**

```bash
just test
git add demo-android/src/
git commit -m "feat(demo-android): the shared commands, scene walking and capture mode"
```

---

### Task 5: `demo-android` — the Java half

**Files:**

- Modify:
  `demo-android/android/java/dev/driftsys/dashscene/demo/DemoActivity.java`
- Modify:
  `demo-android/android/java/dev/driftsys/dashscene/demo/DemoNative.java`
- Modify: `demo-android/android/AndroidManifest.xml` (only if the theme must
  change for edge to edge)

The class comment currently says "Touch input is not wired: that would be layer
1, app state writing signals, and the showcase writes its own from Rust." **That
sentence becomes false in this task and must be rewritten**, not left. What
makes touch correct here is that the write still happens in Rust — Java forwards
an event, it does not author a signal.

- [ ] **Step 1: Go edge to edge**

In `onCreate`, before `setContentView`:

```java
WindowCompat.setDecorFitsSystemWindows(getWindow(), false);
```

or, without the AndroidX dependency this host may not carry,
`getWindow().setDecorFitsSystemWindows(false)` on API 30+ with the
`SYSTEM_UI_FLAG_*` fallback below it. Check what `demo-android/android/build.sh`
puts on the classpath before choosing — this host builds without Gradle.

- [ ] **Step 2: Verify the extent actually changed**

Run: `just android && adb logcat -d | grep "demo: surfaceChanged"` Expected:
`1080x2340` on the Pixel 5, not `1080x1984`. **This is the step the whole
comparison rests on** — if the extent does not change, Tasks 8 and 9 compare two
different drawables and every number they produce is wrong.

- [ ] **Step 3: Build the view stack**

Replace the bare `setContentView(view)` with a `FrameLayout` holding the
`SurfaceView` and, above it, a `TextView` for the readout — `View.GONE` while
suppressed.

- [ ] **Step 4: Wire touch**

A `GestureDetector` for the tap, long press and swipes, plus a raw
`onTouchEvent` horizontal-drag path calling `nativeDrag`. A two-finger tap is
`onTouchEvent` with `getPointerCount() == 2`. Each maps to the `Command`
discriminants Task 4 defined.

- [ ] **Step 5: Wire key events**

`onKeyDown` maps 21/22 to the signal ends, 92/93 to previous/next, 62 to action,
19 to orientation, 46 to the readout. The same five bindings the gestures
produce, so the harness and a hand drive the same code.

- [ ] **Step 6: Read the capture extras**

`--es capture_scene`, `--ei capture_phase`, `--ef capture_signal`, forwarded to
the native half.

- [ ] **Step 7: Rewrite the stale class comment**

- [ ] **Step 8: Write the contract record**

Create `docs/decisions/the-showcase-hosts-share-one-surface.md`, carrying the
Global Constraints section of this plan as its normative content: the vocabulary
table, the entry order, the extent policy, the readout rule and capture mode.
State the left-and-right ruling and its date, and name the three harness call
sites Task 7 changes.

It is written here, in the task that first implements it, rather than as a story
of its own. A documentation-only story in this repository has twice produced
dozens of review findings and no executable defect.

Link it from `docs/design/host-integration.md`, which is the design record that
owns how hosts sit on the runtime. One owning file, linked from the design, not
the same rule restated in both.

- [ ] **Step 9: Verify on the device**

```bash
just android
adb shell input keyevent 93   # next entry
adb shell input keyevent 62   # action
adb shell input keyevent 46   # readout
adb logcat -d | grep "demo:"
```

Expected: the log names a scene change, the action running, and the readout
toggling.

- [ ] **Step 10: Commit**

```bash
just test
git add demo-android/android/ docs/decisions/the-showcase-hosts-share-one-surface.md docs/design/host-integration.md
git commit -m "feat(demo-android): touch, keys and a readout, edge to edge"
```

---

### Task 6: the Unity showcase matches the contract

**Files:**

- Modify: `unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneShowcase.cs`

- [ ] **Step 1: Verify the page-key assumption on the device, first**

This is the one assumption the contract rests on and cannot be checked by
reading. Before changing any navigation:

```bash
just unity-demo-android action=cycle
adb shell input keyevent 93
adb logcat -d | grep "\[showcase\]"
```

Expected: the log shows the entry advancing. If `PAGE_DOWN` does not reach
`KeyCode.PageDown`, stop and pick another non-colliding pair — `KEYCODE_TAB`
(61) and `KEYCODE_GRAVE` (68) are candidates — and record the substitution in
the contract record before continuing.

- [ ] **Step 2: Move navigation off the arrow keys**

`RightArrow` / `LeftArrow` stop navigating and start driving the signal to `1.0`
/ `0.0` through `DemoScenes.Signal` from Task 2. `PageDown` / `PageUp` take over
next / previous. `Space` and `UpArrow` are unchanged.

- [ ] **Step 3: Add touch**

A horizontal drag calls `DemoScenes.Signal` with
`Mathf.Clamp01(touch.position.x / Screen.width)`. Swipe up and down navigate, a
tap runs the action, a two-finger tap rotates, a long press toggles the readout.
The same five bindings as `demo-android`.

- [ ] **Step 4: Make the readout suppressible**

A `bool _readoutVisible`, read **inside** `OnGUI` and returning early when
false. Never `enabled = false` on the behaviour: that file records that
disabling it took the frame loop down with the readout.

- [ ] **Step 5: Add capture mode**

Read the same three intent extras through
`AndroidJavaClass("com.unity3d.player.UnityPlayer")` → `currentActivity` →
`getIntent()`. Build the scene, pin the phase, set the signal, hide the readout,
draw the fixed frame count, hold.

- [ ] **Step 6: Compile the package**

Run: `just unity-editor` Expected: the package and both samples compile on
`6000.3.23f1`.

- [ ] **Step 7: Verify on the device**

Run: `just unity-demo-android action=cycle`, then the same keyevent sequence as
Task 5 step 8. Expected: the same five behaviours as `demo-android`.

- [ ] **Step 8: Commit**

```bash
git add unity/com.driftsys.dashscene/Samples~/Showcase/DashsceneShowcase.cs
git commit -m "feat(unity): the showcase takes the shared vocabulary, and touch"
```

---

### Task 7: the three harness call sites that send keyevent 22

Navigation moved off `DPAD_RIGHT` in Task 6, so anything sending 22 to walk
entries now drives the signal instead and silently measures the same scene three
times.

**Files:**

- Modify: `measure/android/unity-frame-cost.sh:135`
- Modify: `justfile:4039`
- Modify: `justfile:3979` (the printed hint)

- [ ] **Step 1: Confirm the call sites are still exactly three**

Run: `grep -rn "keyevent 22" measure/ justfile` Expected: three hits, at the
lines above. If there are more, this task covers all of them — the count in this
plan is a reading, not a licence to stop at three.

- [ ] **Step 2: Change 22 to 93, and check 19 is untouched**

`unity-lifecycle.sh` sends 19 for orientation, which collides with nothing and
must not change.

- [ ] **Step 3: Run the device-free harness tests**

Run: `just harness-tests` Expected: green, including `unity-android-test.sh` and
`unity-lifecycle-test.sh`.

- [ ] **Step 4: Commit**

```bash
git add measure/android/unity-frame-cost.sh justfile
git commit -m "fix(measure): the harness walks entries with the page key"
```

---

### Task 8: `measure/android/host-parity.sh`

**Files:**

- Create: `measure/android/host-parity.sh`
- Create: `measure/android/host-parity-test.sh`
- Modify: `justfile` (a `host-parity` recipe, appended at the end of the android
  block)
- Modify: `measure/android/record-check.py` (re-derive the new table)

- [ ] **Step 1: Read the two scripts this one is modelled on**

Run: `sed -n '1,80p' measure/android/unity-frame-cost.sh` and
`sed -n '1,60p' measure/android/unity-lifecycle-test.sh`

Follow their structure, their `lib.sh` helpers and their argument checking. Do
not invent a third convention.

- [ ] **Step 2: Write the device-free test first**

`host-parity-test.sh` drives `host-parity.sh` against a fake `adb` on `PATH`,
the way the existing device-free tests do. Cases: a capture whose logged scene
name does not match what was asked refuses; a capture whose extent does not
match refuses; two identical captures pass; two differing captures report the
fraction, the bounds and the channel delta.

- [ ] **Step 3: Run it to verify it fails**

Run: `bash measure/android/host-parity-test.sh` Expected: FAIL —
`host-parity.sh` does not exist.

- [ ] **Step 4: Write `host-parity.sh`**

Per scene, per host: launch in capture mode with the scene, phase and signal;
read back the logged entry name and extent and refuse unless both match; then
`adb exec-out screencap -p`. Then compare each pair through Task 3's
`compare_pngs` and write the table.

**Read every capture from a file, never from a pipe into `grep -q`.** A match
above the pipe buffer inverts the result under `pipefail`; this repository has
paid for that twice, once in this very directory. Use a herestring.

- [ ] **Step 5: Run the test to verify it passes**

Run: `bash measure/android/host-parity-test.sh` Expected: PASS.

- [ ] **Step 6: Mutate to prove the test bites**

Delete the extent assertion. Expected: the extent case FAILS. Revert.

- [ ] **Step 7: Commit**

```bash
just harness-tests
git add measure/android/host-parity.sh measure/android/host-parity-test.sh justfile
git commit -m "test(measure): compare what the two Android hosts draw"
```

---

### Task 9: one run condition

**Files:**

- Modify: `measure/android/unity-frame-cost.sh` (the dwell)
- Modify: `justfile` (the `android-measure` and `unity-demo-android` profiles)
- Modify: `demo-android/src/timing.rs` only if the sample size must change

- [ ] **Step 1: Raise the Unity dwell until three samples land**

Today a report covers 240 frames and each entry's dwell yields **one** sample,
so `max` carries pipeline warm-up that the lean painter's table discounts. Raise
`DS_DWELL` until three reports land per entry, and discard the first on both
sides.

- [ ] **Step 2: Verify three samples actually land**

Run: `just unity-demo-android action=cycle` and count the reported lines per
entry. Expected: at least 3. **Do not raise the dwell and assume** — the
previous attempt at this table assumed and published one sample as three.

- [ ] **Step 3: Build both hosts at `demo-release`**

`[profile.demo-release]` in `Cargo.toml` records why plain release cannot build
the producer. The lean host was measured at `release`; move it to `demo-release`
so the profile is not a free variable between the two tables.

- [ ] **Step 4: Commit**

```bash
just test
git add measure/android/unity-frame-cost.sh justfile
git commit -m "fix(measure): both hosts are measured at one profile, extent and sample size"
```

---

### Task 10: the device pass

**Files:**

- Modify: `docs/design/android-toolchain.md`
- Create: `docs/archive/2026-XX-XX-showcase-host-parity/` (the raw captures)
- Modify: `docs/decisions/the-showcase-hosts-share-one-surface.md` (the measured
  tolerance)

- [ ] **Step 1: Confirm exactly one device is attached**

Run: `adb devices` Expected: exactly one. Two attached targets make every
`android-*` recipe exit non-zero, because none of them passes `adb -s`. An
emulator counts as the second.

- [ ] **Step 2: Take the frame-cost run**

Run: `just android-measure` and `just unity-demo-android action=cycle`

- [ ] **Step 3: Take the parity run**

Run: `just host-parity`

- [ ] **Step 4: Write the tolerance the run measured**

Record the differing fraction, the bounds and the maximum channel delta per
scene, and only then set the threshold the harness enforces. **The threshold is
the measurement plus a stated margin, never a number chosen first.**

- [ ] **Step 5: Check the prediction the spec made**

`typography` should be the closest match of the three, because Unity's HLSL is
generated from the same `sdf.wgsl` and 2393 probes over 13 functions already
check it. If it is not, stop and investigate the text seam — do not widen the
tolerance to accept it.

- [ ] **Step 6: Check whether the tick ratio survives**

The Unity host's `tick` was 0.51, 0.40 and 0.27 of the Rust host's, under a
different extent and a different profile. Both are now identical. Either the
ratio reproduces, and the record gains a real finding, or it dissolves, and the
record says which variable it was. Write whichever happened.

- [ ] **Step 7: Re-derive every published cell**

Run: `python3 measure/android/record-check.py` Expected: green. No cell in
`android-toolchain.md` is transcribed by hand.

- [ ] **Step 8: Commit**

```bash
just build
git add docs/
git commit -m "docs(measure): the two Android hosts, measured and compared on one device"
```

---

## Before the pull request

- [ ] Garden `docs/wip/` into the durable records — the contract into
      `docs/decisions/`, the readings into `docs/design/android-toolchain.md` —
      and delete both `docs/wip/2026-08-29-showcase-host-parity-*.md` in the
      same commit. A record written while the raw original stays in `docs/wip/`
      is a copy, not a gardened record.
- [ ] `just build` green, and quote its Summary line.
- [ ] File the six issues under epic #1107 on the v0.21 milestone, and note in
      the epic that filing them holds the slice open past its phase-end
      revision.
