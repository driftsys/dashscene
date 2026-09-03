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

use package_gate::PAINTER_PATH as PAINTER;
use package_gate::cs_scan::member_body;
use package_gate::painter_source;

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
    assert!(
        emission.contains("visibleCount = (uint)run,"),
        "{PAINTER}'s emission loop no longer sets `visibleCount` from `run`. \
         A literal `visibleCount` would defeat the one-instance rule \
         silently — the loop could still fix `run` at one instance while the \
         emitted command claimed a different count (issue #1401)."
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
