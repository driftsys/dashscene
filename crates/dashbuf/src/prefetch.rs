//! Which assets one root's subtree draws — the set a host makes resident
//! before it paints, and nothing else.
//!
//! `docs/decisions/verification-moves-from-open-to-touch.md` D4: "the load path
//! prefetches the shown root's assets, and nothing else. Cold start is bounded
//! by making resident exactly what the shown root needs: the asset indices
//! reachable from the root's subtree through its nodes' paint entries."
//!
//! That is R5's claim stated as a computation. A sixty-five-frame document and
//! a one-frame document showing the same root reach the same assets, so a host
//! that touches this set and no more reads the same number of bytes out of
//! either — which is what the startup-scaling criterion asserts
//! (`docs/decisions/startup-scaling-is-measured-by-a-counter.md`).
//!
//! **Nothing here reads a payload.** The whole computation is over the document
//! and its pools, which are the hot half of the file and already read by the
//! time [`crate::open`] returns. No payload is touched to decide which payloads
//! to touch.

use crate::{Document, Fill, NO_FIELD, NO_PAINT, NO_PARENT};

/// The document's first root node, or [`None`] for a document with no nodes.
///
/// "The shown root" for both hosts today. A `.dsb` compiled from a Figma file
/// with many artboards carries one root node per artboard, in the order they
/// were lowered, and the first is the one a host with no other instruction
/// shows — which is what story #598's fixture already says structurally, by
/// building its two documents so that frame 0 is the same subtree in each.
///
/// **For any document that passed the load gate this is node 0**, and the search
/// below cannot answer otherwise: `dashscene_validator` refuses a node whose
/// parent does not precede it, and no parent index precedes zero. Replacing the
/// predicate with `index == 0` therefore survives every test in this
/// repository, which is a fact about the format rather than a gap in coverage.
///
/// It is a function anyway for two reasons: a host should name the thing it
/// wants rather than the index that thing currently has, since a shown-root
/// selector is the next thing to arrive here; and a document with no nodes
/// answers [`None`] rather than zero, which is what stops a host prefetching
/// against a node that is not there.
pub fn first_root(document: &Document<'_>) -> Option<u32> {
    let nodes = document.nodes().unwrap_or_default();
    (0..nodes.len())
        .find(|&index| nodes.get(index).parent() == NO_PARENT)
        .map(|index| index as u32)
}

/// The asset entries `root`'s subtree draws, ascending and without repeats.
///
/// Each value indexes `Document.assets`, which is the same order
/// [`crate::open`] returns its [`crate::Wanted`]s in, so a host touches
/// `wanted[index]` for each index here.
///
/// Two ways a paint entry reaches an asset, and both are followed:
///
/// - an **image fill**, through `Paint.fill` and through every layer of
///   `Paint.extra_fills` — a stacked fill is as much a fill as the bottom one;
/// - a **baked vector shape**, through `Paint.shape_field` to
///   `Document.vector_shapes`, that shape's atlas, and that atlas's image. The
///   MSDF sheet is an ordinary asset entry and a node drawing a vector shape
///   needs it exactly as a node drawing a picture needs its picture.
///
/// Strokes, shadows and blurs reach no asset: a v0.3 stroke is solid-only by
/// its own schema comment, and an effect carries no image.
///
/// # This assumes a document that passed the load gate
///
/// Subtree membership is computed in one forward pass, which is sound because
/// the node array is in DFS order and `dashscene_validator::validate_document`
/// refuses a node whose parent does not precede it. An index that misses its
/// pool is skipped rather than panicked on: the gate is what reports a broken
/// reference by name (P4), and a prefetch that guessed would report it twice.
pub fn assets_of_root(document: &Document<'_>, root: u32) -> Vec<u32> {
    let nodes = document.nodes().unwrap_or_default();
    let paints = document.paints().unwrap_or_default();
    let shapes = document.vector_shapes().unwrap_or_default();
    let atlases = document.vector_atlases().unwrap_or_default();
    let assets = document.assets().unwrap_or_default();

    let mut inside = vec![false; nodes.len()];
    let mut wanted: Vec<u32> = Vec::new();

    for index in 0..nodes.len() {
        let node = nodes.get(index);
        let parent = node.parent();
        inside[index] = index as u32 == root
            || (parent != NO_PARENT && (parent as usize) < index && inside[parent as usize]);
        if !inside[index] {
            continue;
        }

        let entry = node.paint_entry();
        if entry == NO_PAINT || entry as usize >= paints.len() {
            continue;
        }
        let paint = paints.get(entry as usize);

        if paint.fill_type() == Fill::ImageFill
            && let Some(fill) = paint.fill_as_image_fill()
        {
            wanted.push(fill.image());
        }
        for layer in paint.extra_fills().unwrap_or_default() {
            if layer.fill_type() == Fill::ImageFill
                && let Some(fill) = layer.fill_as_image_fill()
            {
                wanted.push(fill.image());
            }
        }

        let field = paint.shape_field();
        if field != NO_FIELD && (field as usize) < shapes.len() {
            let atlas = shapes.get(field as usize).atlas();
            if (atlas as usize) < atlases.len() {
                wanted.push(atlases.get(atlas as usize).image());
            }
        }
    }

    wanted.retain(|index| (*index as usize) < assets.len());
    wanted.sort_unstable();
    wanted.dedup();
    wanted
}
