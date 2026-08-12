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
//!
//! # Which root, since story #837
//!
//! [`ShownRoot`] is how a host says which one. Until that story there was no
//! saying: both integration crates called a `first_root` that took no argument,
//! so "the shown root" meant "root 0" everywhere it appeared — a bound on the
//! load and a synonym for the first root, not a choice anyone made. A host now
//! names a [`ShownRoot`] and [`resolve`] turns it into the node index the rest
//! of this module takes.
//!
//! It still bounds the **load** and nothing below it. The solve, the committed
//! table and the paint still cover every root; story #838 is what changes that,
//! and `docs/decisions/the-shown-root-bounds-the-load-not-the-paint.md` D3 is
//! why a selection concept had to exist before it could.

use crate::{Document, Fill, NO_FIELD, NO_PAINT, NO_PARENT};

/// Which root a host is showing — an ordinal over the document's roots, in the
/// order the document declares them.
///
/// A `.dsb` compiled from a Figma file with many artboards carries one root node
/// per artboard, in the order they were lowered. [`ShownRoot::FIRST`] is the
/// artboard a host with no other instruction shows, and was the only answer
/// available until story #837.
///
/// # Why an ordinal, and not a node index or a name
///
/// Settled in `docs/decisions/the-shown-root-is-named-by-ordinal.md`; the short
/// form is that the other two candidates cannot address every root of every
/// document.
///
/// - **A node index** is what [`resolve`] returns, not what a host says. Asking
///   an embedder for one means asking it to know which entries of `Document.nodes`
///   happen to be roots — a fact about the node table's packing rather than
///   about the picture. It is also the value this type exists to be
///   distinguishable from: both are `u32`, and passing one where the other
///   belongs would read the wrong subtree rather than fail.
/// - **A document-declared name** is optional in the schema — `Node.name` is a
///   plain `string` field and a lowered artboard need not carry one — so a
///   name-keyed selection cannot name every root. It is also a producer's
///   vocabulary reaching into the format's, which P5 keeps apart. A name-to-
///   ordinal lookup is a convenience that can be added over this; the reverse
///   is not available.
///
/// An ordinal is a `u32`, is exactly validatable against the document's root
/// count, and crosses the C ABI as a scalar with no encoding, lifetime or
/// allocation question.
///
/// # One root, not a set
///
/// A host shows one root. That is what both integration crates do, what a
/// full-screen panel needs, and what keeps story #838's traversal change a
/// single index space rather than a union of them. Widening to a set later adds
/// a second entry point; narrowing from one would be a breaking change, so the
/// order is not symmetric.
///
/// # No `Default`
///
/// A default would mean root 0, which is the convenience D7 of that record
/// deletes `first_root` to remove: "leaving it in the API is an invitation to
/// hardcode the same thing again under a different name". A struct holding one
/// cannot be filled in with `..Default::default()` and quietly get the first
/// root; it has to say [`ShownRoot::FIRST`], which is a statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShownRoot(u32);

impl ShownRoot {
    /// The document's first root — the artboard a host with no other
    /// instruction shows, and what every host did before story #837.
    pub const FIRST: Self = Self(0);

    /// The `ordinal`-th root, counting from zero over the roots the document
    /// declares.
    ///
    /// Constructing one cannot fail: whether a document has that many roots is a
    /// question about a document, and is answered by [`resolve`].
    pub const fn nth(ordinal: u32) -> Self {
        Self(ordinal)
    }

    /// The ordinal this names.
    pub const fn ordinal(self) -> u32 {
        self.0
    }
}

/// Every root node of `document`, as node indices, in document order.
///
/// A root is a node whose `parent` is [`NO_PARENT`]; the format carries no roots
/// table, so this is a scan of the node table. It reads only the hot half of the
/// file, which [`crate::open`] has already read by the time a host can call
/// this.
///
/// This yields **node** indices, which [`ShownRoot`] exists so an embedder never
/// has to handle. It is here for a consumer that walks every root — the
/// browser's widened bound is the one in the tree — rather than for one choosing
/// between them. A caller that only wants how many there are calls
/// [`root_count`], which says so at the call site; both are one pass over the
/// node table and neither allocates.
pub fn roots<'d>(document: &'d Document<'_>) -> impl Iterator<Item = u32> + 'd {
    let nodes = document.nodes().unwrap_or_default();
    (0..nodes.len())
        .filter(move |&index| nodes.get(index).parent() == NO_PARENT)
        .map(|index| index as u32)
}

/// How many roots `document` declares.
///
/// The number both integration crates report beside a refused [`ShownRoot`], so
/// an embedder can see whether it asked past the end or against a document with
/// no roots at all. A `u32` rather than a `usize` because a root is a node and
/// node indices are `u32` throughout this crate, so the count fits by
/// construction — and because the two numbers exist to be compared with each
/// other.
pub fn root_count(document: &Document<'_>) -> u32 {
    roots(document).count() as u32
}

/// The node index `shown` names, or [`None`] when `document` has no such root.
///
/// [`None`] covers both ways that happens, and [`root_count`] is what tells them
/// apart — it is what both integration crates report beside the refusal: a
/// document with no nodes at all answers zero, and a document whose root count
/// does not reach the ordinal asked for answers how far it does reach. Either way the honest answer
/// is that this host was asked to show something that is not there, which is
/// what stops a prefetch running against a node that does not exist.
///
/// **For [`ShownRoot::FIRST`] over any document that passed the load gate this
/// is node 0**, and the scan cannot answer otherwise: `dashscene_validator`
/// refuses a node whose parent does not precede it, and no parent index precedes
/// zero. That is a fact about the format rather than a gap in coverage, and it
/// stops being the whole story exactly here — for any other ordinal the scan is
/// doing real work.
pub fn resolve(document: &Document<'_>, shown: ShownRoot) -> Option<u32> {
    roots(document).nth(shown.ordinal() as usize)
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
