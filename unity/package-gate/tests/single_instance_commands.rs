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
//! `run`) are named and excluded directly. The per-command material
//! (`materialID = _materialId,`, the class material, since story #1448) and
//! the visible-instance stream
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

    // **The class material, since story #1448.** A glyph takes the ordered
    // path (`BrgPainter.TextPath`), so every instance this loop emits is a
    // node drawn with the painter's own class material, and no text material
    // is registered with the group at all. The material used to be read per
    // instance through `MaterialOf(at)`; the ordered path's runs now name
    // their material by key, through `DrawRouting.OrderedKeyOf`.
    assert!(
        emission.contains("materialID = _materialId,"),
        "{PAINTER}'s emission loop no longer draws every command with the \
         class material. Only nodes reach this loop since story #1448: a \
         glyph is skipped and drawn by the ordered pass."
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

/// `Draw`'s cached command count is the number of instances that take the
/// BatchRendererGroup path, counted inside its own accumulation loop and
/// assigned nowhere else in the member.
///
/// **Not the instance count since story #1448.** A glyph takes the ordered
/// path, so on a lit painter the commands are its node instances only, and an
/// overlay painter registers no batch at all — `_batchCount` stays zero, the
/// loop below adds nothing, and `OnPerformCulling` returns at its guard.
#[test]
fn the_command_count_is_the_brg_path_instance_count() {
    let source = painter_source();
    let (s, e) = member_body(&source, "public void Draw(FrameLease lease)");
    let draw = &source[s..=e];

    let (bs, be) = member_body(draw, "for (var b = 0; b < _batchCount; b++)");
    let accumulation = &draw[bs..=be];
    assert!(
        accumulation.contains("_commandCount += BrgInstancesInBatch(b);"),
        "{PAINTER}'s Draw no longer counts one command per BRG-path instance \
         INSIDE its own accumulation loop over `_batchCount`. The cached count \
         and the emission loop must agree exactly — Unity allocates the \
         command array from the count and the loop writes into it — and a \
         count that included the glyphs the loop skips would leave the last \
         commands unwritten."
    );

    let assignments = assignment_count(draw, "_commandCount");
    assert_eq!(
        assignments, 1,
        "{PAINTER}'s Draw assigns `_commandCount` {assignments} time(s), not \
         once — the reset to zero before the accumulation loop above."
    );

    // **Counted by `DrawRouting.BrgPathInstances`**, which `unity/ffi-check`
    // executes over `DrawRouting.PathOf` — the predicate the emission loop
    // skips by, through `PathOf` below. Two predicates could disagree about
    // one instance, and the count and the loop would part.
    let (cs, ce) = member_body(&source, "private int BrgInstancesInBatch(int b)");
    let counted = squeeze(&source[cs..=ce]);
    assert!(
        counted.contains(
            "return DrawRouting.BrgPathInstances( _packer.Paint, first, first + InstancesInBatch(b), _materialClass);"
        ),
        "{PAINTER}'s BrgInstancesInBatch no longer counts batch `b`'s \
         instances with `DrawRouting.BrgPathInstances` over this painter's \
         class, the count the emission loop's predicate makes."
    );

    // **An overlay painter registers no batch.** `UploadInstances` is the only
    // caller of `EnsureCapacity`, which is the only caller of `AddBatches`, and
    // `Draw` reaches it only on the BRG path.
    let squeezed_draw = squeeze(draw);
    assert!(
        squeezed_draw
            .contains("if (PathFor(_materialClass) == DrawPath.Brg) { UploadInstances(); }"),
        "{PAINTER}'s Draw no longer reaches `UploadInstances` only on the BRG \
         path. An ordered painter that called it would register batches, and \
         every overlay node would draw twice: once in the culling callback and \
         once in the ordered pass."
    );
    assert_eq!(
        draw.matches("UploadInstances()").count(),
        1,
        "{PAINTER}'s Draw calls `UploadInstances` more than once."
    );
    for (callee, caller) in [
        ("EnsureCapacity(", "private void UploadInstances()"),
        ("AddBatches(", "private void EnsureCapacity(int wanted)"),
    ] {
        let (ms, me) = member_body(&source, caller);
        let total = source.matches(callee).count();
        let inside = source[ms..=me].matches(callee).count();
        assert_eq!(
            (total, inside),
            (2, 1),
            "{PAINTER} calls `{callee}` outside `{caller}` (found {total} \
             occurrence(s) including the declaration, {inside} inside the \
             caller). Batches are registered on the BRG path alone."
        );
    }
}

/// The emission loop skips every instance that does not take the
/// BatchRendererGroup path, before it lays out a command.
///
/// **What keeps a lit painter's glyphs from drawing twice** (R4 of the plan's
/// rulings): the ordered pass draws them in runs, so a command here for the
/// same instance would draw it a second time, after every lit node.
#[test]
fn the_culling_callback_emits_only_brg_path_instances() {
    let source = painter_source();
    let (s, e) = member_body(&source, CULLING);
    let culling = &source[s..=e];
    let (ls, le) = member_body(culling, "for (var at = first;");
    let emission = squeeze(&culling[ls..=le]);

    let skip = "if (PathOf(at) != DrawPath.Brg) { at++; continue; }";
    let skip_at = emission.find(skip).unwrap_or_else(|| {
        panic!(
            "{PAINTER}'s emission loop no longer skips the instances that do \
             not take the BRG path with `{skip}`."
        )
    });
    let end_at = emission
        .find("var end = at + 1;")
        .expect("pinned by every_flagged_draw_command_names_one_visible_instance");
    assert!(
        skip_at < end_at,
        "{PAINTER}'s emission loop decides an instance's path after it starts \
         laying out its command."
    );

    // **The routing itself is executed, not scanned.** `DrawRouting.PathOf`
    // routes a glyph by `TextPath` and every other instance by its painter's
    // class, reading the kind tag of the instance's first Paint word and never
    // its atlas (R4); `unity/ffi-check` runs it over both classes. What is
    // left to pin here is that the painter's `PathOf` hands it this frame's
    // Paint stream and this painter's class.
    let (ps, pe) = member_body(&source, "private DrawPath PathOf(int at)");
    assert!(
        squeeze(&source[ps..=pe])
            .contains("return DrawRouting.PathOf(_packer.Paint, at, _materialClass);"),
        "{PAINTER}'s PathOf no longer routes instance `at` by \
         `DrawRouting.PathOf` over the packer's Paint stream and this \
         painter's class."
    );
}

/// The two lit classes take the BatchRendererGroup path, the overlay class and
/// text the ordered one.
///
/// **One arm per class and a throwing default**, so a class added to
/// `MaterialClass` is a failure at its first draw rather than a silent choice
/// of path. The ordered path draws in document order with no sorting key; the
/// lit classes write and test depth, so their order is the depth buffer's.
///
/// **The arms are executed by `unity/ffi-check`**, in
/// `Runtime/DrawRouting.cs`, every class and an unknown one. This pins that
/// the painter's public answers are those, not a second copy that could
/// disagree.
#[test]
fn each_class_takes_the_path_the_spec_names() {
    let source = painter_source();
    let squeezed = squeeze(&source);
    let (s, e) = member_body(&source, "public static DrawPath PathFor(MaterialClass c)");
    assert_eq!(
        squeeze(&source[s + 1..e]),
        "return DrawRouting.PathFor(c);",
        "{PAINTER}'s PathFor no longer answers with `DrawRouting.PathFor`, the \
         arms `unity/ffi-check` executes."
    );
    assert!(
        squeezed.contains("public const DrawPath TextPath = DrawRouting.TextPath;"),
        "{PAINTER}'s `TextPath` is no longer `DrawRouting.TextPath`, the value \
         `unity/ffi-check` holds to the ordered path."
    );
}

/// The ordered pass draws one procedural draw per run, in the order the runs
/// were cut, and the runs are cut in instance order.
///
/// **`SetInteger`, never `SetInt`.** `_DsFirstInstance` is an `int` in the
/// shading, and `SetInteger` is the typed, non-deprecated setter for it.
/// `SetInt` is the deprecated float path, and story #1448's mutation run
/// measured that it does NOT change the picture on Metal with editor
/// 6000.3.23f1 — the order probes stayed 7 of 7 — so this scan, not
/// `just unity-render`, is what pins the choice.
#[test]
fn the_ordered_pass_issues_one_draw_per_material_run_in_run_order() {
    let pass = package_gate::package_cs_files()
        .into_iter()
        .find(|(path, _)| path.ends_with(PASS))
        .unwrap_or_else(|| panic!("the package no longer ships {PASS}"))
        .1;
    let scanned = package_gate::cs_scan::blank_comments_and_strings(&pass);
    let (s, e) = member_body(
        &scanned,
        "public override void RecordRenderGraph(RenderGraph renderGraph, ContextContainer frameData)",
    );
    let body = squeeze(&scanned[s..=e]);

    let iterate = body
        .find("foreach (var run in d.Runs)")
        .unwrap_or_else(|| panic!("{PASS}'s render function no longer iterates `d.Runs` in order"));
    let first = body
        .find("d.Block.SetInteger(FirstInstanceId, run.Offset);")
        .unwrap_or_else(|| {
            panic!(
                "{PASS} no longer sets the run's first instance as an integer \
                 from `run.Offset`"
            )
        });
    let draw = body
        .find("ctx.cmd.DrawProcedural(d.Sheet, run.Material, 0, MeshTopology.Triangles, 6, run.Count, d.Block);")
        .unwrap_or_else(|| {
            panic!(
                "{PASS} no longer draws each run with the seven-argument \
                 `DrawProcedural`: six vertices per instance, `run.Count` \
                 instances, the run's material and the block"
            )
        });
    assert!(
        iterate < first && first < draw,
        "{PASS} sets the first instance after the draw, or outside the loop \
         over the runs."
    );
    assert_eq!(
        body.matches("DrawProcedural(").count(),
        1,
        "{PASS} draws more than once per run."
    );
    for forbidden in ["Sort(", "OrderBy(", "Reverse(", "SetInt("] {
        assert!(
            !body.contains(forbidden),
            "{PASS}'s RecordRenderGraph contains `{forbidden}`. The runs are \
             drawn in the order they were cut, and the first instance is an \
             integer."
        );
    }

    // **The cut is executed, not scanned.** `DrawRouting.CutRuns` walks the
    // instances once, in order, and cuts where the resolved material key
    // changes (R8); `unity/ffi-check` runs it over an overlay and a lit
    // painter, and reversing the runs or misplacing a run's start turns it
    // red. What is left here is the engine half: the painter cuts this
    // frame's instances, and names each run's material in the order the runs
    // were cut.
    let painter = painter_source();
    let (s, e) = member_body(&painter, "private void CutRuns()");
    let cut = squeeze(&painter[s..=e]);
    for forbidden in ["Sort(", "OrderBy(", "Reverse(", "Insert("] {
        assert!(
            !cut.contains(forbidden),
            "{PAINTER}'s CutRuns contains `{forbidden}`: the runs are drawn in \
             the order `DrawRouting.CutRuns` cut them."
        );
    }
    let delegated = "DrawRouting.CutRuns( _packer.Paint, _packer.InstanceAtlas, InstanceCount, \
                     _textMaterials.Length, _materialClass, _keyedRuns);";
    let named = "_runs.Clear(); for (var r = 0; r < _keyedRuns.Count; r++) { var run = \
                 _keyedRuns[r]; var material = run.Key == DrawRouting.ClassMaterial ? _material : \
                 _textMaterials[run.Key]; _runs.Add(new OrderedRun(material, run.Offset, run.Count)); }";
    let at_cut = cut.find(delegated).unwrap_or_else(|| {
        panic!(
            "{PAINTER}'s CutRuns no longer cuts this frame's instances with \
             `{delegated}`."
        )
    });
    let at_named = cut.find(named).unwrap_or_else(|| {
        panic!(
            "{PAINTER}'s CutRuns no longer turns each cut run, in order, into \
             one `OrderedRun` naming its key's material: `{named}`."
        )
    });
    assert!(
        at_cut < at_named,
        "{PAINTER}'s CutRuns names the runs' materials before it cuts them."
    );

    let (s, e) = member_body(&painter, "public void Draw(FrameLease lease)");
    assert_eq!(
        painter[s..=e].matches("CutRuns();").count(),
        1,
        "{PAINTER}'s Draw does not cut the runs exactly once per frame."
    );
}

/// The ordered pass, relative to the package root.
const PASS: &str = "Runtime/Engine/OrderedDrawPass.cs";
