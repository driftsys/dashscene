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
//! Two terms, both exact and identical on every machine:
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
//!
//! Neither is a stopwatch, and that is deliberate.
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
//! document           frame         solves   rect rows
//! small-root (1)     paint-only         0           1
//! small-root (1)     layout             1           1
//! many-root (65)     paint-only         0           1
//! many-root (65)     layout             1           1
//! ratio, many/small  layout          1.00x       1.00x
//! ```
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
//! The band landed first so that before-number was measured rather than
//! remembered — a band added in the same change that improves what it measures
//! cannot fail, and cannot show what the change was worth. And it is still
//! measured: [`the_confinement_is_what_makes_the_number_one`] removes the
//! confinement on every run and reports 65 again.
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
//! [`within_band`] is the assertion, written once and called three times: by
//! the criterion, and by the two guards that are committed to breach it. A
//! band nothing has been shown to break is not yet a band
//! (`docs/technotes/measured-verification.md`, "the sensitivity guard").
//!
//! - [`a_paint_only_frame_that_marks_layout_intent_breaches_the_solve_term`]
//!   drives the paint-only frame with a layout property instead. The solve term
//!   goes from 0 to 65 and the band rejects it. That is the measurement a
//!   misclassifying `set_prop` would produce, from the same path — it does not
//!   prove the classifier is correct, which is `Arena::layout_dirty`'s own
//!   tests' job, but it does prove this band would see it.
//! - [`the_confinement_is_what_makes_the_number_one`] clears the shown root, so
//!   the runtime traverses every root exactly as it did before story #838. Both
//!   terms go to 65 and the band rejects them. It replaced a guard that ran the
//!   same frames over a seventeen-root document: that one worked while the
//!   terms tracked the document's size, and cannot work now that they do not —
//!   which is the story's whole claim, restated as a test that had to be
//!   retired.
//!
//! **The upward injection story #836 said was unavailable is available now**,
//! and it is the second guard. At 65 roots both terms were saturated at the
//! document's own size, so nothing a test could stage made either larger; with
//! the traversal confined, removing the confinement is exactly such a mutation
//! and it breaches both terms at once.
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

use std::sync::Arc;
use std::time::Instant;

use dashbuf::map::MappedFile;
use dashpaint::Color;
use dashscene_core::{Arena, MappedPayload, NodeId, Prop, Region, ShownRoot, load_document_mapped};
use dashscene_engine::TaffySolver;

mod common;

use common::many_root::{EXTRA_FRAMES, document};

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
    let mut txn = arena.open();
    txn.show_root(Some(ShownRoot::FIRST));
    txn.commit();

    let roots = arena.roots().len();
    assert_eq!(
        roots,
        extra + 1,
        "the fixture builds one root per frame, so a document with {extra} extra frames has \
         {} roots; a loader that nested them would make every count below a different quantity",
        extra + 1
    );
    let shown = arena.roots()[0];

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
    txn.commit_with(solver);
    FrameCost {
        solves: solver.solves() - before,
        rect_rows: loaded.arena.committed().rects().len(),
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
/// **Every measurement in this file starts here, including the guards.** The
/// assertion below is the anchor: if a fresh solver ever stopped rebuilding,
/// every count taken after it would be about something other than the steady
/// state, and a guard that had its own copy of this prologue could keep passing
/// while the criterion failed.
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
        "the first commit through a fresh solver must build the retained tree and solve the shown \
         root. It solved `loaded.roots` of them until story #838 — the tree is still built over \
         every root, because that is what makes a later change of shown root cheap, and only the \
         shown one is computed. A zero here means no tree was built and every count after it is \
         about something other than the steady state"
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

/// The band. One predicate over the many-root document's two steady-state
/// frames, so the criterion and the guards that breach it run the same check
/// rather than two checks that have to agree.
///
/// `Err` carries what breached and by how much, so a guard's own assertion can
/// name it and a real regression reads as a diagnosis rather than as a number
/// mismatch.
fn within_band(paint: FrameCost, layout: FrameCost) -> Result<(), String> {
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

/// Prints one document's numbers. The counts are the criterion; the wall clock
/// and the machine are recorded here and asserted on nowhere (D6 of
/// `startup-scaling-is-measured-by-a-counter.md`).
fn report(label: &str, roots: usize, first: FrameCost, paint: FrameCost, layout: FrameCost) {
    println!(
        "PER-FRAME SCALING — {label}, root count {roots}: first frame {} solves / {} rect rows, \
         paint-only frame {} solves / {} rect rows, layout frame {} solves / {} rect rows",
        first.solves,
        first.rect_rows,
        paint.solves,
        paint.rect_rows,
        layout.solves,
        layout.rect_rows,
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
        "PER-FRAME SCALING — ratio {:.2}x on the solve and {:.2}x on the committed table \
         (#838's target: 1.00x on both), measured on {} {} in {:.1} ms — which is the whole \
         measurement, almost all of it generating the two documents rather than running frames",
        many_layout.solves as f64 / small_layout.solves as f64,
        many_layout.rect_rows as f64 / small_layout.rect_rows as f64,
        std::env::consts::OS,
        std::env::consts::ARCH,
        started.elapsed().as_secs_f64() * 1000.0,
    );

    within_band(many_paint, many_layout).unwrap_or_else(|breach| {
        panic!(
            "the per-frame band over the {}-root document is breached — {breach}. This band was \
             measured at {MANY_LAYOUT_SOLVES} solves and {MANY_RECT_ROWS} rect rows per frame \
             (story #836) and is what story #838 moves to 1 and 1 by confining the solve, the \
             committed table and the paint to the shown root. If #838 is what moved it, re-measure \
             and move these constants, stating the before and the after. If it is not, a frame has \
             become more expensive than the document's own size, which nothing here predicts — \
             check dashscene_engine::compute_all (one solve per root) and Arena::dfs_order (every \
             root's subtree into one table).",
            many.roots
        )
    });
}

/// The solve term is sensitive: a frame that marks layout intent where the
/// paint-only frame marks none moves it from 0 to one per root, and the band
/// rejects it.
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

    let breach = within_band(mutated_paint, layout)
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
    let mut many = load(EXTRA_FRAMES);
    let (_, bounded_paint, bounded_layout) = steady_state(&mut many);
    within_band(bounded_paint, bounded_layout)
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
        "PER-FRAME SCALING — guard: with no shown root the same document measures {} solves and \
         {} rect rows on a layout frame, against the band's {MANY_LAYOUT_SOLVES} and \
         {MANY_RECT_ROWS} — the before-number story #838 moved",
        unbounded_layout.solves, unbounded_layout.rect_rows
    );

    let breach = within_band(unbounded_paint, unbounded_layout)
        .expect_err("an unconfined traversal must breach the band");
    assert!(
        breach.contains("layout frame ran") && breach.contains("rect rows against"),
        "both terms must move when the confinement goes, not just one; it reported: {breach}"
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
