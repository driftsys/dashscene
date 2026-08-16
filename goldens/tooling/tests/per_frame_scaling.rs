//! The per-frame scaling criterion — what showing one root out of a many-root
//! document costs **every frame** (story #836, epic #833).
//!
//! `startup_scaling.rs` beside this file measures the load and stops there:
//! its own D3 puts the boundary at "a committed arena with the shown root's
//! assets resident". This file starts where that one stops, because
//! `docs/decisions/the-shown-root-bounds-the-load-not-the-paint.md` names a
//! second cost under "Consequences" and leaves it unmeasured:
//!
//! > Because the engine solves every root and `dfs_order` walks all of them
//! > into one table, a document with sixty-five artboards costs sixty-five
//! > artboards of solve and committed table **per frame** while one is shown.
//! > R5 and its benchmark bound the load only. Whether this needs its own
//! > criterion is a v0.19 planning question, not one this record settles.
//!
//! It is one. Without it the per-frame half of issue #822's justification would
//! ship as an assertion, which is the shape v0.13's t2 tier spent a slice
//! removing.
//!
//! # What this measures, and why it is a count
//!
//! Three terms, all exact — and the first two identical on every machine. The
//! third is exact on any one target and **not portable**: it is a byte count, so
//! it moves with type layout and with `std`'s `Vec` growth strategy, where a
//! count of solves and a count of table rows do not. Its figures below are
//! recorded on macos aarch64 for that reason, and `report` prints the machine
//! beside every measurement.
//!
//! - **the solve** — `TaffySolver::solves()`, the number of Taffy layout
//!   computations one frame ran. `dashscene_engine`'s `compute_all` runs one
//!   per root and counts each, on the rebuild path and on the incremental one
//!   alike, so this is literally "how many artboards did this frame solve".
//!   It is one *per root*, not one per frame: the #322 baseline pass calls
//!   `compute_all` a second time when a text row's floor changes, which this
//!   fixture never triggers because it carries no text.
//! - **the committed table** — the row count of `CommittedScene::rects()`,
//!   which `Arena::dfs_order` fills from **all** roots. Every frame rebuilds
//!   it, so its size is a per-frame cost and not a load-time one.
//! - **the commit's allocation** — bytes one steady-state layout frame asks the
//!   allocator for, per root beyond the first, counted through a
//!   `#[global_allocator]` wrapping `System`. Added by story #944, which is
//!   about a cost **the two terms above cannot see**: the commit's per-node
//!   scratch was sized by the document while the solve and the table were
//!   already confined to the shown root, so both of those read 1.00x while a
//!   frame still allocated sixty-five entries in nine vectors to produce a
//!   one-row table. See [`BYTES_PER_EXTRA_ROOT`] for why this one is a slope
//!   where the others are levels, why it is the layout frame, and why it is
//!   bytes rather than a count of allocations.
//!
//! None of the three is a stopwatch, and that is deliberate.
//! `docs/decisions/startup-scaling-is-measured-by-a-counter.md` D1 rules for
//! this repository that "a cost with no visible symptom needs a counter, not a
//! stopwatch": a count is exact, identical on every machine, and either right
//! or wrong with no tolerance to argue about, where a timing ratio needs a
//! threshold that drifts and cannot run on the CI runners without flaking. The
//! same rule applies here for the same reason, so the assertions below are
//! equalities.
//!
//! A wall clock and the machine are printed beside every measurement and
//! asserted on nowhere — D6 of that record.
//!
//! # The measurement, on macos aarch64
//!
//! The document is [`common::many_root::document`], the one `startup_scaling.rs`
//! is stated over: sixty-five root frames, each a leaf drawing a distinct tile.
//! Both are measured in the steady state — after the frame that builds the
//! retained Taffy tree, which is the frame a host pays once (issue #164).
//!
//! ```text
//! document           frame         solves   rect rows   bytes
//! small-root (1)     paint-only         0           1     892
//! small-root (1)     layout             1           1     280
//! many-root (65)     paint-only         0           1   11260
//! many-root (65)     layout             1           1     280
//! ratio, many/small  layout          1.00x       1.00x   1.00x
//! slope, per extra root                                      0
//! ```
//!
//! **The layout row's byte figures are equal, not merely close.** That is the
//! third term at its strongest: the commit and the solver allocate the same
//! bytes for a sixty-five-root document as for a one-root one.
//!
//! **The paint-only row is not**, and the band does not assert on it. Its
//! difference between the two documents came out at about 162 bytes per extra
//! root on every run taken while issue #1111 was worked — unchanged by three
//! separate changes to the engine's scratch, which is what says it is
//! document-scaled rather than noise. That is not the same as calling the row
//! *stable*: its **level** does move between repeats over one unchanged
//! document (884, 884, 1284, 1172 below), which is why the term is taken on the
//! layout row and why nothing here asserts on this one. Both readings are
//! needed, and issue #1146 carries the unattributed cause.
//!
//! The byte levels are stated for completeness and asserted on nowhere. What is
//! asserted is the many-root layout frame's own figure against the one-root
//! frame's plus the slope times the extra roots — which at a slope of 0 is the
//! byte identity above, and is why the row reads 1.00x. See
//! [`BYTES_PER_EXTRA_ROOT`] for why it is stated per root and why the
//! comparison is not a per-root quotient. The paint-only row's figures are the ones
//! that move between runs of the same document, which is why the term is taken
//! on the layout row.
//!
//! # Before and after, because this band exists to state both
//!
//! Story #836 wrote this file and measured **65.00x on both terms**: 65 Taffy
//! layout computations and 65 committed rect rows per frame, against a one-root
//! document's 1 and 1. Story #838 confined the solve, the committed table and
//! the paint to the shown root, and the same measurement over the same document
//! is **1.00x on both**.
//!
//! ```text
//!                    solves        rect rows
//! before (#836)      65   65.00x   65   65.00x
//! after  (#838)       1    1.00x    1    1.00x
//! ```
//!
//! **The third term is on the same footing.** Neither term above moves when
//! the commit's per-node scratch does, so story #944 added the byte slope
//! first, measured it against the unchanged commit, and then bounded the
//! scratch.
//!
//! ```text
//!                    bytes per extra root
//! before (#944)      69
//! after  (#944)      17
//! after  (#1111)      0
//! ```
//!
//! It came down in four measured steps, each predicted before it was taken and
//! each landing on its prediction exactly: 69 to 65 when the slot-to-rect map
//! stopped being built twice, to 45 when `solved` and its carry-forward loop
//! were keyed by rect row, to 21 when the clip, mask, visibility and
//! group-opacity vectors followed, and to 17 when `dfs_order` stopped
//! reserving the whole document.
//!
//! **The last 17 were `dashscene-engine`'s**, which story #944 deliberately did
//! not touch and issue #1111 then closed, in three steps that landed on their
//! predictions exactly: 17 to 9 when `state.roots.clone()` became a borrow of a
//! disjoint field, to 8 when `incremental`'s `on_path` became a set of the
//! dirty closure rather than a flag per node, and to 0 when `baseline_pass`'s
//! `cross_offset` became a map — which allocates nothing at all on a scene with
//! no baseline text rows, where the vector it replaced was sized by the
//! document and thrown away unused.
//!
//! **The one-root document's layout frame moved in both directions on the
//! way**: 289 to 297 under story #944, because an unreserved `Vec` growing to
//! one element takes a larger minimum allocation than a one-element reserve,
//! then 297 to 280 under issue #1111, because the vectors that frame still
//! allocated became either a map that stays empty or a stamp on a vector the
//! solver already retains. The term is a difference precisely so that a level
//! moving either way in the small case cannot be mistaken for the
//! document-scaled cost it measures — over the same two changes the
//! sixty-five-root document went 4705 bytes to 280.
//!
//! The band landed first so that before-number was measured rather than
//! remembered — a band added in the same change that improves what it measures
//! cannot fail, and cannot show what the change was worth. And it is still
//! measured: [`the_confinement_is_what_makes_the_number_one`] removes the
//! confinement on every run and reports 65 again.
//!
//! **On the byte term that guard reports 119, which is neither the 69 story
//! #944 started from nor the 0 the band now holds, and it is not a
//! regression.** An unconfined commit really
//! does draw sixty-five artboards, so the rect table and everything else sized
//! by rect rows grows with it — costs the confined commit avoids by drawing
//! less, rather than scratch it was wasting. The guard's job is to show the
//! term moves when the confinement goes, and 119 against 0 shows it.
//!
//! The paint-only row did not move, and that is worth stating. It was already
//! zero at 65 roots: a commit whose changes are all paint-only solves nothing,
//! whatever the document holds, because the retained tree is not recomputed
//! (issue #164). So #838's saving is on the **layout** row and in the **table**
//! column, and claiming the paint-only zero as part of it would be claiming
//! something issue #164 had already bought.
//!
//! # The band has been shown to break
//!
//! The assertion is written once and called by the criterion and by both guards
//! committed to breach it, so none of them can drift from the others. It is two
//! predicates underneath: [`within_count_band`] holds the two count terms and
//! [`within_byte_band`] the third, because only that one needs a one-root
//! baseline and a baseline is a whole document load — so a guard breaching a
//! count alone does not pay for one (issue #1119). [`within_band`] is the band
//! itself and joins them, and it keeps that name so the only predicate reading
//! as "the band" is the one checking every term. A band nothing has been shown to break is
//! not yet a band (`docs/technotes/measured-verification.md`, "the sensitivity
//! guard").
//!
//! - [`a_paint_only_frame_that_marks_layout_intent_breaches_the_solve_term`]
//!   drives the paint-only frame with a layout property instead. The solve term
//!   goes from 0 to 1 and the band rejects it — **1, not 65: story #838
//!   confined the solve, so a layout-dirty frame over this document runs one
//!   Taffy computation whatever its root count.** That is the measurement a
//!   misclassifying `set_prop` would produce, from the same path — it does not
//!   prove the classifier is correct, which is `Arena::layout_dirty`'s own
//!   tests' job, but it does prove this band would see it.
//! - [`the_confinement_is_what_makes_the_number_one`] clears the shown root, so
//!   the runtime traverses every root exactly as it did before story #838. The
//!   two count terms go to 65 and the byte term to 136, and the band rejects
//!   all three. It replaced a guard that ran the
//!   same frames over a seventeen-root document: that one worked while the
//!   terms tracked the document's size, and cannot work now that they do not —
//!   which is the story's whole claim, restated as a test that had to be
//!   retired.
//!
//! **The upward injection story #836 said was unavailable is available now**,
//! and it is the second guard. At 65 roots the two count terms were saturated
//! at the document's own size, so nothing a test could stage made either
//! larger; with the traversal confined, removing the confinement is exactly
//! such a mutation and it breaches every term at once.
//!
//! # It was also run against the two paths it names, once
//!
//! The two guards above are committed and re-executed, which is what makes them
//! guards. This paragraph records something weaker and worth having anyway: the
//! band was demonstrated failing against the real code, by mutating each
//! measured path in the working tree and running it. Both edits were reverted;
//! neither is committed, so this is a one-off demonstration rather than a
//! standing assertion, and it is written down so it can be repeated rather than
//! believed.
//!
//! - **`dashscene_engine::compute_all`, computing each root twice.** Every
//!   solve count doubles, and the criterion fails on the one-root document's
//!   denominator before it reaches the band — which is the intended order: the
//!   denominator is what the ratio is taken against.
//! - **`Arena::dfs_order`, seeded from the first root only.** This is story
//!   #838's change in miniature. The committed table drops to 1 row on both
//!   frames of the many-root document and the band rejects it by name, saying
//!   what it says here: re-measure and move the constants, stating the before
//!   and the after.
//!
//! The second one matters most. It says the band will notice when #838 lands,
//! rather than passing through the change it exists to price.
//!
//! # What this does not measure
//!
//! **Not what a frame costs in milliseconds.** Two counts are not a frame time,
//! and nothing here converts one into the other.
//! `docs/technotes/frame-budget.md` holds the wall-clock measurements, on the
//! showcase host and on real scenes, and says the solver is not where that time
//! goes — `tick` was 0.01 to 0.03 ms against a paint of 0.76 to 16.54 ms.
//! **Both are true at once**: this criterion is about a cost that grows with
//! the document rather than with what is shown, whatever its constant is today
//! on a desktop CPU, because a tiling GPU with a fixed frame budget is where it
//! is spent.
//!
//! **Not a real scene's per-frame work.** The fixture's roots are leaves with
//! an image fill and no text, so nothing here pays the typesetter, the glyph
//! staging (`TaffySolver::stage_text`, which also walks every root) or a
//! painter. Adding any of those would put costs that are not root-scaled into a
//! number that is supposed to be about root count. A scene with text would show
//! the same 65:1 shape with a larger constant, not a different shape.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;
use std::time::Instant;

use dashbuf::map::MappedFile;
use dashpaint::Color;
use dashscene_core::{Arena, MappedPayload, NodeId, Prop, Region, load_document_mapped};
use dashscene_engine::TaffySolver;

mod common;

use common::many_root::{EXTRA_FRAMES, document};

thread_local! {
    /// Bytes this thread has asked the allocator for while [`COUNTING`] was on.
    ///
    /// Const-initialised, and `Cell` has no destructor, so touching either of
    /// these from inside `alloc` neither allocates nor registers a thread-local
    /// destructor — both of which would be re-entrant.
    static BYTES: Cell<u64> = const { Cell::new(0) };
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}

/// The system allocator, adding up request sizes on the measuring thread while
/// the measurement is on.
///
/// **Thread-local rather than a global counter**, because CI runs this binary
/// under `cargo test` (`.github/workflows/ci.yml`), which runs a binary's tests
/// as threads in one process: a global counter would report this frame plus
/// whatever the other two tests were allocating at the time. Under nextest,
/// which gives each test its own process, the two would be equivalent — the
/// suite is run both ways, so the weaker assumption is the one to hold.
///
/// `try_with` rather than `get`, so an allocation made while thread-locals are
/// being torn down reads as "not counting" instead of panicking inside the
/// allocator.
struct Counting;

impl Counting {
    fn on() -> bool {
        COUNTING.try_with(Cell::get).unwrap_or(false)
    }

    fn add(bytes: usize) {
        let _ = BYTES.try_with(|total| total.set(total.get() + bytes as u64));
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if Self::on() {
            Self::add(layout.size());
        }
        // SAFETY: the layout is passed through unchanged to the allocator this
        // one wraps, which is the only allocator any of these pointers came
        // from.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // Forwarded rather than left to the trait default, which would route
        // through `alloc` and then memset. `vec![false; n]` and `vec![0u8; n]`
        // are most of what a commit allocates, and the default would take them
        // off `System`'s lazily-zeroed pages in the one binary whose subject is
        // allocation. The byte count is the same either way.
        if Self::on() {
            Self::add(layout.size());
        }
        // SAFETY: as above.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: as above — `ptr` was produced by `System` through this
        // wrapper, with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if Self::on() {
            // The growth, not the new size: the old bytes were counted when
            // they were first asked for, and counting them again would report
            // a vector that doubled once as if it had been allocated twice.
            Self::add(new_size.saturating_sub(layout.size()));
        }
        // SAFETY: as above.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Counting on for as long as this lives, and off however it ends.
struct CountingOn;

impl CountingOn {
    fn start() -> Self {
        BYTES.set(0);
        COUNTING.set(true);
        CountingOn
    }
}

impl Drop for CountingOn {
    fn drop(&mut self) {
        COUNTING.set(false);
    }
}

/// Taffy layout computations a **paint-only** frame runs over the many-root
/// document. Zero: a commit whose changes are all paint-only marks nothing
/// dirty and the retained tree is not recomputed (issue #164).
const MANY_PAINT_SOLVES: u64 = 0;

/// Taffy layout computations a **layout** frame runs over the many-root
/// document while one root is shown. **One — it was 65 until story #838**, one
/// per root in the document, and that is half of what the story removed.
const MANY_LAYOUT_SOLVES: u64 = 1;

/// Rows the committed rect table holds over the many-root document, on every
/// frame. **One — it was 65 until story #838.** Each root of this fixture is a
/// leaf, so the shown root's subtree is one node; in general it is the node
/// count under the shown root. That is the other half of what the story
/// removed.
const MANY_RECT_ROWS: usize = 1;

/// Bytes one steady-state **layout** frame asks the allocator for, per root
/// beyond the first — the third term, and the only one of the three that can
/// see what a commit's per-node scratch costs.
///
/// **69 until story #944, 17 after it, and 0 since issue #1111.** Zero is an
/// equality, not a rounding: over the band's fixture the sixty-five-root
/// document's steady-state layout frame allocates byte-identically to the
/// one-root document's, 280 bytes each. A frame's allocation does not grow with
/// the document at all, which is the third term's whole claim and is now stated
/// at its strongest.
///
/// **This is the layout frame.** The paint-only frame is still document-scaled
/// at about 162 bytes per extra root, from a cause nothing has attributed —
/// issue #1146. That is why the sentence above says "steady-state layout frame"
/// and not "a frame".
///
/// # Why a slope where the other two terms are levels
///
/// The two above are counts of work and a level states them exactly. A byte
/// level would also hold every *fixed* allocation a frame makes — the rect
/// table, the dirty sets, Taffy's own per-solve working memory — and would move
/// whenever any of those changed, including on a dependency bump that has
/// nothing to do with what this term is about. The slope between the one-root
/// and the sixty-five-root documents cancels every one of them and leaves
/// exactly what grows with the document, which is the claim being made.
///
/// **[`within_byte_band`] compares the whole growth, not a per-root quotient.** This
/// constant times the extra-root count is the expectation, so the term is an
/// exact equality like the other two. Dividing first would truncate: over 64
/// extra roots, up to 63 bytes of new document-scaled cost divides away and the
/// band stays green, which is the failure this term exists to close.
///
/// # Why the layout frame and not the paint-only one
///
/// The paint-only frame's byte count moves across repeats over one unchanged
/// document — 884, 884, 1284, 1172 on the small one — because the paint table's
/// own interning and its pooled-entry compaction (issue #197) move with it, and
/// neither is a document-scaled cost. The layout frame repeats
/// bit-identically: 289, 1393 and 4705 bytes over the one, seventeen and
/// sixty-five-root documents, which is 69 per extra root at all three sizes.
///
/// # Why bytes and not an allocation count
///
/// Issue #944 offers either. **The count cannot see this at all**: a
/// steady-state layout frame makes exactly 21 allocation calls over the
/// one-root, seventeen-root and sixty-five-root documents alike. Only the sizes
/// differ, because what scales is the length of vectors the commit allocates
/// once each.
const BYTES_PER_EXTRA_ROOT: u64 = 0;

/// The same three over the single-root document — the denominator the ratio is
/// taken against, and what the many-root column becomes when #838 lands.
const SMALL_PAINT_SOLVES: u64 = 0;
const SMALL_LAYOUT_SOLVES: u64 = 1;
const SMALL_RECT_ROWS: usize = 1;

// Until story #838 the two constants above equalled the fixture's own root
// count, and a compile-time assertion held them to it so that shrinking the
// fixture failed by naming the fixture rather than the runtime. **That
// coincidence is what the story removed**: the band is now independent of how
// many roots the document carries, which is the whole claim, so there is
// nothing left to tie. `the_confinement_is_what_makes_the_number_one` below is
// what the tie became — it measures the document's size on purpose, by removing
// the confinement, and requires the band to reject it.

/// What one frame cost, exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameCost {
    /// Taffy layout computations the frame ran.
    solves: u64,
    /// Rows the committed rect table holds after the frame.
    rect_rows: usize,
    /// Bytes the commit asked the allocator for, counted by [`Counting`].
    bytes: u64,
}

/// A committed arena, and the temporary file its payloads are mapped from.
///
/// The mapping itself needs no field here: `load_document_mapped`'s contract is
/// that "the table adopts the region", so the arena holds it. The `TempDir`
/// does, because it deletes what it was given when it drops, and a fixture that
/// leans on a mapping outliving its unlinked file is leaning on a platform
/// detail it has no reason to bet on.
struct Loaded {
    arena: Arena,
    shown: NodeId,
    roots: usize,
    _dir: tempfile::TempDir,
}

/// Loads a document with `extra` frames beyond the shown root, the way the
/// native host does: written out, mapped, and replayed into a committed arena
/// with its payloads bound as ranges.
///
/// **The prefetch is deliberately not run here**, where `startup_scaling.rs`
/// does run it. It makes payloads resident, and residency is what that
/// criterion counts; it changes neither the node tree, the solve nor the
/// committed table, which is all this one reads. Keeping it out also keeps the
/// two boundaries distinct: everything below this function is a frame, and
/// nothing in it is a load.
fn load(extra: usize) -> Loaded {
    let file = document(extra);

    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("frames.dsb");
    std::fs::write(&path, &file).expect("the generated document writes");

    let mapped = Arc::new(MappedFile::open(&path).expect("the generated document maps"));
    let (doc, wanted) = dashbuf::open(mapped.bytes()).expect("the file opens");
    let payloads: Vec<MappedPayload> = wanted
        .iter()
        .map(|want| MappedPayload::canonical(want.range.clone()))
        .collect();

    let mut arena = Arena::new();
    let region: Arc<dyn Region> = mapped.clone();
    load_document_mapped(&doc, region, &payloads, &mut arena);
    // What a host does, and what story #838 made the measurement depend on:
    // name the root being shown, so the traversal, the solve and the paint
    // follow it. Before that story there was nothing to name and this line did
    // not exist; the numbers in the module documentation above are the two
    // sides of adding it.
    //
    // The load was into a fresh arena, so the document's first root is the
    // arena's; a host loading into a populated one converts its own ordinal
    // (issue #943).
    //
    // One binding, used for the confinement and returned as the fixture's
    // `shown`, so the root this harness names and the root its frames mutate
    // cannot drift apart.
    let shown = arena.roots()[0];
    let mut txn = arena.open();
    txn.show_root(Some(shown));
    txn.commit();

    let roots = arena.roots().len();
    assert_eq!(
        roots,
        extra + 1,
        "the fixture builds one root per frame, so a document with {extra} extra frames has \
         {} roots; a loader that nested them would make every count below a different quantity",
        extra + 1
    );

    Loaded {
        arena,
        shown,
        roots,
        _dir: dir,
    }
}

/// Runs one frame: stage `prop` on the shown root, commit through `solver`, and
/// report what that cost.
fn frame(loaded: &mut Loaded, solver: &mut TaffySolver<'_>, prop: Prop) -> FrameCost {
    let before = solver.solves();
    let mut txn = loaded.arena.open();
    txn.set_prop(loaded.shown, prop);
    // The commit and nothing else. `open` and `set_prop` are the producer's
    // side of the seam and run outside the frame loop (P3), so counting them
    // would put a producer's cost into a per-frame number.
    //
    // Turned off by a drop guard rather than by the next statement: `commit_with`
    // has several `assert!` paths, and an unwinding one would otherwise leave
    // this thread counting every later allocation, so the *next* frame measured
    // on it would report a number with no bad commit anywhere near it.
    let bytes = {
        let _counting = CountingOn::start();
        txn.commit_with(solver);
        BYTES.get()
    };
    FrameCost {
        solves: solver.solves() - before,
        rect_rows: loaded.arena.committed().rects().len(),
        bytes,
    }
}

/// Reaches the steady state: builds the retained Taffy tree and hands back the
/// solver holding it, the shown root's authored x, and what that first frame
/// cost.
///
/// The first commit through a fresh solver is a structural rebuild — it builds
/// the tree and reports every node. A host pays that once per document, so it
/// is not the per-frame cost this criterion is about; it is run here and
/// discarded, and its own cost is returned so that discarding it is visible
/// rather than silent.
///
/// **Every measurement in this file starts here, including the guards**, so
/// what the prologue produced is worth asserting rather than assuming: a guard
/// with its own copy of it could otherwise keep passing while the criterion
/// failed.
///
/// **What the anchor can and cannot tell you, since story #838** (issue #946).
/// It checked `solves == 1` and said a fresh solver that stopped rebuilding
/// would be caught. It would not be. A structural rebuild and an ordinary
/// incremental layout frame both run exactly one Taffy computation now that
/// only the shown root is computed, so the counter cannot tell them apart, and
/// the old message's "a zero here means no tree was built" named an outcome
/// `rebuild` cannot produce while `shown_roots()` is non-empty.
///
/// What is checked instead is the readback: the commit produced a rect table.
/// That is the term that goes to zero if no tree was built, and it is honest
/// about being a floor rather than a discriminator.
///
/// A solver with no typesetter, because the fixture carries no text: a
/// text-measuring solver would add the typesetter's own per-frame work to a
/// number that is supposed to be about root count.
fn warm_up(loaded: &mut Loaded) -> (TaffySolver<'static>, f32, FrameCost) {
    let mut solver = TaffySolver::new();

    // `Prop::X` of the root's own x marks layout intent without moving
    // anything, so the tree is built from the document's own geometry.
    let x = loaded.arena.layout(loaded.shown).x;
    let first = frame(loaded, &mut solver, Prop::X(x));
    assert_eq!(
        first.solves, 1,
        "the first commit through a fresh solver must solve the shown root. It solved \
         `loaded.roots` of them until story #838 — the tree is still built over every root, \
         because that is what makes a later change of shown root cheap, and only the shown one is \
         computed"
    );
    assert!(
        first.rect_rows > 0,
        "and it must produce a rect table: this is the term that goes to zero if no tree was \
         built, which the solve counter cannot report now that a rebuild and an incremental frame \
         both run one computation (issue #946). Zero here means every count after it is about \
         something other than the steady state"
    );

    (solver, x, first)
}

/// A paint-only frame: a solid fill over the shown root's image fill. No layout
/// intent, so `Arena::layout_dirty` stays empty and the retained tree is not
/// recomputed.
fn paint_only_frame(loaded: &mut Loaded, solver: &mut TaffySolver<'_>) -> FrameCost {
    frame(
        loaded,
        solver,
        Prop::Fill(Color {
            r: 0.1,
            g: 0.2,
            b: 0.3,
            a: 1.0,
        }),
    )
}

/// The two steady-state frames the band is stated over, after the warm-up.
fn steady_state(loaded: &mut Loaded) -> (FrameCost, FrameCost, FrameCost) {
    let (mut solver, x, first) = warm_up(loaded);
    let paint = paint_only_frame(loaded, &mut solver);
    // The shown root moves by one unit. One node of one root is dirty.
    let layout = frame(loaded, &mut solver, Prop::X(x + 1.0));
    (first, paint, layout)
}

/// The two **count** terms, over the many-root document's two steady-state
/// frames — the solve and the committed table. One predicate, so the criterion
/// and the guards that breach it run the same check rather than two checks that
/// have to agree.
///
/// Split from the byte term, which needs a one-root baseline this does not
/// (issue #1119): a guard that breaches only a count no longer has to load a
/// second document to satisfy a signature.
///
/// **Deliberately not called `within_band`.** The band has three terms, and
/// every other artifact — the criterion's panic, the decision record, the
/// guards' own messages — calls the three-term thing "the band". A predicate
/// named for the band that silently checks two of its three terms is how a
/// later caller drops one with no compile error to stop them, which is the
/// failure this file's guards exist to make impossible. [`within_band`] is the
/// one that checks all three.
///
/// `Err` carries what breached and by how much, so a guard's own assertion can
/// name it and a real regression reads as a diagnosis rather than as a number
/// mismatch.
fn within_count_band(paint: FrameCost, layout: FrameCost) -> Result<(), String> {
    let mut breaches = Vec::new();
    if paint.solves != MANY_PAINT_SOLVES {
        breaches.push(format!(
            "a paint-only frame ran {} Taffy layout computations against {MANY_PAINT_SOLVES}: the \
             retained tree's paint-only fast path (issue #164) no longer holds over this document",
            paint.solves
        ));
    }
    if layout.solves != MANY_LAYOUT_SOLVES {
        breaches.push(format!(
            "a layout frame ran {} Taffy layout computations against {MANY_LAYOUT_SOLVES}",
            layout.solves
        ));
    }
    for (label, rows) in [("a paint-only", paint), ("a layout", layout)] {
        if rows.rect_rows != MANY_RECT_ROWS {
            breaches.push(format!(
                "{label} frame committed {} rect rows against {MANY_RECT_ROWS}",
                rows.rect_rows
            ));
        }
    }
    if breaches.is_empty() {
        Ok(())
    } else {
        Err(breaches.join("; "))
    }
}

/// The third term: what a steady-state layout frame allocates that the document
/// makes it allocate.
///
/// Separate from [`within_count_band`] because it is the only term needing a
/// one-root baseline, and a baseline is a whole document load. A guard that
/// breaches a count and asserts on nothing else calls that one and skips this
/// (issue #1119).
///
/// **The many-root frame's own figure is compared, not a saturated
/// difference.** This term was a difference against a non-zero constant until
/// issue #1111 took the constant to 0, and a `saturating_sub` on the way in
/// quietly turned the equality into "the many-root frame allocates no *more*
/// than the one-root frame": a frame that allocated 58 bytes *fewer* read as a
/// growth of zero and passed. That is not what four documents in this
/// repository state, which is byte identity. Comparing `layout.bytes` against
/// `small_layout.bytes + expected` is the same test whenever the constant is
/// positive and a real equality when it is 0, with no subtraction to saturate
/// and no underflow to guard against.
///
/// **The whole growth is compared, not the slope**, so this term is an exact
/// equality like the two counts. Dividing by the root count first would
/// truncate: over 64 extra roots, anything up to 63 bytes of new
/// document-scaled cost divides away and the band stays green — which is the "a
/// change with no term that can see it" failure this term exists to close. The
/// slope is derived from the equality for the message and the log, never
/// asserted on.
fn within_byte_band(
    layout: FrameCost,
    small_layout: FrameCost,
    extra_roots: u64,
) -> Result<(), String> {
    let expected = BYTES_PER_EXTRA_ROOT * extra_roots;
    if layout.bytes == small_layout.bytes + expected {
        return Ok(());
    }
    // Only for the message: the report reads as a growth even when the frame
    // shrank, so it is saturated here rather than in the comparison above.
    let growth = layout.bytes.saturating_sub(small_layout.bytes);
    Err(format!(
        "a layout frame allocated {growth} bytes over the one-root document's {} across \
         {extra_roots} extra roots, against {expected} ({} bytes per extra root against \
         {BYTES_PER_EXTRA_ROOT})",
        small_layout.bytes,
        slope_of(growth, extra_roots),
    ))
}

/// **The band**: all three terms, for the callers that assert on all three.
///
/// Joined the way one predicate used to join them, so a breach in any term
/// still reads as one message and a caller can still name which term moved.
/// This name is the three-term one on purpose — see [`within_count_band`].
fn within_band(
    paint: FrameCost,
    layout: FrameCost,
    small_layout: FrameCost,
    extra_roots: u64,
) -> Result<(), String> {
    let breaches: Vec<String> = [
        within_count_band(paint, layout),
        within_byte_band(layout, small_layout, extra_roots),
    ]
    .into_iter()
    .filter_map(Result::err)
    .collect();
    if breaches.is_empty() {
        Ok(())
    } else {
        Err(breaches.join("; "))
    }
}

/// The byte term as a rate, for a message or a log line. Never the thing
/// asserted on — [`within_byte_band`] compares the whole growth, because a
/// division truncates and a truncated comparison cannot see a small
/// regression.
///
/// Answers 0 rather than dividing when a caller measures a one-root fixture
/// against itself, which is the only way `extra_roots` reaches zero — and
/// `EXTRA_FRAMES`' own documentation says a measurement wanting a different
/// root count "asks for fewer", so that is reachable rather than theoretical.
/// **Every site that prints or reports the rate goes through here**, so none of
/// them can divide by zero and panic before an assertion has run.
fn slope_of(growth: u64, extra_roots: u64) -> u64 {
    growth.checked_div(extra_roots).unwrap_or(0)
}

/// The one-root document's steady-state layout frame — the denominator the byte
/// slope is taken against.
///
/// A whole load and three frames, which is the cheapest honest way to get it:
/// the alternative is a second constant holding the one-root document's own byte
/// count, and that is the level this term is a difference specifically to avoid.
///
/// **One caller**, [`the_confinement_is_what_makes_the_number_one`], which
/// asserts the byte term moves and so genuinely needs a baseline. The criterion
/// measures the small document anyway and passes its own; the paint-only guard
/// asserts on a count and calls [`within_band`], which needs none (issue
/// #1119).
fn small_baseline() -> FrameCost {
    let mut small = load(0);
    let (_, _, layout) = steady_state(&mut small);
    layout
}

/// Prints one document's numbers. The counts are the criterion; the wall clock
/// and the machine are recorded here and asserted on nowhere (D6 of
/// `startup-scaling-is-measured-by-a-counter.md`).
fn report(label: &str, roots: usize, first: FrameCost, paint: FrameCost, layout: FrameCost) {
    println!(
        "PER-FRAME SCALING — {label}, root count {roots}: first frame {} solves / {} rect rows / \
         {} B, paint-only frame {} solves / {} rect rows / {} B, layout frame {} solves / {} rect \
         rows / {} B",
        first.solves,
        first.rect_rows,
        first.bytes,
        paint.solves,
        paint.rect_rows,
        paint.bytes,
        layout.solves,
        layout.rect_rows,
        layout.bytes,
    );
}

/// The criterion. Showing one root costs one solve and one committed-table row
/// per node of **that root's subtree**, on every frame, whatever else the
/// document carries — which is what story #838 made true and what story #836
/// measured at 65:1 before it.
///
/// The equality is the assertion and the ratio is derived from it, which is D4
/// of `startup-scaling-is-measured-by-a-counter.md` applied to the frame
/// instead of the load. It reports 1.00x, the same answer the load criterion
/// beside it reports — which is D2 of
/// `docs/decisions/the-shown-root-bounds-the-load-not-the-paint.md` built, and
/// is the sentence that record's D5 could not write before it.
#[test]
fn a_frame_costs_the_shown_root_and_not_the_document() {
    let started = Instant::now();

    let mut small = load(0);
    let (small_first, small_paint, small_layout) = steady_state(&mut small);
    report(
        "small-root document",
        small.roots,
        small_first,
        small_paint,
        small_layout,
    );

    let mut many = load(EXTRA_FRAMES);
    let (many_first, many_paint, many_layout) = steady_state(&mut many);
    report(
        "many-root document",
        many.roots,
        many_first,
        many_paint,
        many_layout,
    );

    // The denominator is asserted before the ratio is printed, not after. Two
    // reasons, and only the first is about correctness: without it the ratio
    // could be produced by a small document that got *more* expensive rather
    // than by a many-root one that is expensive; and a denominator of zero
    // would print "ratio inf x" into the CI log this step exists to write,
    // which is the record story #838 re-measures against.
    assert_eq!(
        (
            small_paint.solves,
            small_layout.solves,
            small_paint.rect_rows,
            small_layout.rect_rows,
        ),
        (
            SMALL_PAINT_SOLVES,
            SMALL_LAYOUT_SOLVES,
            SMALL_RECT_ROWS,
            SMALL_RECT_ROWS,
        ),
        "showing the only root of a one-root document must cost one solve on a layout frame, none \
         on a paint-only frame, and one committed rect row on both"
    );

    println!(
        "PER-FRAME SCALING — {} bytes per extra root on a layout frame, against the band's \
         {BYTES_PER_EXTRA_ROOT} ({} B over the one-root document's {} B, across {} extra roots)",
        slope_of(
            many_layout.bytes.saturating_sub(small_layout.bytes),
            many.roots as u64 - 1,
        ),
        many_layout.bytes.saturating_sub(small_layout.bytes),
        small_layout.bytes,
        many.roots - 1,
    );

    println!(
        "PER-FRAME SCALING — ratio {:.2}x on the solve and {:.2}x on the committed table \
         (#838's target: 1.00x on both), measured on {} {} in {:.1} ms — which is the whole \
         measurement, and mostly document generation rather than frames. Under `cargo test` \
         that figure varies with scheduling order: the documents are memoised per size \
         (issue #930), so this test pays a build, waits on another test's, or finds one \
         already made. Nothing asserts on it",
        many_layout.solves as f64 / small_layout.solves as f64,
        many_layout.rect_rows as f64 / small_layout.rect_rows as f64,
        std::env::consts::OS,
        std::env::consts::ARCH,
        started.elapsed().as_secs_f64() * 1000.0,
    );

    within_band(
        many_paint,
        many_layout,
        small_layout,
        many.roots as u64 - 1,
    )
    .unwrap_or_else(|breach| {
        panic!(
            "the per-frame band over the {}-root document is breached — {breach}. This band was \
             measured at {MANY_LAYOUT_SOLVES} solves and {MANY_RECT_ROWS} rect rows per frame \
             (story #836) and is what story #838 moves to 1 and 1 by confining the solve, the \
             committed table and the paint to the shown root. If #838 is what moved it, re-measure \
             and move these constants, stating the before and the after. If it is not, a frame has \
             become more expensive than the document's own size, which nothing here predicts — \
             check dashscene_engine::compute_all (one solve per root) and Arena::dfs_order (every \
             root's subtree into one table). If what breached is the byte term, it is a per-frame \
             allocation sized by the document rather than by the shown root: the commit's own \
             scratch is keyed by rect row since story #944, so look first at whatever was added \
             since, and at the three allocations issue #1111 still carries in dashscene-engine. A \
             term that breached *downward* is that issue being closed — re-measure and move \
             {BYTES_PER_EXTRA_ROOT}, stating the before and the after.",
            many.roots
        )
    });
}

/// The solve term is sensitive: a frame that marks layout intent where the
/// paint-only frame marks none moves it from 0 to 1, and the band rejects it.
///
/// **One, not one per root.** Story #838 confined the solve to the shown root,
/// so a layout-dirty frame over the sixty-five-root fixture runs a single Taffy
/// computation — which is what the assertion below requires
/// (`MANY_LAYOUT_SOLVES`).
///
/// This is the measurement a `set_prop` that misclassified a paint property as
/// layout-affecting would produce, taken from the same path. It does not prove
/// the classifier is right — `Arena::layout_dirty`'s own tests do that — and it
/// is not a defect in the code as it stands. What it establishes is that the
/// zero in the band is a measured zero rather than a term nothing can move.
#[test]
fn a_paint_only_frame_that_marks_layout_intent_breaches_the_solve_term() {
    let mut many = load(EXTRA_FRAMES);
    // The same warm-up the criterion runs, and the same anchor assertion inside
    // it — this guard measures the steady state or it measures nothing.
    let (mut solver, x, _) = warm_up(&mut many);

    // Where the criterion stages `Prop::Fill`, this stages layout intent.
    let mutated_paint = frame(&mut many, &mut solver, Prop::X(x));
    let layout = frame(&mut many, &mut solver, Prop::X(x + 1.0));

    println!(
        "PER-FRAME SCALING — guard: a layout-dirty 'paint-only' frame ran {} solves against the \
         band's {MANY_PAINT_SOLVES}",
        mutated_paint.solves
    );

    // The count terms alone. This guard asserts on the solve term and nothing
    // else, and the byte term is the only one needing a one-root baseline — so
    // calling the joined predicate here would load a whole second document to
    // satisfy a signature (issue #1119).
    let breach = within_count_band(mutated_paint, layout)
        .expect_err("a layout-dirty frame in the paint-only slot must breach the band");
    assert!(
        breach.contains("paint-only frame ran"),
        "the guard must breach the solve term, not something else; it reported: {breach}"
    );
    assert_eq!(
        mutated_paint.solves, MANY_LAYOUT_SOLVES,
        "and it must breach it by a whole solve of the shown root"
    );
}

/// **The confinement is what makes the number one**, and removing it puts the
/// band back where story #836 measured it.
///
/// This is the guard the scaling one became. Until story #838 the two terms
/// tracked the document's root count, and a differently-sized document breached
/// the band — which is what said the numbers were measured rather than
/// reported. That test cannot exist now: a seventeen-root document measures
/// exactly what a sixty-five-root one does, which is the story's whole claim.
///
/// So the mutation moved from the fixture to the thing under test. Clearing the
/// shown root is the pre-#838 runtime exactly — every root solved, every root's
/// subtree in the committed table — and the band must reject it. It is
/// committed and re-executed, so the before-number is a measurement this suite
/// takes on every run rather than a figure in a commit message.
#[test]
fn the_confinement_is_what_makes_the_number_one() {
    let small_layout = small_baseline();
    let mut many = load(EXTRA_FRAMES);
    let extra_roots = many.roots as u64 - 1;
    let (_, bounded_paint, bounded_layout) = steady_state(&mut many);
    within_band(bounded_paint, bounded_layout, small_layout, extra_roots)
        .expect("the band holds while the shown root is named");

    // The confinement removed, and nothing else. A fresh solver, because the
    // retained one would report only what moved and this is measuring what a
    // frame costs, not what a transition costs.
    let mut txn = many.arena.open();
    txn.show_root(None);
    txn.commit();
    let mut solver = TaffySolver::new();
    let x = many.arena.layout(many.shown).x;
    let unbounded_first = frame(&mut many, &mut solver, Prop::X(x));
    let unbounded_paint = paint_only_frame(&mut many, &mut solver);
    let unbounded_layout = frame(&mut many, &mut solver, Prop::X(x + 1.0));

    println!(
        "PER-FRAME SCALING — guard: with no shown root the same document measures {} solves, \
         {} rect rows and {} bytes per extra root on a layout frame, against the band's \
         {MANY_LAYOUT_SOLVES}, {MANY_RECT_ROWS} and {BYTES_PER_EXTRA_ROOT} — the before-numbers \
         stories #838 and #944 moved",
        unbounded_layout.solves,
        unbounded_layout.rect_rows,
        slope_of(
            unbounded_layout.bytes.saturating_sub(small_layout.bytes),
            extra_roots,
        ),
    );

    let breach = within_band(unbounded_paint, unbounded_layout, small_layout, extra_roots)
        .expect_err("an unconfined traversal must breach the band");
    assert!(
        breach.contains("layout frame ran")
            && breach.contains("rect rows against")
            && breach.contains("bytes per extra root"),
        "all three terms must move when the confinement goes, not just some of them; it reported: \
         {breach}"
    );
    assert_eq!(
        (
            unbounded_first.solves as usize,
            unbounded_layout.solves as usize,
            unbounded_layout.rect_rows,
        ),
        (many.roots, many.roots, many.roots),
        "unconfined, both terms are the document's root count exactly — which is what story #836 \
         measured at 65 and what this story removed"
    );
}
