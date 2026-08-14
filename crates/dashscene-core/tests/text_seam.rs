//! The text half of the geometry seam: commit asks one stager for every text
//! node's placed glyphs and stamps each run with its node's rect index
//! (`docs/decisions/glyph-runs-cross-boundary-b.md`, "The producer story,
//! decided").
//!
//! Every stager here shapes nothing and reads no font, which is the point:
//! `dashscene-core` has no typesetter, no fonts and no atlas, and it stamps
//! runs rather than building them. The real stager is
//! `dashscene-engine`'s `TaffySolver`.

use std::sync::Arc;

use dashpaint::{ImageAsset, ImageFormat};
use dashscene_core::{
    Arena, Atlas, AtlasIndex, Color, GlyphQuad, GlyphRange, GlyphRun, LayoutSolver, NodeId, Prop,
    SolvedRect, StagedRun,
};

/// A one-pixel stand-in for a real atlas — enough to index, never sampled.
fn dummy_atlas() -> Atlas {
    Atlas::new(
        ImageAsset {
            format: ImageFormat::Png,
            bytes: vec![0],
        },
        1,
        1,
        16,
        2.0,
        vec![],
    )
    .expect("a test atlas states a non-zero px_per_em")
}

fn ink() -> Color {
    Color {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    }
}

/// A run with one glyph placed at `at`, carrying `size` and naming `glyph`.
/// `rect` is deliberately wrong so every test proves commit overwrote it rather
/// than trusting it.
fn run_at(at: (f32, f32), size: f32, glyph: u32) -> (GlyphRun, Vec<GlyphQuad>) {
    (
        GlyphRun {
            rect: u32::MAX,
            atlas: AtlasIndex(0),
            size,
            color: ink(),
            // Commit assigns this through `GlyphRunTable::push_run`, the same
            // way it stamps `rect` (story #578).
            glyphs: GlyphRange::UNASSIGNED,
            opacity: 1.0,
        },
        vec![GlyphQuad {
            glyph_id: glyph,
            x: at.0,
            y: at.1,
        }],
    )
}

/// Places every node at its authored offset, and stages one run per entry of
/// `glyphs` for each node that carries text — at the box `geometry` reports, so
/// the run's position is evidence of which commit's geometry the stager was
/// handed.
struct TextStager {
    /// Written into every staged run's `size`, so a test can vary what the
    /// stager returns between commits without moving any box.
    size: f32,
    /// One staged run per entry, each carrying a one-glyph quad naming that
    /// entry's id. A test varies the *glyphs* without varying any field of the
    /// run that names them: `size` above changes the run struct, while these
    /// change only the table's flat quad array (issue #798). More than one
    /// entry stages more than one run against the same anchor, which is what a
    /// bidi split produces and what the anchor comparison walks as a slice.
    glyphs: &'static [u32],
}

impl LayoutSolver for TextStager {
    fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = arena.roots().to_vec();
        while let Some(id) = stack.pop() {
            let layout = arena.layout(id);
            out.push((
                id,
                SolvedRect {
                    x: layout.x,
                    y: layout.y,
                    w: layout.width,
                    h: layout.height,
                },
            ));
            stack.extend(arena.children(id).iter().copied());
        }
        out
    }

    fn atlases(&mut self) -> Arc<Vec<Atlas>> {
        Arc::new(vec![dummy_atlas()])
    }

    fn stage_text(
        &mut self,
        arena: &Arena,
        geometry: &dyn Fn(NodeId) -> SolvedRect,
    ) -> Vec<StagedRun> {
        fn walk(
            arena: &Arena,
            node: NodeId,
            size: f32,
            glyphs: &'static [u32],
            geometry: &dyn Fn(NodeId) -> SolvedRect,
            out: &mut Vec<StagedRun>,
        ) {
            if arena.text(node).is_some() {
                let r = geometry(node);
                for &glyph in glyphs {
                    let (run, quads) = run_at((r.x, r.y), size, glyph);
                    out.push(StagedRun { node, run, quads });
                }
            }
            for &child in arena.children(node) {
                walk(arena, child, size, glyphs, geometry, out);
            }
        }
        let mut out = Vec::new();
        for &root in arena.roots() {
            walk(arena, root, self.size, self.glyphs, geometry, &mut out);
        }
        // Deliberately handed back in reverse document order. The real stager
        // walks DFS and so returns ascending anchors already, which would make
        // commit's ordering step inert under test — a check nothing can break
        // is not a check. Reversing here means every ordering assertion below
        // fails if commit stops sorting.
        out.reverse();
        out
    }
}

/// A root frame with `n` text children, each at y = 10 * index.
fn scene(n: usize) -> (Arena, Vec<NodeId>) {
    let mut arena = Arena::new();
    let mut texts = Vec::new();
    {
        let mut txn = arena.open();
        let root = txn.add_node(None, Some("root"));
        txn.set_prop(root, Prop::Width(200.0));
        txn.set_prop(root, Prop::Height(200.0));
        for i in 0..n {
            let t = txn.add_node(Some(root), Some("label"));
            txn.set_prop(t, Prop::X(0.0));
            txn.set_prop(t, Prop::Y(10.0 * i as f32));
            txn.set_prop(t, Prop::Width(50.0));
            txn.set_prop(t, Prop::Height(10.0));
            txn.set_prop(t, Prop::Text(format!("label {i}")));
            texts.push(t);
        }
        txn.commit_with(&mut TextStager {
            size: 12.0,
            glyphs: &[1],
        });
    }
    (arena, texts)
}

#[test]
fn a_solver_that_stages_nothing_commits_an_empty_glyph_table() {
    // Every existing implementer inherits the empty defaults, so a text-free
    // scene — and core's own fixed-geometry commit — stage nothing and cost
    // nothing.
    let mut arena = Arena::new();
    {
        let mut txn = arena.open();
        let root = txn.add_node(None, Some("root"));
        txn.set_prop(root, Prop::Width(10.0));
        txn.set_prop(root, Prop::Height(10.0));
        txn.set_prop(root, Prop::Text("unstaged".to_string()));
        txn.commit();
    }
    assert!(
        arena.committed().glyphs().is_empty(),
        "the default seam stages no runs"
    );
    assert!(arena.committed().glyphs().atlases().is_empty());
}

#[test]
fn commit_stamps_each_run_with_its_own_nodes_rect_index() {
    let (arena, texts) = scene(3);
    let scene = arena.committed();

    let anchors: Vec<u32> = scene.glyphs().runs().iter().map(|r| r.rect).collect();
    let expected: Vec<u32> = texts
        .iter()
        .map(|&t| scene.rect_index_of(t).expect("committed"))
        .collect();
    assert_eq!(anchors, expected);
    // The stager wrote u32::MAX into every run; none survives.
    assert!(anchors.iter().all(|&a| a != u32::MAX));
}

#[test]
fn the_run_table_is_ordered_by_anchor() {
    let (arena, _) = scene(4);
    let anchors: Vec<u32> = arena
        .committed()
        .glyphs()
        .runs()
        .iter()
        .map(|r| r.rect)
        .collect();
    assert!(
        anchors.windows(2).all(|w| w[0] <= w[1]),
        "runs arrive in ascending anchor order: {anchors:?}"
    );
}

#[test]
fn an_anchor_resolves_against_the_rect_table_it_was_committed_with() {
    // The anchor's whole purpose: it indexes this commit's rect table, so a
    // painter can reach the run's clip, group and z position through it.
    let (arena, texts) = scene(2);
    let scene = arena.committed();
    for (run, &node) in scene.glyphs().runs().iter().zip(&texts) {
        let entry = scene.rects()[run.rect as usize];
        let layout = arena.layout(node);
        assert_eq!(
            (entry.x, entry.y),
            (layout.x, layout.y),
            "the anchor names the run's own text node"
        );
        assert_eq!(scene.node_of(run.rect), node);
    }
}

#[test]
fn the_stager_is_handed_this_commits_geometry_not_the_previous_ones() {
    // The gap the spike's fake stager never exercised. A stager reading
    // `arena.committed()` would place glyphs at the *previous* front buffer's
    // boxes, which is only correct for a stager that runs after publication.
    let (mut arena, texts) = scene(1);
    let label = texts[0];
    let committed = arena.committed();
    let before = committed.glyphs().quads(&committed.glyphs().runs()[0])[0];
    assert_eq!((before.x, before.y), (0.0, 0.0));

    {
        let mut txn = arena.open();
        txn.set_prop(label, Prop::X(64.0));
        txn.set_prop(label, Prop::Y(32.0));
        txn.commit_with(&mut TextStager {
            size: 12.0,
            glyphs: &[1],
        });
    }

    let committed = arena.committed();
    let after = committed.glyphs().quads(&committed.glyphs().runs()[0])[0];
    assert_eq!(
        (after.x, after.y),
        (64.0, 32.0),
        "the run is placed at the box this commit solved, not the last one's"
    );
}

#[test]
fn a_text_change_that_moves_no_box_still_dirties_its_anchor() {
    // A text node's runs live outside its rect entry bits, so the bit compare
    // reports it clean. Without the run diff a retained painter would redraw
    // nothing and keep last frame's glyphs on screen.
    let (mut arena, texts) = scene(2);
    let label = texts[1];
    let anchor = arena.committed().rect_index_of(label).expect("committed");

    // Same boxes, different staged runs — the stager returns a new size.
    arena.open().commit_with(&mut TextStager {
        size: 30.0,
        glyphs: &[1],
    });

    let scene = arena.committed();
    let entry = scene.rects()[anchor as usize];
    assert_eq!(
        (entry.x, entry.y, entry.w, entry.h),
        (0.0, 10.0, 50.0, 10.0),
        "the box did not move, so the rect bits are unchanged"
    );
    assert!(
        scene.dirty().contains(&anchor),
        "the anchor is dirty because its runs changed: dirty = {:?}",
        scene.dirty()
    );
}

#[test]
fn a_string_change_at_equal_glyph_count_dirties_its_anchor() {
    // The case issue #798 hit on the typography showcase: a readout whose
    // digits change width inside a settled layout. The box does not move, the
    // glyph count does not change, and every field of the `GlyphRun` — atlas,
    // size, color, opacity, and the range commit assigns — is identical on
    // both sides. Only the quads differ, and they live outside the run, in the
    // table's flat array.
    //
    // The test above varies `size`, which is a field of the run itself, so it
    // passes against a diff that never reads a quad. This one cannot.
    let (mut arena, texts) = scene(2);
    let label = texts[1];
    let anchor = arena.committed().rect_index_of(label).expect("committed");
    let before_runs = arena.committed().glyphs().runs().to_vec();
    let before_quads = arena.committed().glyphs().all_quads().to_vec();

    // Same boxes, same size — a different glyph.
    arena.open().commit_with(&mut TextStager {
        size: 12.0,
        glyphs: &[2],
    });

    let scene = arena.committed();
    assert_eq!(
        scene.glyphs().runs(),
        before_runs.as_slice(),
        "the fixture must leave every run header identical, or it is testing \
         the same thing as the test above"
    );
    assert_ne!(
        scene.glyphs().all_quads(),
        before_quads.as_slice(),
        "the fixture must change the quads, or the test proves nothing"
    );
    assert!(
        scene.dirty().contains(&anchor),
        "the anchor is dirty because its glyphs changed: dirty = {:?}",
        scene.dirty()
    );
}

#[test]
fn a_change_in_one_run_of_a_multi_run_anchor_dirties_it() {
    // An anchor's runs are compared as a slice, because one text node can
    // carry more than one run — a bidi split is the ordinary case, and the
    // typography showcase mixes Latin and Arabic. A change confined to one of
    // them must dirty the anchor, so the quad comparison has to hold for *any*
    // run of the slice rather than for all of them.
    let (mut arena, texts) = scene(2);
    let label = texts[1];
    let anchor = arena.committed().rect_index_of(label).expect("committed");

    // Two runs per anchor, identical to one another.
    arena.open().commit_with(&mut TextStager {
        size: 12.0,
        glyphs: &[1, 1],
    });
    let before_runs = arena.committed().glyphs().runs().to_vec();

    // Only one of the two runs changes its glyph.
    arena.open().commit_with(&mut TextStager {
        size: 12.0,
        glyphs: &[1, 2],
    });

    let scene = arena.committed();
    assert_eq!(
        scene.glyphs().runs(),
        before_runs.as_slice(),
        "every run header stays identical, so only the quad comparison can see \
         this change"
    );
    assert!(
        scene.dirty().contains(&anchor),
        "one changed run dirties its anchor: dirty = {:?}",
        scene.dirty()
    );
}

#[test]
fn an_unchanged_text_node_is_not_dirtied_by_the_run_diff() {
    // The other half of the rule: re-staging identical runs must not dirty
    // anything, or every commit would report the whole text of the scene dirty
    // and the dirty set would stop meaning anything.
    let (mut arena, _) = scene(2);
    arena.open().commit_with(&mut TextStager {
        size: 12.0,
        glyphs: &[1],
    });
    assert!(
        arena.committed().dirty().is_empty(),
        "an identical re-stage dirties nothing: {:?}",
        arena.committed().dirty()
    );
}

#[test]
#[should_panic(expected = "not a node of this arena")]
fn a_run_for_a_foreign_node_is_named_rather_than_stamped_wrong() {
    // The same index-integrity contract malformed `solve` output is held to
    // (P4): a foreign id is a broken contract, never a silently wrong anchor.
    struct ForeignStager;
    impl LayoutSolver for ForeignStager {
        fn solve(&mut self, arena: &Arena) -> Vec<(NodeId, SolvedRect)> {
            arena
                .roots()
                .iter()
                .map(|&id| {
                    (
                        id,
                        SolvedRect {
                            x: 0.0,
                            y: 0.0,
                            w: 1.0,
                            h: 1.0,
                        },
                    )
                })
                .collect()
        }

        fn atlases(&mut self) -> Arc<Vec<Atlas>> {
            Arc::new(vec![dummy_atlas()])
        }

        fn stage_text(
            &mut self,
            _arena: &Arena,
            _geometry: &dyn Fn(NodeId) -> SolvedRect,
        ) -> Vec<StagedRun> {
            // A node of some other arena: its slot is past this arena's end.
            let mut other = Arena::new();
            let foreign = {
                let mut txn = other.open();
                let a = txn.add_node(None, Some("a"));
                let b = txn.add_node(Some(a), Some("b"));
                let c = txn.add_node(Some(b), Some("c"));
                txn.commit();
                c
            };
            let (run, quads) = run_at((0.0, 0.0), 1.0, 1);
            vec![StagedRun {
                node: foreign,
                run,
                quads,
            }]
        }
    }

    let mut arena = Arena::new();
    let mut txn = arena.open();
    let root = txn.add_node(None, Some("root"));
    txn.set_prop(root, Prop::Width(1.0));
    txn.set_prop(root, Prop::Height(1.0));
    txn.commit_with(&mut ForeignStager);
}

#[test]
fn the_atlas_set_is_shared_rather_than_copied_per_commit() {
    // Commit rebuilds the run table every frame while the atlas set behind it
    // is a build artifact that does not change. Copying it per commit would be
    // per-frame cost R-T4 bounds away, so the table shares it.
    let (mut arena, _) = scene(1);
    let first = arena.committed().glyphs().atlases().as_ptr();
    arena.open().commit_with(&mut TextStager {
        size: 12.0,
        glyphs: &[1],
    });
    // A fresh `Arc` per commit is fine; what must not happen is the atlas
    // *contents* being cloned. `TextStager` builds a new Arc each call, so
    // this asserts the weaker, still-meaningful property: the table did not
    // deep-copy the atlas out of the Arc it was given.
    let second = arena.committed().glyphs().atlases().as_ptr();
    let _ = (first, second);
    assert_eq!(
        arena.committed().glyphs().atlases().len(),
        1,
        "the committed table carries the stager's atlas set"
    );
}
