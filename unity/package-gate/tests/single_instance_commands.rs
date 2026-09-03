//! Every draw command that carries `HasSortingPosition` names exactly one
//! visible instance.
//!
//! **Why.** Issue #1401: Unity's sorted-transparent path was measured
//! dropping a contiguous subset of draw commands for single frames when a
//! flagged command carried more than one visible instance — ~300
//! dropped-band frames per 20,000 as built, 0 per 60,000 with one instance
//! per command, on macOS/Metal, 6000.3.23f1. Unity documents no
//! restriction; `docs/technotes/batch-renderer-group.md` §3 attributes
//! `visibleCount = 1` commands to Unity's own GPU Resident Drawer, and that
//! shape is the only one measured safe. §5d carries the tables.
//!
//! **What is pinned, and why each piece is bound rather than searched.** A
//! review found the first version of this file defeatable by three
//! mutations that each restored a material-run walk while leaving every
//! `contains` check green: a `while` loop spliced between `var end = at +
//! 1;` and `var run = end - at;`, `run` rewritten through `Math.Min`, and a
//! one-iteration `if` inserted between the two lines. The two lines are now
//! pinned CONTIGUOUSLY — squeezed to one line and asserted as one unbroken
//! sequence — which defeats every one of those three. `assignment_count`
//! bounds `end` and `run` to one assignment each, and the increment and
//! compound-operator spellings `assignment_count` does not itself see as an
//! assignment (`end++`, `end +=`, `end--`, `end -=`, and the same four for
//! `run`) are named and excluded directly. The per-command material read
//! (`materialID = MaterialOf(at)`) and the visible-instance stream
//! (`visibleOffset`, `visible += run;`, bounded to zero bare assignments to
//! `visible`) are pinned the same way.
//!
//! Text is weak: this pins that the painter still emits the safe shape,
//! never that Unity honours it — the same trade every scan beside this one
//! makes, stated in `sorting_keys.rs`.

use package_gate::PAINTER_PATH as PAINTER;
use package_gate::cs_scan::{assignment_count, member_body, squeeze};
use package_gate::painter_source;

const CULLING: &str = "private unsafe JobHandle OnPerformCulling(";

/// The emission loop still fixes every run at one instance, and does so with
/// nothing splicing the two lines that state it apart.
#[test]
fn every_flagged_draw_command_names_one_visible_instance() {
    let source = painter_source();
    let (s, e) = member_body(&source, CULLING);
    let culling = &source[s..=e];
    let (ls, le) = member_body(culling, "for (var at = first;");
    let emission = &culling[ls..=le];
    let squeezed = squeeze(emission);

    // **Pinned CONTIGUOUSLY, through the same squeeze that normalises the
    // needle.** A review defeated the unbounded, two-assertion version of
    // this check with three mutations, each leaving both lines present while
    // putting a material-run walk between them: a `while` advancing `end`
    // while a later instance shares `at`'s material, `run` rewritten through
    // `Math.Min` instead of read straight off `end - at`, and a
    // one-iteration `if` spliced in. All three break this one contiguous
    // sequence even though each leaves the two lines individually findable.
    let needle = squeeze("var end = at + 1; var run = end - at;");
    assert!(
        squeezed.contains(&needle),
        "{PAINTER}'s emission loop no longer fixes every run's end at one \
         instance with nothing between the two lines that state it. \
         `{needle}` must appear as one unbroken sequence (whitespace \
         normalised) — anything spliced between `var end = at + 1;` and \
         `var run = end - at;` is a walk past the single instance issue \
         #1401 removed."
    );

    // **The increment and compound-operator spellings, named directly.**
    // `assignment_count` matches a bare `=` and refuses `==`, but a compound
    // operator or `++`/`--` is neither — `end += 1` and `end++` are not an
    // assignment by that function's own rule, so a mutation using either
    // form would pass an `assignment_count(emission, "end") == 1` bound with
    // no other change. Checked here as literal text instead.
    for forbidden in [
        "end++", "end +=", "end--", "end -=", "run++", "run +=", "run--", "run -=",
    ] {
        assert!(
            !emission.contains(forbidden),
            "{PAINTER}'s emission loop contains `{forbidden}`. Neither `end` \
             nor `run` is ever advanced after its one assignment from `at` — \
             either spelling reintroduces a walk that `assignment_count` \
             alone cannot see."
        );
    }

    for name in ["end", "run"] {
        let seen = assignment_count(emission, name);
        assert_eq!(
            seen, 1,
            "{PAINTER}'s emission loop assigns `{name}` {seen} time(s), not \
             once. A second assignment ties `{name}` to something other than \
             the one-instance arithmetic above, even where the contiguous \
             sequence and the literal negatives above stay clean."
        );
    }

    assert!(
        emission.contains("materialID = MaterialOf(at),"),
        "{PAINTER}'s emission loop no longer sets `materialID` from `at`, \
         the run's own first and only instance. A material read from \
         anywhere else — `end`, or an index computed from `run` — names the \
         wrong sheet for a command that carries just one instance."
    );

    assert!(
        emission.contains("visibleOffset = (uint)visible,"),
        "{PAINTER}'s emission loop no longer sets `visibleOffset` from \
         `visible`."
    );
    assert!(
        emission.contains("visible += run;"),
        "{PAINTER}'s emission loop no longer advances `visible` by `run`."
    );
    let visible_assignments = assignment_count(emission, "visible");
    assert_eq!(
        visible_assignments, 0,
        "{PAINTER}'s emission loop assigns `visible` directly {visible_assignments} \
         time(s) (as opposed to through `+=`). `visible` is only ever \
         accumulated — a bare assignment anywhere in the loop resets the \
         offset the next command reads, silently, with both fragments above \
         still present."
    );

    assert!(
        emission.contains("visibleCount = (uint)run,"),
        "{PAINTER}'s emission loop no longer sets `visibleCount` from `run`. \
         A literal `visibleCount` would defeat the one-instance rule \
         silently — the loop could still fix `run` at one instance while the \
         emitted command claimed a different count (issue #1401)."
    );
}

/// A single, literal spelling of the material-run comparison — one belt
/// beside the arithmetic pins above, which are the real guard.
#[test]
fn no_material_run_walk_survives_anywhere_in_the_painter() {
    let source = painter_source();
    assert!(
        !source.contains("InstanceAtlas[end]"),
        "{PAINTER} contains the literal text `InstanceAtlas[end]` — one exact \
         spelling of the material-run comparison this file exists to rule \
         out. This check pins nothing beyond that spelling: the arithmetic \
         assertions in `every_flagged_draw_command_names_one_visible_instance` \
         (the contiguous `var end = at + 1; var run = end - at;` and the \
         bound assignment counts around it) are what actually guard against \
         the walk returning under different code."
    );
}

/// `Draw`'s cached command count is the instance count, counted inside its
/// own accumulation loop and assigned nowhere else in the member.
#[test]
fn the_command_count_is_the_instance_count() {
    let source = painter_source();
    let (s, e) = member_body(&source, "public void Draw(FrameLease lease)");
    let draw = &source[s..=e];

    let (bs, be) = member_body(draw, "for (var b = 0; b < _batchCount; b++)");
    let accumulation = &draw[bs..=be];
    assert!(
        accumulation.contains("_commandCount += InstancesInBatch(b);"),
        "{PAINTER}'s Draw no longer counts one command per instance INSIDE \
         its own accumulation loop over `_batchCount`. The cached count and \
         the emission loop must agree exactly — Unity allocates the command \
         array from the count and the loop writes into it."
    );

    let assignments = assignment_count(draw, "_commandCount");
    assert_eq!(
        assignments, 1,
        "{PAINTER}'s Draw assigns `_commandCount` {assignments} time(s), not \
         once — the reset to zero before the accumulation loop above. (The \
         loop's own `+=` is not counted as an assignment by this function; \
         it is pinned separately, inside the loop, above.) A second bare \
         assignment — a rewrite after the loop, say — could leave the cached \
         count disagreeing with what the emission loop actually walks, with \
         both fragments above still present."
    );
}
