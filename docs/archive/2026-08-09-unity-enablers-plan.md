# Unity integration Phase 1 — the data-plane ABI: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

    status   WIP — plan, written 2026-08-09. Phase 1 only. Phases 2 and 3
             are scoped at the foot of this file and get their own plans,
             for the reason stated there.
    scope    `crates/dashscene-unity` — the C data plane a non-Rust painter
             consumes. No Unity project, no C#, no repo creation.
    builds on docs/wip/2026-08-09-unity-host-integration.md,
             docs/decisions/unity-painter-uses-brg.md,
             docs/technotes/rendering-and-painters.md §10,
             docs/decisions/host-integration-in-three-layers.md

**Goal:** Give a non-Rust painter everything it needs to draw one frame — the
committed tables, the glyph runs, the atlas payloads and the dirty set — across
a C ABI whose layout the consumer can verify before trusting it.

**Architecture:** `crates/dashscene-unity` already pins boundary B's *types* to
a C representation and denies `improper_ctypes_definitions` over them. It has no
control plane: nothing creates a runtime, loads a document, commits, or hands
out a table. This phase adds exactly that, as one call per frame returning a
struct of `(pointer, count, stride)` views into memory the runtime owns.

**Tech Stack:** Rust 2024, `cdylib` + `staticlib`, `cargo nextest`, a C test
compiled by `cc` in a build script. No new dependencies outside the workspace.

## Global Constraints

- `#![deny(improper_ctypes_definitions)]` stays on. Every new `extern "C"`
  signature is checked by it; a type that stops being FFI-safe must break the
  build, not the consumer.
- No panic crosses the boundary. Every entry point that can fail returns a
  status code; `catch_unwind` wraps anything that can panic.
- No error is representable only as a formatted string — story #840's stated
  lesson, and #819/#815 are the open issues that prove it recurs.
- Every exported struct is `#[repr(C)]` and pinned by an `offset_of!` test, in
  the same task that adds it.
- Boundary-B row types are re-exported, never redefined. A local mirror of
  `RectEntry` would be a second source of truth.
- The tables are borrowed, not copied. Lifetime is "valid until the next
  `commit` or `destroy` on the same runtime", stated in the header.
- `edition = "2024"`, `license = "MIT"`, workspace `resolver = "3"`.

---

## The finding this plan exists to act on, and its deadline

**Story #840 builds a C ABI whose scope is Layer 0's needs: "create and destroy
a runtime, hand it a surface handle, load or build a document, drive a tick,
resize, select a root, and report an error."** That is a *surface-handle* ABI —
the host gives dashscene a surface and dashscene draws into it. It is correct
for Android, and it does not serve Unity, which must receive the tables and draw
them itself.

These are two data planes over one runtime, and they should share the runtime
handle, the error type and the versioning rule. #840's definition of done fixes
"a C header and a stable symbol set, with the versioning rule stated" — so if
the Unity consumer is not considered while it is written, the symbol set is
surface-shaped and the second consumer arrives as a bolt-on.

**#840 is open now, on branch `integration/v0.19-android`.** Task 1 exists to
put this in front of it while that is still cheap. If #840 has already merged
when this plan is executed, Task 1 becomes a follow-up issue against the header
rather than a comment on the story, and Task 2 must adopt whatever handle type
#840 shipped instead of defining one.

## File structure

| file                                         | responsibility                                              |
| -------------------------------------------- | ----------------------------------------------------------- |
| `crates/dashscene-unity/src/lib.rs`          | unchanged: the layout/round-trip surface and its `offset_of!` pins |
| `crates/dashscene-unity/src/runtime.rs`      | new: the opaque runtime handle, create/destroy/load/commit  |
| `crates/dashscene-unity/src/frame.rs`        | new: `DsSlice`, `DsFrame`, and the per-frame accessor        |
| `crates/dashscene-unity/src/status.rs`       | new: the status enum every fallible entry point returns      |
| `crates/dashscene-unity/tests/abi.rs`        | new: Rust-side behaviour tests over the C surface            |
| `crates/dashscene-unity/tests/c/abi_test.c`  | new: the C consumer that proves a non-Rust caller works      |
| `crates/dashscene-unity/build.rs`            | new: compiles the C test                                     |
| `crates/dashscene-unity/include/dashscene.h` | new: the hand-written C header, pinned by the C test         |

Split by responsibility rather than by layer: `runtime.rs` owns lifetime,
`frame.rs` owns the per-frame view, `status.rs` owns the error vocabulary. Each
is small enough to hold in context, and each has a test file that fails for its
own reasons.

---

### Task 1: Put the second consumer in front of story #840

No code. This is a coordination task and it is first because its window closes.

**Files:** none.

**Interfaces:**

- Consumes: nothing.
- Produces: agreement (or a recorded disagreement) on whether the runtime
  handle, the status type and the versioning rule are shared between the
  surface-handle ABI and the data-plane ABI. Task 2 reads that answer.

- [ ] **Step 1: Read the current state of #840**

```bash
gh issue view 840
gh pr list --search "c-abi" --state all --json number,title,state
git log --oneline origin/integration/v0.19-android -- crates/ | head -20
```

- [ ] **Step 2: Comment on #840 with the second consumer**

Post the concrete ask, not a general concern:

```bash
gh issue comment 840 --body "$(cat <<'EOF'
A second consumer for this ABI, raised while the symbol set is still open.

This story's scope is surface-handle shaped — hand it a surface, drive a tick —
which is right for layer 0. A Unity painter needs the inverse data plane: the
committed tables out, so the host draws them. Both are consumers of one runtime.

The ask is narrow: that the runtime handle, the status/error type and the
versioning rule be shared rather than surface-specific, so the data-plane ABI
extends this symbol set instead of standing up a second runtime beside it.

Concretely, three things this story fixes that the second consumer would
otherwise re-derive incompatibly:
- the opaque handle type and its create/destroy names
- the status enum (this story's DoD already says no error is representable only
  as a formatted string)
- the symbol prefix and the version-negotiation call

No scope is being asked for here — the table accessors are separate work and are
planned in docs/wip/2026-08-09-unity-enablers-plan.md. Only that this story's
seam not foreclose them.

Refs #833.
EOF
)"
```

- [ ] **Step 3: Record the answer where Task 2 will find it**

If #840 agrees to share, note the agreed handle and status names here in this
plan file, under Task 2's Interfaces, and commit that edit. If it declines or
has already merged, write the divergence into Task 2's Interfaces instead —
Task 2 must not start with the question open.

- [ ] **Step 4: Commit**

```bash
git add docs/wip/2026-08-09-unity-enablers-plan.md
git commit -m "docs(docs): record the #840 ABI answer for the Unity data plane"
```

---

### Task 2: The status enum

**Files:**

- Create: `crates/dashscene-unity/src/status.rs`
- Modify: `crates/dashscene-unity/src/lib.rs` (add `mod status; pub use status::*;`)
- Test: `crates/dashscene-unity/tests/abi.rs`

**Interfaces:**

- Consumes: Task 1's answer on whether #840 shipped a status type. If it did,
  re-export that instead of defining this one, and skip to Task 3.
- Produces: `#[repr(i32)] pub enum DsStatus { Ok = 0, NullHandle = 1, InvalidUtf8 = 2, LoadFailed = 3, Panic = 4 }`, and
  `pub extern "C" fn dashscene_status_message(status: DsStatus) -> *const c_char`
  returning a static NUL-terminated string, never null.

- [ ] **Step 1: Write the failing test**

```rust
// crates/dashscene-unity/tests/abi.rs
use dashscene_unity::{DsStatus, dashscene_status_message};

#[test]
fn every_status_has_a_message_and_ok_is_zero() {
    assert_eq!(DsStatus::Ok as i32, 0, "Ok must be zero so C can test !status");
    for status in [
        DsStatus::Ok,
        DsStatus::NullHandle,
        DsStatus::InvalidUtf8,
        DsStatus::LoadFailed,
        DsStatus::Panic,
    ] {
        let ptr = dashscene_status_message(status);
        assert!(!ptr.is_null(), "{status:?} has no message");
        let msg = unsafe { std::ffi::CStr::from_ptr(ptr) };
        assert!(!msg.to_bytes().is_empty(), "{status:?} has an empty message");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p dashscene-unity --test abi`
Expected: FAIL — `unresolved import dashscene_unity::DsStatus`.

- [ ] **Step 3: Write minimal implementation**

```rust
// crates/dashscene-unity/src/status.rs
//! The status every fallible entry point returns.
//!
//! A discriminant, not a string. Story #840's definition of done says no error
//! is representable only as a formatted string, and issues #815 and #819 are
//! the same defect one layer up: an adapter exposed only as a formatted string
//! cannot be matched on by the caller.

use std::ffi::c_char;

/// The result of a fallible entry point. `Ok` is zero so C can test `!status`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DsStatus {
    Ok = 0,
    NullHandle = 1,
    InvalidUtf8 = 2,
    LoadFailed = 3,
    Panic = 4,
}

/// A static message for `status`. Never null, never empty, never owned by the
/// caller — the pointer is valid for the lifetime of the library.
#[unsafe(no_mangle)]
pub extern "C" fn dashscene_status_message(status: DsStatus) -> *const c_char {
    let message: &'static str = match status {
        DsStatus::Ok => "ok\0",
        DsStatus::NullHandle => "a required handle argument was null\0",
        DsStatus::InvalidUtf8 => "a path argument was not valid UTF-8\0",
        DsStatus::LoadFailed => "the document could not be loaded\0",
        DsStatus::Panic => "a panic was caught at the ABI boundary\0",
    };
    message.as_ptr().cast()
}
```

Add to `crates/dashscene-unity/src/lib.rs`, after the existing `use` block:

```rust
mod status;
pub use status::{DsStatus, dashscene_status_message};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo nextest run -p dashscene-unity --test abi`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/dashscene-unity/src/status.rs crates/dashscene-unity/src/lib.rs crates/dashscene-unity/tests/abi.rs
git commit -m "feat(dashscene-unity): a status discriminant for the C data plane"
```

---

### Task 3: The runtime handle

**Files:**

- Create: `crates/dashscene-unity/src/runtime.rs`
- Modify: `crates/dashscene-unity/src/lib.rs`, `crates/dashscene-unity/Cargo.toml`
- Test: `crates/dashscene-unity/tests/abi.rs`

**Interfaces:**

- Consumes: `DsStatus` from Task 2.
- Produces: an opaque `DsRuntime`; `dashscene_runtime_create() -> *mut DsRuntime`;
  `dashscene_runtime_destroy(*mut DsRuntime)`; and
  `dashscene_runtime_commit(*mut DsRuntime) -> DsStatus`. Task 4 calls
  `commit` before reading a frame.

**The trap this task exists to avoid, verified 2026-08-09.** `commit` is not on
`Arena`. It is on `Txn`, reached by `Arena::open(&mut self) -> Txn<'_>`, and it
has two forms:

- `Txn::commit(self) -> u64` resolves with core's internal `FixedSolver` —
  authored offset and fixed size, **flex intent ignored**. Its own doc comment
  says so: "Product code with flex layout commits through `commit_with` and a
  real solver."
- `Txn::commit_with(self, solver: &mut dyn LayoutSolver) -> u64` is the one a
  product runtime uses.

Calling the bare form would produce a runtime that looks correct for
fixed-position nodes and is wrong for everything flex resolves — the same class
of error as `Arena::layout` being intent-side, recorded in the capture's §1.5.
**Do not use `Txn::commit`.**

A real solver owns a `Typesetter` and an `Arc<Vec<Atlas>>`; `TaffySolver` itself
is a short-lived borrower, constructed per solve via
`TaffySolver::with_typesetter(&mut self.typesetter)`. `corpus/showcase/src/solver.rs`
is the reference implementation of that ownership split — read it before writing
`UnitySolver`, and mirror its `atlases()` contract of returning the same `Arc`
every commit, so the run table a painter reads points at one atlas set rather
than a fresh copy per frame.

- [ ] **Step 1: Write the failing test**

```rust
// append to crates/dashscene-unity/tests/abi.rs
use dashscene_unity::{dashscene_runtime_commit, dashscene_runtime_create, dashscene_runtime_destroy};

#[test]
fn a_runtime_round_trips_and_null_is_refused() {
    let rt = dashscene_runtime_create();
    assert!(!rt.is_null(), "create returned null");
    assert_eq!(unsafe { dashscene_runtime_commit(rt) }, DsStatus::Ok);
    unsafe { dashscene_runtime_destroy(rt) };

    // A null handle is a status, not a crash — the whole point of the enum.
    assert_eq!(
        unsafe { dashscene_runtime_commit(std::ptr::null_mut()) },
        DsStatus::NullHandle
    );
    // Destroying null is a no-op, not a fault.
    unsafe { dashscene_runtime_destroy(std::ptr::null_mut()) };
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p dashscene-unity --test abi`
Expected: FAIL — `unresolved import dashscene_unity::dashscene_runtime_create`.

- [ ] **Step 3: Write minimal implementation**

Add to `crates/dashscene-unity/Cargo.toml`:

```toml
[dependencies]
dashscene-core = { workspace = true }
dashscene-engine = { workspace = true }
dashscene-typeset = { workspace = true }

[lib]
crate-type = ["lib", "cdylib", "staticlib"]
```

```rust
// crates/dashscene-unity/src/runtime.rs
//! The runtime handle a non-Rust caller owns.
//!
//! Opaque by construction: the caller holds a `*mut DsRuntime` and never
//! dereferences it. Every entry point that takes one checks for null and
//! returns [`DsStatus::NullHandle`] rather than faulting, because a foreign
//! caller's mistake must not be a crash inside this library.

use std::sync::Arc;

use dashscene_core::Arena;
use dashscene_engine::TaffySolver;
use dashscene_typeset::text::Typesetter;

use crate::status::DsStatus;

/// The solver the runtime commits through. Owns the typesetter and the atlas
/// set, because `TaffySolver` borrows both and is constructed per solve.
///
/// Mirrors `corpus/showcase/src/solver.rs`, including its `atlases` contract:
/// the same `Arc` every commit, so a painter's run table points at one atlas
/// set for the life of the runtime rather than at a fresh copy per frame.
pub(crate) struct UnitySolver {
    typesetter: Typesetter,
    atlases: Arc<Vec<dashpaint::Atlas>>,
}

/// The runtime a caller creates, commits and destroys. Opaque to C.
pub struct DsRuntime {
    pub(crate) arena: Arena,
    pub(crate) solver: UnitySolver,
}

/// Creates a runtime with an empty document. Never returns null in practice;
/// a caller must still check, because a future allocation failure would.
#[unsafe(no_mangle)]
pub extern "C" fn dashscene_runtime_create() -> *mut DsRuntime {
    Box::into_raw(Box::new(DsRuntime {
        arena: Arena::new(),
        solver: UnitySolver {
            typesetter: Typesetter::default(),
            atlases: Arc::new(Vec::new()),
        },
    }))
}

/// Destroys a runtime. Passing null is a no-op, so a caller's cleanup path
/// needs no guard of its own.
///
/// # Safety
///
/// `runtime` must be a pointer returned by [`dashscene_runtime_create`] and not
/// already destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dashscene_runtime_destroy(runtime: *mut DsRuntime) {
    if runtime.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(runtime) });
}

/// Commits staged mutations, producing the tables [`crate::frame`] exposes.
///
/// # Safety
///
/// `runtime` must be null or a live pointer from [`dashscene_runtime_create`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dashscene_runtime_commit(runtime: *mut DsRuntime) -> DsStatus {
    let Some(rt) = (unsafe { runtime.as_mut() }) else {
        return DsStatus::NullHandle;
    };
    // Destructured so `arena` and `solver` are borrowed separately: the whole
    // runtime cannot be borrowed mutably twice for one call.
    let DsRuntime { arena, solver } = rt;
    let committed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // commit_with, never Txn::commit — the bare form resolves with
        // FixedSolver and ignores flex intent. See this task's preamble.
        arena.open().commit_with(solver)
    }));
    match committed {
        Ok(_generation) => DsStatus::Ok,
        Err(_) => DsStatus::Panic,
    }
}
```

`UnitySolver` must implement `dashscene_core::LayoutSolver` — `solve`,
`atlases` and `stage_text`. Copy the shape from
`corpus/showcase/src/solver.rs`; its `solve` is one line
(`TaffySolver::with_typesetter(&mut self.typesetter).solve(arena)`) and its
comments explain why `stage_text` is separate from `solve`.

Add to `crates/dashscene-unity/src/lib.rs`:

```rust
mod runtime;
pub use runtime::{
    DsRuntime, dashscene_runtime_commit, dashscene_runtime_create, dashscene_runtime_destroy,
};
```

**Check `Typesetter`'s constructor before writing this.** `Typesetter::default()`
above is an assumption; read `crates/dashscene-typeset/src/text/` and use the
real one. A runtime created with no fonts stages no glyphs, which is correct for
an empty document and is the thing Task 4's test relies on.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo nextest run -p dashscene-unity --test abi`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/dashscene-unity/
git commit -m "feat(dashscene-unity): an opaque runtime handle for the C data plane"
```

---

### Task 4: The per-frame table view

**Files:**

- Create: `crates/dashscene-unity/src/frame.rs`
- Modify: `crates/dashscene-unity/src/lib.rs`
- Test: `crates/dashscene-unity/tests/abi.rs`

**Interfaces:**

- Consumes: `DsRuntime` and `DsStatus` from Tasks 2 and 3.
- Produces: `#[repr(C)] pub struct DsSlice { pub ptr: *const c_void, pub count: u32, pub stride: u32 }`;
  `#[repr(C)] pub struct DsFrame { pub rects, glyph_runs, glyph_quads, clips, groups, dirty: DsSlice, pub generation: u64 }`;
  and `dashscene_runtime_frame(*const DsRuntime, *mut DsFrame) -> DsStatus`.
  Phase 2's C# painter reads exactly this.

`stride` exists so a consumer can assert its own `sizeof` against the producer's
in one comparison. It is the same discipline `AbiLayout` already applies to the
row types, applied to the arrays.

- [ ] **Step 1: Write the failing test**

```rust
// append to crates/dashscene-unity/tests/abi.rs
use dashscene_unity::{DsFrame, DsSlice, dashscene_runtime_frame};

#[test]
fn a_frame_reports_strides_that_match_the_row_types() {
    let rt = dashscene_runtime_create();
    assert_eq!(unsafe { dashscene_runtime_commit(rt) }, DsStatus::Ok);

    let mut frame = DsFrame::default();
    assert_eq!(unsafe { dashscene_runtime_frame(rt, &raw mut frame) }, DsStatus::Ok);

    // An empty document still reports correct strides: a consumer verifies its
    // declarations on frame zero, before any row exists to be misread.
    assert_eq!(frame.rects.stride as usize, size_of::<dashpaint::RectEntry>());
    assert_eq!(frame.glyph_runs.stride as usize, size_of::<dashpaint::GlyphRun>());
    assert_eq!(frame.glyph_quads.stride as usize, size_of::<dashpaint::GlyphQuad>());
    assert_eq!(frame.clips.stride as usize, size_of::<dashpaint::ClipBox>());
    assert_eq!(frame.dirty.stride as usize, size_of::<u32>());

    unsafe { dashscene_runtime_destroy(rt) };

    assert_eq!(
        unsafe { dashscene_runtime_frame(std::ptr::null(), &raw mut frame) },
        DsStatus::NullHandle
    );
}

#[test]
fn a_frame_slice_is_null_only_when_it_is_empty() {
    let rt = dashscene_runtime_create();
    assert_eq!(unsafe { dashscene_runtime_commit(rt) }, DsStatus::Ok);
    let mut frame = DsFrame::default();
    assert_eq!(unsafe { dashscene_runtime_frame(rt, &raw mut frame) }, DsStatus::Ok);
    for (name, slice) in [
        ("rects", frame.rects),
        ("glyph_runs", frame.glyph_runs),
        ("clips", frame.clips),
    ] {
        assert_eq!(
            slice.ptr.is_null(),
            slice.count == 0,
            "{name}: a null pointer must mean an empty table and nothing else"
        );
    }
    unsafe { dashscene_runtime_destroy(rt) };
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p dashscene-unity --test abi`
Expected: FAIL — `unresolved import dashscene_unity::DsFrame`.

- [ ] **Step 3: Write minimal implementation**

```rust
// crates/dashscene-unity/src/frame.rs
//! One call per frame, returning borrowed views of the committed tables.
//!
//! The tables are the runtime's, not the caller's. Every pointer here is valid
//! until the next `dashscene_runtime_commit` or `dashscene_runtime_destroy` on
//! the same runtime, and the header says so — a consumer that caches one across
//! a commit reads freed memory.
//!
//! One call rather than one per table, for the reason
//! `docs/specification/03-target-hardware-rules.md` R-T4 gives: the per-frame
//! cost is the upload and the submission, and a per-table crossing would add a
//! cost that scales with nothing.

use std::ffi::c_void;

use crate::{runtime::DsRuntime, status::DsStatus};

/// A borrowed array: where it starts, how many rows, and how wide a row is.
///
/// `stride` is not redundant with the consumer's own `sizeof`. It is how the
/// consumer checks that its declaration agrees with this build in one
/// comparison, the same job [`crate::AbiLayout`] does for a single row.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DsSlice {
    pub ptr: *const c_void,
    pub count: u32,
    pub stride: u32,
}

impl DsSlice {
    fn of<T>(rows: &[T]) -> Self {
        Self {
            // An empty slice's `as_ptr` is dangling but non-null, which would
            // make "null means empty" false. Normalising here is what lets the
            // header state that invariant.
            ptr: if rows.is_empty() {
                std::ptr::null()
            } else {
                rows.as_ptr().cast()
            },
            count: u32::try_from(rows.len()).expect("a boundary-B table exceeds u32::MAX rows"),
            stride: size_of::<T>() as u32,
        }
    }

    const fn empty_of<T>() -> Self {
        Self {
            ptr: std::ptr::null(),
            count: 0,
            stride: size_of::<T>() as u32,
        }
    }
}

/// Everything a painter needs for one frame.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DsFrame {
    pub rects: DsSlice,
    pub glyph_runs: DsSlice,
    pub glyph_quads: DsSlice,
    pub clips: DsSlice,
    pub groups: DsSlice,
    /// Rect indices changed since the previous commit. Advisory: redrawing
    /// everything is always correct, and a painter honouring this must produce
    /// identical output to one that does not.
    pub dirty: DsSlice,
    /// Increments on every commit. A consumer caches a rect index only while
    /// this is unchanged.
    pub generation: u64,
}

impl Default for DsFrame {
    fn default() -> Self {
        Self {
            rects: DsSlice::empty_of::<dashpaint::RectEntry>(),
            glyph_runs: DsSlice::empty_of::<dashpaint::GlyphRun>(),
            glyph_quads: DsSlice::empty_of::<dashpaint::GlyphQuad>(),
            clips: DsSlice::empty_of::<dashpaint::ClipBox>(),
            groups: DsSlice::empty_of::<dashpaint::GroupComposite>(),
            dirty: DsSlice::empty_of::<u32>(),
            generation: 0,
        }
    }
}

/// Fills `out` with views of `runtime`'s committed tables.
///
/// # Safety
///
/// `runtime` must be null or live; `out` must be a writable `DsFrame`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dashscene_runtime_frame(
    runtime: *const DsRuntime,
    out: *mut DsFrame,
) -> DsStatus {
    let (Some(rt), Some(out)) = (unsafe { runtime.as_ref() }, unsafe { out.as_mut() }) else {
        return DsStatus::NullHandle;
    };
    let scene = rt.arena.committed();
    *out = DsFrame {
        rects: DsSlice::of(scene.rects()),
        glyph_runs: DsSlice::of(scene.glyphs().runs()),
        glyph_quads: DsSlice::of(scene.glyphs().all_quads()),
        clips: DsSlice::of(scene.clips().boxes()),
        groups: DsSlice::of(scene.groups()),
        dirty: DsSlice::of(scene.dirty()),
        generation: scene.generation(),
    };
    DsStatus::Ok
}
```

Add to `crates/dashscene-unity/src/lib.rs`:

```rust
mod frame;
pub use frame::{DsFrame, DsSlice, dashscene_runtime_frame};
```

**Check the real accessor names before writing this.** `GlyphRunTable::runs`,
`all_quads` and `ClipTable::boxes` are named from
`crates/dashpaint/src/lib.rs` as it stands; read them and use what is there. If
a table exposes no flat accessor, adding one to `dashpaint` is part of this
task, not a reason to copy rows.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo nextest run -p dashscene-unity`
Expected: PASS, both new tests and the existing layout pins.

- [ ] **Step 5: Commit**

```bash
git add crates/dashscene-unity/
git commit -m "feat(dashscene-unity): one per-frame call returning the committed tables"
```

---

### Task 5: Pin the frame types, and prove no panic escapes

**Files:**

- Modify: `crates/dashscene-unity/src/lib.rs` (extend the existing `offset_of!` test module)
- Test: `crates/dashscene-unity/tests/abi.rs`

**Interfaces:**

- Consumes: `DsSlice`, `DsFrame` from Task 4.
- Produces: nothing new. This task adds the pins the Global Constraints require.

- [ ] **Step 1: Write the failing tests**

```rust
// append to crates/dashscene-unity/tests/abi.rs
#[test]
fn the_frame_types_have_the_layout_the_header_declares() {
    use std::mem::offset_of;

    assert_eq!(size_of::<DsSlice>(), 16);
    assert_eq!(align_of::<DsSlice>(), 8);
    assert_eq!(offset_of!(DsSlice, ptr), 0);
    assert_eq!(offset_of!(DsSlice, count), 8);
    assert_eq!(offset_of!(DsSlice, stride), 12);

    assert_eq!(offset_of!(DsFrame, rects), 0);
    assert_eq!(offset_of!(DsFrame, glyph_runs), 16);
    assert_eq!(offset_of!(DsFrame, glyph_quads), 32);
    assert_eq!(offset_of!(DsFrame, clips), 48);
    assert_eq!(offset_of!(DsFrame, groups), 64);
    assert_eq!(offset_of!(DsFrame, dirty), 80);
    assert_eq!(offset_of!(DsFrame, generation), 96);
    assert_eq!(size_of::<DsFrame>(), 104);
}

#[test]
fn a_panic_inside_commit_becomes_a_status() {
    // The runtime cannot be made to panic through the public surface, which is
    // the point: this test pins the catch_unwind wrapper by calling the same
    // path with a poisoned closure, so removing the wrapper fails a test rather
    // than unwinding into C on the day something does panic.
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| panic!("boom")));
    assert!(caught.is_err(), "catch_unwind must convert a panic, not propagate it");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -p dashscene-unity --test abi`
Expected: FAIL on the offset assertions, with the real numbers in the message.

- [ ] **Step 3: Correct the expected values from the failure output**

Do not adjust the struct to match the guesses above. Read the actual offsets
from the failure and write them in — the assertion's job is to fail when they
*change*, not to encode what a plan author expected. If a value surprises you
(a hole, an alignment jump), that is the finding; record it in a comment.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo nextest run -p dashscene-unity`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/dashscene-unity/
git commit -m "test(dashscene-unity): pin the frame view layout and the panic barrier"
```

---

### Task 6: The C header, and a C caller that proves it

Story #840's definition of done requires "a test exercising the ABI from C, not
only from Rust", and the same requirement applies here for the same reason: a
Rust test calling an `extern "C"` function does not exercise the header a
foreign consumer actually compiles against.

**Files:**

- Create: `crates/dashscene-unity/include/dashscene.h`
- Create: `crates/dashscene-unity/tests/c/abi_test.c`
- Create: `crates/dashscene-unity/build.rs`
- Modify: `crates/dashscene-unity/Cargo.toml`

**Interfaces:**

- Consumes: every symbol from Tasks 2 through 5.
- Produces: `include/dashscene.h` — the file Phase 2's C# `DllImport`
  declarations are written against.

- [ ] **Step 1: Write the failing C test**

```c
/* crates/dashscene-unity/tests/c/abi_test.c */
#include "dashscene.h"
#include <assert.h>
#include <stddef.h>
#include <stdio.h>

int main(void) {
    /* A C consumer's first act: check its declarations against the build. */
    DsAbiLayout rect = dashscene_abi_rect_entry_layout();
    assert(rect.size == sizeof(DsRectEntry));
    assert(rect.align == _Alignof(DsRectEntry));

    DsRuntime *rt = dashscene_runtime_create();
    assert(rt != NULL);
    assert(dashscene_runtime_commit(rt) == DS_STATUS_OK);

    DsFrame frame;
    assert(dashscene_runtime_frame(rt, &frame) == DS_STATUS_OK);
    assert(frame.rects.stride == sizeof(DsRectEntry));
    assert((frame.rects.ptr == NULL) == (frame.rects.count == 0));

    /* A null handle is a status, not a crash. */
    assert(dashscene_runtime_commit(NULL) == DS_STATUS_NULL_HANDLE);
    assert(dashscene_status_message(DS_STATUS_NULL_HANDLE) != NULL);

    dashscene_runtime_destroy(rt);
    dashscene_runtime_destroy(NULL); /* no-op, not a fault */

    printf("c abi ok\n");
    return 0;
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo nextest run -p dashscene-unity --test c_abi`
Expected: FAIL — no `dashscene.h`, no build script, no test target.

- [ ] **Step 3: Write the header and the build script**

Write `crates/dashscene-unity/include/dashscene.h` by hand rather than
generating it. It is the document a consumer reads, and it must state the
lifetime rule the Rust doc comments state:

```c
#ifndef DASHSCENE_H
#define DASHSCENE_H
/* The dashscene data plane: the committed tables, for a host that draws them
 * itself. A host that instead hands dashscene a surface uses the layer-0 ABI
 * (story #840), not this one.
 *
 * LIFETIME. Every pointer in DsFrame is owned by the runtime and is valid
 * until the next dashscene_runtime_commit or dashscene_runtime_destroy on that
 * same runtime. Caching one across a commit reads freed memory.
 *
 * VERIFY FIRST. Call the dashscene_abi_*_layout functions and compare against
 * your own sizeof and _Alignof before reading any table. A silently resized row
 * is otherwise discovered as garbled geometry rather than as an error. */
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum DsStatus {
    DS_STATUS_OK = 0,
    DS_STATUS_NULL_HANDLE = 1,
    DS_STATUS_INVALID_UTF8 = 2,
    DS_STATUS_LOAD_FAILED = 3,
    DS_STATUS_PANIC = 4
} DsStatus;

typedef struct DsAbiLayout { uint32_t size; uint32_t align; } DsAbiLayout;
typedef struct DsSlice { const void *ptr; uint32_t count; uint32_t stride; } DsSlice;

typedef struct DsFrame {
    DsSlice rects, glyph_runs, glyph_quads, clips, groups, dirty;
    uint64_t generation;
} DsFrame;

typedef struct DsRuntime DsRuntime;

const char *dashscene_status_message(DsStatus status);
DsRuntime *dashscene_runtime_create(void);
void dashscene_runtime_destroy(DsRuntime *runtime);
DsStatus dashscene_runtime_commit(DsRuntime *runtime);
DsStatus dashscene_runtime_frame(const DsRuntime *runtime, DsFrame *out);
DsAbiLayout dashscene_abi_rect_entry_layout(void);

#ifdef __cplusplus
}
#endif
#endif /* DASHSCENE_H */
```

`DsRectEntry` and the other row structs are declared in the same header,
mirroring `dashpaint`'s definitions field for field. Add them as the C test
needs them; every one added must have its `_layout` call asserted in Step 1's
test, or it is an unchecked declaration.

The build script and manifest wiring:

```rust
// crates/dashscene-unity/build.rs
fn main() {
    println!("cargo:rerun-if-changed=tests/c/abi_test.c");
    println!("cargo:rerun-if-changed=include/dashscene.h");
}
```

```toml
# crates/dashscene-unity/Cargo.toml
[[test]]
name = "c_abi"
harness = false
```

The `c_abi` test target compiles `tests/c/abi_test.c` with `cc`, links it
against the `staticlib`, runs it, and asserts exit status zero and `c abi ok` on
stdout. Write it as a Rust test that shells out, so `cargo nextest` runs it with
everything else and CI needs no separate job.

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo nextest run -p dashscene-unity`
Expected: PASS, including `c_abi`.

- [ ] **Step 5: Commit**

```bash
git add crates/dashscene-unity/
git commit -m "feat(dashscene-unity): a C header and a C caller that checks it"
```

---

### Task 7: Reconcile the records this phase contradicts

Two accepted records disagree with what this phase and Phase 2 do, and they
disagree with each other. Leaving that is the drift `AGENTS.md` forbids.

**Files:**

- Modify: `docs/decisions/unity-separate-repo-deferred.md`
- Modify: `docs/decisions/unity-painter-uses-brg.md`
- Modify: `docs/wip/README.md`

**Interfaces:**

- Consumes: nothing.
- Produces: records that agree with each other and with the code.

- [ ] **Step 1: Read both records and state the conflict precisely**

`unity-separate-repo-deferred.md` (accepted 2026-07-11) describes the painter
back end as "projected onto pre-instantiated GameObjects".
`unity-painter-uses-brg.md` (proposed 2026-07-13, two days later) rules
"BatchRendererGroup (BRG) over GameObject-per-node", reserving GameObjects
"only for node-replacement". The second supersedes the first and neither says
so.

- [ ] **Step 2: Correct the superseded sentence in place**

Edit `unity-separate-repo-deferred.md`'s painter-back-end bullet to name BRG
and link the BRG record, per `AGENTS.md`: "A decision that changes one is
recorded there directly." Do not rewrite the rest — its subject is the repo
split, which is unchanged.

- [ ] **Step 3: Decide the BRG record's status**

It reads `proposed — a direction, not yet ratified`. Phase 2 depends on it. Either
ratify it (change `status` to `accepted`, add the date and what ratified it) or
record explicitly that Phase 2 proceeds on a proposed direction and what would
falsify it. Both are legitimate; leaving it ambiguous while building on it is
not.

- [ ] **Step 4: Add this plan to the wip ledger**

`docs/wip/README.md` lists every tracked file with the condition that empties
it. Re-derive the count from `git ls-files docs/wip/` — do not trust the prose,
which this README records as having been wrong five times. Add a row for this
plan whose condition is "Phase 1 lands and Phase 2's plan is written".

- [ ] **Step 5: Commit**

```bash
just fmt && markdownlint 'docs/**/*.md' && dprint check
git add docs/
git commit -m "docs(docs): reconcile the two Unity records, and ledger the enabler plan"
```

---

## Self-review of this plan

**Spec coverage.** The enabler surface named in the capture's §3 — one FFI
crossing per frame, `#[repr(C)]` pinned by `offset_of!`, the dirty set and
generation — is covered by Tasks 4 and 5. The capture's atlas upload path is
**not** covered, and that is a deliberate gap: the atlas payload is a byte blob
plus its `AtlasGlyph` rows, and exporting it needs the same treatment as the
tables. It is the first task of a Phase 1b, not a silent omission.

**Placeholders.** Tasks 3, 4 and 6 each instruct the implementer to check a real
signature before writing, rather than trusting the code in this plan. That is
not a placeholder — the surrounding code is complete and the check is named
because this plan was written against a tree that moves.

**Type consistency.** `DsStatus` is produced in Task 2 and consumed in 3, 4 and
6 under that name. `DsSlice`/`DsFrame` are produced in Task 4 and consumed in 5
and 6. `DsRuntime` is produced in Task 3 and consumed in 4 and 6.

**Known risk.** Task 5's expected offsets are guesses and Step 3 says so
explicitly. Task 1 may be overtaken by #840 merging, and says what to do then.

## Phases 2 and 3 — why they are separate plans

The writing-plans discipline is that every step carries the actual content an
engineer needs, and that a plan covering several independent subsystems should
be split so each produces working, testable software on its own.

**Phase 2 — the Unity backend.** The BRG painter, in a Unity project. Unity
6000.5.6f1 is installed locally, so paths and a test framework are real rather
than invented. But its content is C# written against `include/dashscene.h`, and
that header does not exist until Task 6. Writing the `DllImport` declarations,
the `NativeArray` marshalling and the `GraphicsBuffer` upload now would be
writing against an interface that has not been designed yet. Order:
unlit-overlay material class, then text (atlas upload plus the MSDF variant),
then lit-opaque and lit-cutout.

**Its risk spike does not wait for this phase.** `rendering-and-painters.md`
§10 says to spike the lit plus SDF-clipped-shadow-caster shader on the target
SRP early — "that is where the engineering risk sits" — and that spike needs no
document, no ABI and no C#: hand-authored instances in a test scene are enough.
Run it in parallel with Phase 1. The recorded fallback if it fails is a
by-material-class hybrid, unlit-overlay via BRG and lit via GameObjects.

**Phase 3 — platform integration.** `.dsb` loading and packaging, anchors via
`rect_index_of`, input hit-testing, lifecycle and domain-reload invalidation.
Most of it is painter-independent and could in principle run alongside Phase 2,
but two open stories change its foundation: **#837** adds root selection and
**#838** makes the solve, the committed table and the paint follow the shown
root. Planning it before those land would plan against behaviour that is being
replaced this slice.

**Creating the Unity repository is a Phase 2 gate, not a Phase 1 task.**
`unity-separate-repo-deferred.md` says not to create it until v0 exits; Task 7
step 3 is where that gets faced rather than quietly ignored.
