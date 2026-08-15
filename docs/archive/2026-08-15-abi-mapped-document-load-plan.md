# The C ABI's mapped document load — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** give `dashscene-ffi` a `ds_runtime_load_document_mapped` that maps a
`.dsb` from a path and bounds what it reads by the root the caller names, so R5
has an expression on the C ABI.

**Architecture:** the loader machinery already exists and is reached by two
hosts; this adds a third entry point and lifts the part all three share into
`dashscene-core`. No new machinery, one new symbol, four new status codes at the
tail of an explicitly numbered enum.

**Tech Stack:** Rust 2024, `memmap2` behind `dashbuf::map`, `flatbuffers` for
the document, `blake3` for payload identity, `cargo-nextest` as the runner.

**Spec:** `docs/wip/2026-08-15-abi-mapped-document-load-design.md`

## Global constraints

- `DS_ABI_VERSION` **stays 1**. Adding a symbol and appending `DsStatus`
  variants does not move it; changing a shipped signature or renumbering a
  variant does. Nothing in this plan does either.
- New `DsStatus` variants go at the **tail**, numbered explicitly. `Atlas = 10`
  is the current last.
- No panic crosses the boundary: every entry point's body runs inside `guard`.
- Every pointer that must be non-null is checked and reported as
  `DsStatus::NullArgument` rather than dereferenced.
- Failures are reported as a `DsStatus`; `ds_last_error_message` carries prose
  and no promises.
- Commit scope is the crate name, from `.git-std.toml` — `dashscene-core`,
  `dashscene-ffi`, `dashscene-desktop`, and `docs` for prose.
- Prose in this repo is plain literal English; no idioms.

## File structure

- `crates/dashscene-core/src/load.rs` — **modify.** Gains
  `first_derived_payload` and `show_appended_root`, the two steps all three
  mapped loaders share.
- `crates/dashscene-desktop/src/document.rs` — **modify.** `Document::load`
  drops its inline copies and calls the two.
- `crates/dashscene-web/src/document.rs` — **modify.** Same.
- `crates/dashscene-ffi/src/lib.rs` — **modify.** Gains the entry point, the
  private `load_mapped_into` beside `load_into`, four `DsStatus` variants, the
  module-doc rewrite, and the tests.
- `crates/dashscene-ffi/Cargo.toml` — **modify.** Gains three dev-dependencies.
- `crates/dashscene-ffi/include/dashscene.h` — **modify.** The declaration and
  the four enum entries.
- `crates/dashscene-ffi/tests/abi.c` — **modify.** Exercises the new symbol from
  C, which is what the `c-abi` gate compiles.

---

### Task 1: the shared tail moves into `dashscene-core`

**Files:**

- Modify: `crates/dashscene-core/src/load.rs`
- Test: `crates/dashscene-core/tests/mapped_load.rs` — the existing mapped-load
  test file. It already carries `fixture(name)`, which resolves a committed
  `goldens/dsb` document, and the two constants naming the only fixtures that
  carry an asset at all: `v03-paint.dsb`, whose payload is the document's own
  canonical bytes, and `v03-paint-hifi.dsb`, which carries a **derived** payload
  behind a manifest section. Those two are exactly the pair the first test
  needs, so it builds no fixture of its own.

**Interfaces:**

- Consumes: `dashbuf::Wanted`, `dashbuf::prefetch::{ShownRoot, root_count}`,
  `dashbuf::Document`, `crate::Arena`.
- Produces: `dashscene_core::first_derived_payload(doc, wanted) -> Option<u32>`
  and
  `dashscene_core::show_appended_root(doc, shown_root, roots_before, source,
  arena)`.
  Tasks 2 and 3 call both.

- [ ] **Step 1: Write the failing test**

In `crates/dashscene-core/tests/mapped_load.rs`. The two committed fixtures are
the two answers, so nothing is constructed:

```rust
/// The canonical fixture reports no derivation; the hifi one names the entry
/// whose payload the document has no name for.
///
/// The check exists because a mapped load reads no payload header, so a derived
/// payload bound as canonical would be tagged with the document's own format
/// and nothing downstream would catch it (issue #640). `v03-paint-hifi.dsb`
/// carries exactly that shape behind a manifest section, which is why
/// `RAW_WITH_ASSETS` excludes it.
#[test]
fn a_derived_payload_is_named_by_its_entry_index() {
    let raw = MappedFile::open(fixture("v03-paint.dsb")).expect("the fixture maps");
    let (document, wanted) = dashbuf::open(raw.bytes()).expect("the fixture opens");
    assert!(!wanted.is_empty(), "v03-paint.dsb carries an asset");
    assert_eq!(
        dashscene_core::first_derived_payload(&document, &wanted),
        None,
        "every payload in v03-paint.dsb is the document's own canonical bytes"
    );

    let hifi = MappedFile::open(fixture("v03-paint-hifi.dsb")).expect("the fixture maps");
    let (document, wanted) = dashbuf::open(hifi.bytes()).expect("the fixture opens");
    assert_eq!(
        dashscene_core::first_derived_payload(&document, &wanted),
        Some(0),
        "v03-paint-hifi.dsb resolves entry 0 through its derivation manifest, so the bytes a \
         host would bind are not the ones the entry names"
    );
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo nextest run -p dashscene-core a_derived_payload_is_named` Expected:
FAIL to compile — `first_derived_payload` is not a function.

- [ ] **Step 3: Add `first_derived_payload`**

In `crates/dashscene-core/src/load.rs`:

```rust
/// The first asset entry whose bound payload is **not** the document's own.
///
/// A `.dsb` records the canonical payload's hash and never carries a
/// derivation, so a host binding `dashpack`'s output binds bytes the document
/// has no name for. The owning loader finds that out by parsing the payload's
/// header; a mapped loader reads no header by design, so nothing downstream
/// would catch a KTX2 tagged as a `Png` — which is the mistake issue #640
/// exists to prevent.
///
/// Returns the **index** rather than an error so each host names its own
/// source: `dashscene-desktop` reports a path, `dashscene-web` a URL, and
/// `dashscene-ffi` a path again. Three error types, one comparison.
pub fn first_derived_payload(doc: &Document<'_>, wanted: &[Wanted]) -> Option<u32> {
    let entries = doc.assets().unwrap_or_default();
    wanted
        .iter()
        .zip(entries.iter())
        .position(|(want, entry)| want.hash != entry.hash().bytes())
        .map(|index| index as u32)
}
```

- [ ] **Step 4: Run it and watch it pass**

Run: `cargo nextest run -p dashscene-core a_derived_payload_is_named` Expected:
PASS.

- [ ] **Step 5: Write the failing test for the second function**

A bare node makes the arena hold a root before the load, which is the only
condition under which the ordinal and the arena index can disagree. It carries
no fill, so it adds no image row and `Txn::use_mapped_pool` still accepts the
arena:

```rust
/// The ordinal a host names is a *document* ordinal, and the load appends to
/// whatever the arena already holds — so the two agree only when it held
/// nothing. Issue #943 is that correction, and this is the test that keeps it.
///
/// Ordinal 0 over an arena that already holds a root is the whole case: read as
/// an arena index it names the pre-existing node, and read correctly it names
/// the one this load appended.
#[test]
fn the_shown_root_is_the_appended_one_not_the_arenas_first() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    // No fill, so the image table stays empty and the mapped pool is still
    // available to the load below.
    let existing = txn.add_node(None, Some("already here"));
    txn.commit();
    let roots_before = arena.roots().len();
    assert_eq!(roots_before, 1, "the arena holds one root before the load");

    let mapped = Arc::new(MappedFile::open(fixture("v03-paint.dsb")).expect("the fixture maps"));
    let (document, payloads) = plan_over(mapped.bytes());
    let region: Arc<dyn Region> = mapped.clone();
    load_document_mapped(&document, region, &payloads, &mut arena);

    dashscene_core::show_appended_root(
        &document,
        dashbuf::prefetch::ShownRoot::nth(0),
        roots_before,
        &"v03-paint.dsb",
        &mut arena,
    );

    let shown = arena
        .committed()
        .shown_root()
        .expect("the commit named a shown root");
    assert_ne!(
        shown, existing,
        "ordinal 0 was read as an arena index and named the pre-existing root, so the wrong \
         artboard would be solved and painted (issue #943)"
    );
    assert_eq!(
        shown,
        arena.roots()[roots_before],
        "ordinal 0 names the first root this load appended"
    );
}
```

- [ ] **Step 6: Run it and watch it fail**

Run: `cargo nextest run -p dashscene-core the_shown_root_is_the_appended_one`
Expected: FAIL to compile — `show_appended_root` is not a function.

- [ ] **Step 7: Add `show_appended_root`**

```rust
/// Confines the traversal to the root `shown_root` names, correcting the
/// document ordinal into the arena node it actually named.
///
/// `roots_before` is how many roots the arena held before this load. The load
/// appends, so a document ordinal and an arena index agree only when the arena
/// held nothing; passing the ordinal straight through would confine the
/// traversal to the *first* document's root while the prefetch read this one's
/// — the wrong artboard, solved and painted, with nothing to report it (issue
/// #943).
///
/// A commit of its own rather than a parameter on the loader: the load has
/// already committed by the time this runs, and one extra commit per load is
/// cheaper than a signature change on three public loaders and every call site.
///
/// `source` is what the diagnostic below names — a path, a URL, whatever the
/// caller loaded from.
///
/// # Panics
///
/// If the ordinal names no appended root. That is `dashscene-core` promising
/// one arena root per document root and not delivering, which is an invariant
/// rather than an embedder error — P4's answer to a broken invariant is a
/// diagnostic that names it. Every caller has already proved the document
/// carries the root, through `dashbuf::prefetch::resolve`, so no honest error
/// value can describe this arm.
pub fn show_appended_root(
    doc: &Document<'_>,
    shown_root: ShownRoot,
    roots_before: usize,
    source: &dyn std::fmt::Display,
    arena: &mut Arena,
) {
    let shown = *arena
        .roots()
        .get(roots_before + shown_root.ordinal() as usize)
        .unwrap_or_else(|| {
            // Inside the closure: nothing here runs on the ordinary path, and
            // `saturating_sub` so a shrunken root list cannot replace this
            // diagnostic with a bare subtraction overflow.
            let appended = arena.roots().len().saturating_sub(roots_before);
            panic!(
                "{source} declares {} root(s) and this load appended {appended} to the arena, so \
                 ordinal {} names no node: `load_document_mapped` appends one arena root per \
                 document root, and `dashbuf::prefetch::resolve` already proved this document \
                 carries that root",
                dashbuf::prefetch::root_count(doc),
                shown_root.ordinal(),
            )
        });
    let mut txn = arena.open();
    txn.show_root(Some(shown));
    txn.commit();
}
```

- [ ] **Step 8: Run it and watch it pass**

Run: `cargo nextest run -p dashscene-core the_shown_root_is_the_appended_one`
Expected: PASS.

- [ ] **Step 9: Run the sanity tier and commit**

Run: `just test`

```bash
git add crates/dashscene-core/src/load.rs crates/dashscene-core/tests/mapped_load.rs
git commit -m "feat(dashscene-core): the mapped load's shared tail, stated once

Refs #925."
```

---

### Task 2: the two hosts adopt the shared tail

This is behaviour-preserving. Its whole verification is that the existing tests
for both hosts still pass — including the ones that assert the #943 correction.

**Files:**

- Modify: `crates/dashscene-desktop/src/document.rs` (in `Document::load`)
- Modify: `crates/dashscene-web/src/document.rs`

**Interfaces:**

- Consumes: `first_derived_payload`, `show_appended_root` from Task 1.
- Produces: nothing new. Two call sites shrink.

- [ ] **Step 1: Record the baseline**

Run: `cargo nextest run -p dashscene-desktop` Expected: PASS. Note the count —
the same tests must pass after the change, and a test that disappears is a
regression this task can otherwise hide.

- [ ] **Step 2: Replace the desktop derived-payload loop**

In `crates/dashscene-desktop/src/document.rs`, the loop reading

```rust
let entries = document.assets().unwrap_or_default();
for (want, entry) in wanted.iter().zip(entries.iter()) {
    if want.hash != entry.hash().bytes() {
        return Err(DesktopError::Derived { path: name });
    }
}
```

becomes

```rust
// One comparison, in `dashscene-core`, because `dashscene-web` and
// `dashscene-ffi` make the same one and a fourth would be a fourth place to
// correct.
if dashscene_core::first_derived_payload(&document, &wanted).is_some() {
    return Err(DesktopError::Derived { path: name });
}
```

- [ ] **Step 3: Replace the desktop tail**

The `let shown = *arena.roots().get(..)` block through `txn.commit();` becomes

```rust
dashscene_core::show_appended_root(
    &document,
    shown_root,
    roots_before,
    &self.path.display(),
    arena,
);
```

Delete the now-unused inline panic and its comment block; the argument it
carried has moved onto `show_appended_root` verbatim, so nothing is lost.

- [ ] **Step 4: Run the desktop tests**

Run: `cargo nextest run -p dashscene-desktop` Expected: PASS, with the same test
count as Step 1.

- [ ] **Step 5: Make the same two replacements in `dashscene-web`**

The derived loop becomes the same `first_derived_payload` call returning
`WebError::Derived(url.to_owned())`; the tail becomes

```rust
dashscene_core::show_appended_root(&document, shown_root, roots_before, &url, arena);
```

- [ ] **Step 6: Compile the browser half**

Run: `just wasm-host` Expected: success. `dashscene-web`'s browser half compiles
on no other target, so the host-target build alone does not cover this edit.

- [ ] **Step 7: Run the regression tier and commit**

Run: `just test-regression`

```bash
git add crates/dashscene-desktop/src/document.rs crates/dashscene-web/src/document.rs
git commit -m "refactor(dashscene-desktop): both hosts read the shared mapped tail

Refs #925."
```

---

### Task 3: the ABI entry point

**Files:**

- Modify: `crates/dashscene-ffi/src/lib.rs`
- Modify: `crates/dashscene-ffi/Cargo.toml`

**Interfaces:**

- Consumes: Task 1's two functions; the existing `guard`, `set_last_error`,
  `faces_from_c`, `load_into`, `DsFontFace`, `DsRuntime`.
- Produces: `ds_runtime_load_document_mapped`, and
  `DsStatus::{Map = 11,
  NoSuchRoot = 12, Derived = 13, Payload = 14}`. Task 4
  declares both in C.

- [ ] **Step 1: Add the dev-dependencies**

In `crates/dashscene-ffi/Cargo.toml`, mirroring `dashscene-desktop`'s set and
its reasons:

```toml
[dev-dependencies]
# The mapped-load tests need a file to map, and a mapping is of a path — so the
# one thing they cannot do is work from bytes in memory.
tempfile.workspace = true
# Those tests build a two-root document, which no committed fixture is: every
# `goldens/dsb` fixture has one root, and a one-root document cannot tell "the
# shown root's assets" apart from "every asset in the file".
flatbuffers.workspace = true
# An `AssetEntry` names its payload's canonical BLAKE3, so building a document
# by hand means computing one.
blake3.workspace = true
```

- [ ] **Step 2: Add the four status variants**

At the tail of `DsStatus`, after `Atlas = 10`:

```rust
/// The path could not be used: it is missing, unreadable, empty, or not
/// UTF-8. Only [`ds_runtime_load_document_mapped`] reports it.
Map = 11,
/// The ordinal names no root in this document. The message from
/// [`ds_last_error_message`] carries the ordinal asked for and the count
/// the document does carry, which is what tells an out-of-range ask from a
/// document with no roots at all.
NoSuchRoot = 12,
/// The file's payloads are derivations rather than the document's own
/// canonical bytes. The mapped path reads no payload header, so binding
/// them would tag one format as another with nothing downstream to catch
/// it (issue #640); the file is refused instead.
Derived = 13,
/// An asset the shown root draws did not hash to what its entry names.
Payload = 14,
```

- [ ] **Step 3: Write the failing test — the bound, proved positively**

In `crates/dashscene-ffi/src/lib.rs`'s `mod tests`, with
`two_root_document(corrupt: usize)` copied from
`crates/dashscene-desktop/src/document.rs` (see Step 8 — the duplication is
tracked, not ignored):

```rust
/// Loading bounded to the healthy root succeeds **because** the other root's
/// payload is never touched. The fixture's unshown root carries a corrupted
/// payload, so a load that read the whole table could not return `Ok`.
#[test]
fn a_mapped_load_reads_only_the_shown_root() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("two-root.dsb");
    std::fs::write(&path, two_root_document(1)).expect("the fixture writes");
    let c_path = std::ffi::CString::new(path.to_str().unwrap()).unwrap();

    let mut runtime = std::ptr::null_mut();
    assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
    assert_eq!(
        unsafe {
            ds_runtime_load_document_mapped(
                runtime,
                c_path.as_ptr(),
                0,
                std::ptr::null(),
                0,
            )
        },
        DsStatus::Ok,
        "root 1's payload is one byte wrong, and bounding the load to root 0 \
         must never read it"
    );
    unsafe { ds_runtime_free(runtime) };
}

/// And the other direction: the residency check is reached at all. Without
/// this, the test above would also pass if nothing were ever verified.
#[test]
fn a_corrupt_payload_in_the_shown_root_is_refused() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("corrupt.dsb");
    std::fs::write(&path, two_root_document(0)).expect("the fixture writes");
    let c_path = std::ffi::CString::new(path.to_str().unwrap()).unwrap();

    let mut runtime = std::ptr::null_mut();
    assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
    assert_eq!(
        unsafe {
            ds_runtime_load_document_mapped(
                runtime,
                c_path.as_ptr(),
                0,
                std::ptr::null(),
                0,
            )
        },
        DsStatus::Payload
    );
    unsafe { ds_runtime_free(runtime) };
}

/// An ordinal past the last root is refused, and the message says what the
/// document does carry.
#[test]
fn an_ordinal_past_the_last_root_is_refused() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("two-root.dsb");
    std::fs::write(&path, two_root_document(0)).expect("the fixture writes");
    let c_path = std::ffi::CString::new(path.to_str().unwrap()).unwrap();

    let mut runtime = std::ptr::null_mut();
    assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
    assert_eq!(
        unsafe {
            ds_runtime_load_document_mapped(
                runtime,
                c_path.as_ptr(),
                7,
                std::ptr::null(),
                0,
            )
        },
        DsStatus::NoSuchRoot
    );
    let mut buffer = [0u8; 256];
    let written = unsafe { ds_last_error_message(buffer.as_mut_ptr().cast(), buffer.len()) };
    let message = std::str::from_utf8(&buffer[..written]).expect("the message is UTF-8");
    assert!(
        message.contains('2'),
        "the message must name the count the document carries, and said: {message}"
    );
    unsafe { ds_runtime_free(runtime) };
}

/// A null path is a status, not a dereference.
#[test]
fn a_null_path_is_a_status_and_not_a_dereference() {
    let mut runtime = std::ptr::null_mut();
    assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
    assert_eq!(
        unsafe {
            ds_runtime_load_document_mapped(
                runtime,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
            )
        },
        DsStatus::NullArgument
    );
    unsafe { ds_runtime_free(runtime) };
}

/// A path nothing is at reports `Map` rather than opening an empty document.
#[test]
fn a_path_that_does_not_exist_reports_map() {
    let c_path = std::ffi::CString::new("/nonexistent/no-such.dsb").unwrap();
    let mut runtime = std::ptr::null_mut();
    assert_eq!(unsafe { ds_runtime_new(&mut runtime) }, DsStatus::Ok);
    assert_eq!(
        unsafe {
            ds_runtime_load_document_mapped(
                runtime,
                c_path.as_ptr(),
                0,
                std::ptr::null(),
                0,
            )
        },
        DsStatus::Map
    );
    unsafe { ds_runtime_free(runtime) };
}
```

- [ ] **Step 4: Run them and watch them fail**

Run: `cargo nextest run -p dashscene-ffi` Expected: FAIL to compile —
`ds_runtime_load_document_mapped` is not a function.

- [ ] **Step 5: Add the imports the loader needs**

`dashscene-ffi` already depends on every crate involved, but its `lib.rs` names
none of these types yet. At the top, beside the existing `use` lines:

```rust
use std::sync::Arc;

use dashbuf::map::MappedFile;
use dashbuf::prefetch::ShownRoot;
use dashbuf::residency::BlobResidency;
use dashscene_core::{MappedPayload, Region};
```

- [ ] **Step 6: Add the private loader beside `load_into`**

```rust
/// The mapped load, bounded by `shown_root`.
///
/// Every failure this returns is raised **before** the runtime's arena is
/// replaced, so a refused load leaves the previously loaded document drawable.
/// That ordering is the reason the arena assignment sits where it does rather
/// than beside the other setup.
fn load_mapped_into(
    runtime: &mut DsRuntime,
    path: &str,
    shown_root: ShownRoot,
    text: Option<TextResources>,
) -> DsStatus {
    let file = match MappedFile::open(path) {
        Ok(file) => Arc::new(file),
        Err(error) => {
            set_last_error(format!("{path}: {error}"));
            return DsStatus::Map;
        }
    };
    let bytes = file.bytes();

    // Reads the envelope, every structured section and the binding, and stops
    // at where each payload lies. No blob page is faulted in by this call.
    let (document, wanted) = match dashbuf::open(bytes) {
        Ok(opened) => opened,
        Err(error) => {
            set_last_error(format!("{error:?}"));
            return DsStatus::Open;
        }
    };

    let report = dashscene_validator::validate_document(&document);
    if report.has_errors() {
        set_last_error(format!("{report:?}"));
        return DsStatus::Gate;
    }

    if let Some(index) = dashscene_core::first_derived_payload(&document, &wanted) {
        set_last_error(format!(
            "{path}: asset entry {index}'s payload is a derivation, and this path reads no \
             payload header, so binding it would tag one format as another"
        ));
        return DsStatus::Derived;
    }

    let root = match dashbuf::prefetch::resolve(&document, shown_root) {
        Some(root) => root,
        None => {
            set_last_error(format!(
                "{path}: ordinal {} names no root, and the document carries {}",
                shown_root.ordinal(),
                dashbuf::prefetch::root_count(&document),
            ));
            return DsStatus::NoSuchRoot;
        }
    };

    // The whole of what this reads out of the file's cold half: the assets the
    // shown root's subtree draws, proven one at a time. Everything else stays
    // cold, which is what makes cold start track the root being drawn rather
    // than the file's size (R5).
    let residency = BlobResidency::new();
    for index in dashbuf::prefetch::assets_of_root(&document, root) {
        let want = &wanted[index as usize];
        let payload = &bytes[want.range.start as usize..want.range.end as usize];
        if let Err(error) = residency.touch(want, payload) {
            set_last_error(format!("{error:?}"));
            return DsStatus::Payload;
        }
    }

    // One `MappedPayload` per asset entry, in entry order — which is exactly
    // the order `dashbuf::open` returns its `Wanted`s in.
    let payloads: Vec<MappedPayload> = wanted
        .iter()
        .map(|want| MappedPayload::canonical(want.range.clone()))
        .collect();

    // A fresh arena per load, so a second load does not stack a second
    // document on the first — and `Txn::use_mapped_pool` refuses an arena whose
    // image table already holds rows, whatever put them there.
    runtime.arena = Arena::new();
    let roots_before = runtime.arena.roots().len();
    // The region the table points into is this same mapping, shared rather than
    // opened again. The arena holds its own reference, so the mapping outlives
    // this function and unmaps when the arena it fed is replaced.
    let region: Arc<dyn Region> = file.clone();
    dashscene_core::load_document_mapped(&document, region, &payloads, &mut runtime.arena);
    dashscene_core::show_appended_root(
        &document,
        shown_root,
        roots_before,
        &path,
        &mut runtime.arena,
    );

    runtime.scene = Some(dashlang::attach_live(
        &mut runtime.arena,
        TaffySolver::boxed(text),
    ));
    if let Some(surface) = runtime.surface.as_mut() {
        surface.document_replaced();
    }
    DsStatus::Ok
}
```

- [ ] **Step 7: Add the entry point**

```rust
/// Loads a `.dsb` by **mapping** it from `path`, bounded by the root
/// `shown_root` names.
///
/// This is the bounded counterpart of [`ds_runtime_load_document`]. The file is
/// mapped rather than read, no payload is copied, and the only bytes touched
/// out of the file's cold half are the assets the shown root's subtree draws —
/// so the cost of opening a file tracks the artboard being shown rather than
/// the file's size. That is R5, and until this symbol the C ABI had no
/// expression of it.
///
/// `shown_root` is a document ordinal, and it is **required**. A caller that
/// wants every root has [`ds_runtime_load_document`] and pays the owning cost
/// knowingly; there is no sentinel here, because a bound that can be switched
/// off reads as a bound when it is not one.
///
/// `faces` and `face_count` carry the same rule as
/// [`ds_runtime_load_document_with_text`]: a null `faces`, or a zero
/// `face_count`, loads without text, and text nodes then lay out as empty
/// leaves and draw no glyphs.
///
/// **The mapping is the runtime's, and the caller has no lifetime rule to
/// keep.** The arena holds a reference to it, and each load installs a fresh
/// arena, so the previous mapping unmaps when the previous arena drops. That is
/// the property that made a path preferable to a caller-supplied region, where
/// keeping the mapping alive would have been a contract enforced only by prose.
///
/// Adding this symbol did not move [`DS_ABI_VERSION`].
///
/// # Safety
///
/// `path` must be a NUL-terminated UTF-8 string, `runtime` must be live, and
/// `faces` must point to `face_count` readable [`DsFontFace`] whose own
/// pointers are valid for the lengths beside them.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ds_runtime_load_document_mapped(
    runtime: *mut DsRuntime,
    path: *const c_char,
    shown_root: u32,
    faces: *const DsFontFace,
    face_count: usize,
) -> DsStatus {
    guard(|| {
        if runtime.is_null() || path.is_null() {
            set_last_error("ds_runtime_load_document_mapped: runtime or path is null");
            return DsStatus::NullArgument;
        }
        if faces.is_null() && face_count != 0 {
            set_last_error(
                "ds_runtime_load_document_mapped: faces is null but face_count is not 0",
            );
            return DsStatus::NullArgument;
        }
        let runtime = unsafe { &mut *runtime };
        let path = match unsafe { std::ffi::CStr::from_ptr(path) }.to_str() {
            Ok(path) => path,
            Err(_) => {
                set_last_error("ds_runtime_load_document_mapped: path is not UTF-8");
                return DsStatus::Map;
            }
        };

        // The faces are read and assembled BEFORE the file is opened, so a bad
        // cascade is reported as itself rather than as whatever the document
        // turned out to be — the ordering `ds_runtime_load_document_with_text`
        // established and `tests/abi.c` depends on.
        let text = match unsafe { text_from_c(faces, face_count) } {
            Ok(text) => text,
            Err(status) => return status,
        };
        load_mapped_into(runtime, path, ShownRoot::nth(shown_root), text)
    })
}
```

- [ ] **Step 8: Lift the face assembly both entry points now share**

`ds_runtime_load_document_with_text`'s face block and the one above are the same
code. Extract it once, and have both call it:

```rust
/// The faces a caller supplied, assembled — or the status that says why they
/// could not be. `None` is the measure-only cascade: no faces were offered.
///
/// # Safety
///
/// As the entry points that call it.
unsafe fn text_from_c(
    faces: *const DsFontFace,
    face_count: usize,
) -> Result<Option<TextResources>, DsStatus> {
    if face_count == 0 {
        return Ok(None);
    }
    let described = unsafe { faces_from_c(faces, face_count) }?;
    match TextResources::from_faces(described) {
        Ok(text) => Ok(Some(text)),
        Err(error) => {
            set_last_error(format!("{error}"));
            Err(match error {
                TextResourcesError::Atlas { .. } | TextResourcesError::MixedAtlases => {
                    DsStatus::Atlas
                }
                TextResourcesError::NoFaces
                | TextResourcesError::EmptyFamily { .. }
                | TextResourcesError::Font { .. } => DsStatus::FontFace,
                _ => DsStatus::FontFace,
            })
        }
    }
}
```

Then `ds_runtime_load_document_with_text`'s body becomes
`let text = match unsafe { text_from_c(faces, face_count) } { Ok(text) => text,
Err(status) => return status };`
followed by its existing `load_into` call.

- [ ] **Step 9: Run the tests and watch them pass**

Run: `cargo nextest run -p dashscene-ffi` Expected: PASS, including the two
existing `_with_text` tests — Step 7 changed that path, so a regression there is
this step's to catch.

- [ ] **Step 10: File the fixture duplication as debt**

`two_root_document` now exists in `dashscene-desktop` and `dashscene-ffi`. It is
a test fixture rather than the production recipe, and this workspace declares no
`[features]` on any crate, so the sharing mechanism would be a first — which is
not this change's to introduce. File it so the second copy is tracked:

```bash
gh issue create --label debt --milestone "v0.23 — rolling quick debt" \
  --title "two_root_document is built twice, in dashscene-desktop and dashscene-ffi"
```

- [ ] **Step 11: Commit**

```bash
git add crates/dashscene-ffi/src/lib.rs crates/dashscene-ffi/Cargo.toml
git commit -m "feat(dashscene-ffi): a mapped document load bounded by the shown root

Refs #925."
```

---

### Task 4: the C header, the C test, and the prose this makes false

**Files:**

- Modify: `crates/dashscene-ffi/include/dashscene.h`
- Modify: `crates/dashscene-ffi/tests/abi.c`
- Modify: `crates/dashscene-ffi/src/lib.rs` (module docs only)

**Interfaces:**

- Consumes: Task 3's symbol and variants.
- Produces: the declaration the `c-abi` gate compiles.

- [ ] **Step 1: Add the four enum entries to the header**

After `DS_STATUS_ATLAS = 10`, matching the file's existing naming, with the same
numbering.

- [ ] **Step 2: Declare the symbol**

```c
/*
 * Loads a .dsb by mapping it from a path, bounded by the root that is shown.
 *
 * The bounded counterpart of ds_runtime_load_document. The file is mapped
 * rather than read and no payload is copied, so the cost of opening tracks the
 * artboard shown rather than the file's size.
 *
 * shown_root is a document ordinal and is REQUIRED. A caller that wants every
 * root has ds_runtime_load_document and pays the owning cost knowingly.
 *
 * faces carries the same rule as ds_runtime_load_document_with_text: a NULL
 * faces, or a zero face_count, loads without text.
 *
 * THE MAPPING IS THE RUNTIME'S. You keep no lifetime rule: the arena holds the
 * mapping, and each load installs a fresh arena.
 *
 * Adding this symbol did not move DS_ABI_VERSION.
 */
DsStatus ds_runtime_load_document_mapped(DsRuntime *runtime, const char *path,
                                         uint32_t shown_root,
                                         const DsFontFace *faces,
                                         size_t face_count);
```

- [ ] **Step 3: Exercise it from C**

In `crates/dashscene-ffi/tests/abi.c`, add a case asserting a null path returns
`DS_STATUS_NULL_ARGUMENT` and a nonexistent path returns `DS_STATUS_MAP`. This
is what makes the declaration compile-checked against the Rust half rather than
merely present.

- [ ] **Step 4: Run the C gate**

Run: `just c-abi` Expected: success. Needs a C toolchain; if absent, say so
rather than reporting the gate as passed.

- [ ] **Step 5: Rewrite the module docs that this makes false**

Two passages in `crates/dashscene-ffi/src/lib.rs`:

- The paragraph ending "the shape that costs nothing is the shape that also
  bounds the load, and doing them together is why neither is here yet" — rewrite
  to describe what now exists, keeping the reasoning about _why_ the shape is a
  new symbol rather than a parameter, which is still the record of the decision.
- `ds_runtime_load_document`'s "A mapped load belongs with the platform host
  that has the file (story #841)" — replace with a pointer to the new symbol.
  This is the stale deferral issue #925 was filed about, and it is also in the
  header at the same wording.

- [ ] **Step 6: Verify no stale deferral survives**

Run: `grep -rn "story #841" crates/dashscene-ffi/` Expected: no line that defers
a mapped load to it.

- [ ] **Step 7: Commit**

```bash
git add crates/dashscene-ffi/
git commit -m "docs(dashscene-ffi): the header declares the mapped load, and the deferral goes

Refs #925."
```

---

### Task 5: garden, and settle the issues

**Files:**

- Create: `docs/design/` and/or `docs/decisions/` records
- Move: this plan and its spec to `docs/archive/`

- [ ] **Step 1: Write the durable record**

Edit `docs/design/host-integration.md` — the as-built record covering the
integration surface, the C ABI included — to describe the new entry point and
what it bounds. Edit D2 in `docs/decisions/host-integration-in-three-layers.md`
where it describes the ABI's load. Both **in place**: a decision that changes an
existing record is recorded there, not in a second record beside it.

Check `docs/features.md` in the same pass, against the code rather than against
these records. It asserts feature by feature what is built, and no test fails
when one of its assertions goes stale.

- [ ] **Step 2: Archive the working memory**

```bash
git mv docs/wip/2026-08-15-abi-mapped-document-load-design.md docs/archive/
git mv docs/wip/2026-08-15-abi-mapped-document-load-plan.md docs/archive/
```

Both moves and the durable record land in **one commit** — a record written
while the original stays in `docs/wip/` is a copy, not a gardening.

- [ ] **Step 3: Amend #945's premise**

Its body states the stale-upload defect goes live when a root selection reaches
the ABI. It does not, for the reason the spec records: the root is named only at
load, and `load_into` already reports `document_replaced` after it. Amend the
issue rather than closing it — it remains a real de-duplication.

- [ ] **Step 4: File the Android JNI counterpart**

```bash
gh issue create --label debt --milestone "v0.20 — hardening: the critical findings and the Android recovery path" \
  --title "The Android JNI has no mapped load, so the ABI's bounded path has no caller"
```

- [ ] **Step 5: Run the full local gate**

Run: `just build`, and quote its `Summary` line rather than asserting that tests
passed.

Run: `just android`. `dashscene-ffi` is the crate Android's host sits on, so its
cross-compile is part of this change's gate. Needs an NDK; if absent, say so
rather than reporting the gate as passed.

Then check whether the `packer` path filter in `.github/workflows/ci.yml` names
`crates/dashscene-core/` — Task 1 modifies it — and run `just calibrate` before
merging if it does.

- [ ] **Step 6: Open the PR**

Not a draft. Then run `/code-review` on the PR number while CI runs, and capture
every finding as a checklist in the PR description.
