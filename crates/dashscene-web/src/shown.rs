//! Which payloads a browser load reads, and where each one lands once it has
//! been read (story #792).
//!
//! R5 says cold-start cost is proportional to what is **shown**, not to file
//! size. The native host has held that since epic #594: it maps the file, reads
//! only the payloads the shown root's subtree draws, and binds ranges into the
//! mapping. This module is the browser's equivalent of the two halves that
//! makes possible — choosing the set, and describing where the bytes will sit —
//! and it is separated from the fetching so that both can be tested without a
//! browser.
//!
//! # A browser has a region after all
//!
//! `dashscene_core::load_document_mapped` binds ranges into an
//! `Arc<dyn Region>` rather than copying payloads, which is what lets the
//! native host read only what it draws. Until this story the browser host said
//! it could not use that path, in a comment that read:
//!
//! > Bounding the read by what is shown needs the mapped loader, which needs a
//! > region, which a browser does not have.
//!
//! That was wrong, and it is worth recording why rather than quietly deleting
//! it. `dashpaint` carries
//! `impl<T: AsRef<[u8]> + Send + Sync> Region for T`, so **a `Vec<u8>` is a
//! region**. A browser cannot map a file, but it can assemble the payloads it
//! fetched into one buffer and bind ranges into that. Nothing in `dashpaint`
//! and nothing at boundary B changes.
//!
//! # The ranges are not the file's
//!
//! That is the one real difference from the native host, and the reason this
//! module exists rather than the fetch loop doing it inline. A mapped host's
//! payload ranges *are* the file's own offsets, because the region is the file.
//! Here the region is a buffer this host packed, holding only the payloads it
//! chose to read, so every range has to be restated relative to it.
//!
//! The layout is computed **before a single byte is fetched**, which is
//! possible because [`Wanted`] already carries each payload's length. Two
//! things follow, and both are the point:
//!
//! - the byte cost of a load is known up front, so the criterion can be
//!   asserted over a document without a network; and
//! - a fetch that returns the wrong number of bytes is detectable, because the
//!   loader knows what it asked for.
//!
//! # The bound is conditional, and that is the story's real finding
//!
//! **The runtime paints every root, not the shown one.** `dashscene-engine`
//! solves `for &root in arena.roots()`, `Arena::dfs_order` walks all of them
//! into one committed table, and a painter walks that table. "The shown root"
//! is a property of the load and of nothing below it.
//!
//! A mapped host survives that: it binds a real range for every entry and only
//! *hashes* the shown root's, so an unread row still decodes — unverified,
//! which is debt #779. A browser cannot, because a payload it did not fetch has
//! no bytes at all.
//!
//! So [`layout`] reads the shown root's assets **only when no other root draws
//! one**, and otherwise reads the union over every root, reporting which
//! through [`Bound`]. The many-frame document R5's criterion is really about —
//! many roots, one payload each — takes the widened path, so **R5 does not hold
//! for that shape on this target**. Issue #822 carries the change that would
//! make the bound unconditional: confining the solve, the committed table and
//! the paint to the shown root.
//!
//! # An entry no root draws gets an empty range
//!
//! The image table takes one row per asset entry — `load_document_mapped` zips
//! the two — so a row must exist for a payload this host never read. It names
//! `0..0`.
//!
//! **That is safe only because every bound payload here is canonical**, and a
//! canonical payload is always an encoded format: `dashbuf` carries no baked
//! variant and `dashc`'s emitter refuses one, so only a derivation manifest can
//! bind a baked rung. `ImageTable::push_mapped` asserts that a *baked* payload's
//! range is exactly the length its format and extent require, and an empty range
//! would fail that assertion. [`crate::document`] refuses a derived binding by
//! name before it reaches here, for the same reason the native host does (issue
//! #640), which is what keeps that assertion out of reach.
//!
//! # It was demonstrated failing before it was made to pass
//!
//! Epic #594 required that of its own criterion, and for the reason v0.13's
//! test tiering exists: a check that has only ever been seen passing is the
//! `t2-check-has-no-teeth` shape. The path this story replaces fetched every
//! payload the document named — `Plan::wanted()` in full — so the demonstration
//! is [`layout`] with its set replaced by every entry, which is that behaviour
//! exactly.
//!
//! Measured that way, over the fixtures below, with the criterion's
//! `many_frames(64, false)` document — sixty-four frames of which one draws:
//!
//! ```text
//! small-root  (1 frame)      4 096 B
//! many-frame  (64 frames)  262 144 B
//! ratio                    64x, against a criterion of 1.00x
//! ```
//!
//! The unbounded path reads all sixty-four payloads because it reads the table
//! rather than the drawing, and sixty-three of them are drawn by nobody. Three
//! of the tests here fail in that state. The ones that pass do so **correctly**:
//! over a document with no assets, and over one whose shown root draws every
//! asset, the bounded set and the whole table are the same set. That is the same
//! property that makes a one-root fixture useless here.
//!
//! An empty row is one **no root draws**, which is what the guard above
//! guarantees — an asset any painted root reaches is fetched, so nothing can
//! ask an empty row for pixels. Without that guarantee this would be a crash
//! rather than a placeholder: `dashscene_gpu::residency`'s `decode_png` would be
//! handed an empty slice, and its `expect` says an image payload has a readable
//! header.

use dashbuf::{Document, Wanted};
use dashscene_core::MappedPayload;

/// What a browser load will read, and where it will put it.
///
/// No `PartialEq`: `MappedPayload` carries none, and comparing two layouts
/// wholesale is not something either this crate or its tests want — the
/// assertions worth making are about the set, the byte count and individual
/// ranges, each of which says which property failed.
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    /// The asset entries to fetch, ascending and without repeats — indices into
    /// the document's asset table, which is the order [`Wanted`]s arrive in.
    pub fetch: Vec<u32>,
    /// One row per asset entry, in entry order, as ranges into the region this
    /// load will assemble. An entry not in [`Layout::fetch`] names `0..0`.
    pub payloads: Vec<MappedPayload>,
    /// How many payload bytes this load will read. This is the whole of what
    /// R5 bounds, and it is known before any of them arrive.
    pub bytes: u64,
    /// Whether the shown root bounded this load, and if not, why not.
    pub bound: Bound,
}

/// What decided the set of payloads a load reads.
///
/// Reported rather than inferred from a count, because "read everything" and
/// "the shown root happens to draw everything" produce the same set and are
/// not the same fact. A host that logged only the number could not tell an
/// embedder which one happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bound {
    /// Only the payloads the shown root's subtree draws. This is R5 holding.
    ShownRoot,
    /// Every asset **any** root draws, because a root other than the shown one
    /// draws one and the runtime paints every root.
    ///
    /// Not every entry in the file: an asset no root draws is still not read,
    /// so this is the widest safe set rather than a surrender.
    ///
    /// **R5 does not hold for such a document on this target**, and saying so
    /// is the point of this variant. Bounding the fetch while the painter still
    /// resolves every rect would leave a row with no bytes for the painter to
    /// decode. The fix that would remove this case is confining the solve, the
    /// committed table and the paint to the shown root, which is a change to
    /// `dashscene-engine`, `dashscene-core` and every painter rather than to
    /// this crate — issue #822.
    EveryRoot,
}

impl Layout {
    /// The payload ranges the fetcher must request, in the order it must append
    /// them — which is [`Layout::fetch`] order, because that is the order the
    /// ranges were laid out in.
    pub fn requests<'w>(
        &self,
        wanted: &'w [Wanted],
    ) -> impl Iterator<Item = &'w Wanted> + use<'w, '_> {
        self.fetch.iter().map(|&index| &wanted[index as usize])
    }
}

/// Plans a load of `document` bounded by the root that is shown.
///
/// `wanted` is `Plan::wanted()` — one entry per asset, in asset-entry order.
/// `root` is the root whose subtree is drawn — named as `dashbuf`'s own
/// `assets_of_root` names it, and deliberately not `shown`: a binding of that
/// name reads as a host keeping its own shown generation, which
/// `demo/tests/host_policy_invariant.rs` forbids and reported here.
///
/// The set comes from `dashbuf::prefetch::assets_of_root`, which is the same
/// call the native host makes and which computes it from the hot document
/// alone. A browser host has the hot run before it fetches any payload, so it
/// can decide what to fetch before fetching anything — which is the property
/// that makes this bounded at all rather than merely tidier.
pub(crate) fn layout(document: &Document<'_>, wanted: &[Wanted], root: u32) -> Layout {
    // **The bound is only safe when nothing else draws.** The runtime paints
    // every root, not the shown one: `dashscene_engine` solves
    // `for &root in arena.roots()`, `Arena::dfs_order` seeds its stack from all
    // of them, and a painter walks the whole committed table. So a payload this
    // load skipped is a payload the painter may still ask for — and on this
    // target skipping it means there are no bytes at all, where a mapped host
    // still names real ones.
    //
    // That asymmetry is the whole reason for this check, and it is why the
    // native host can bind every row and this one cannot. It was found by
    // review rather than by a test: an empty row reaches
    // `dashscene_gpu::residency`'s `decode_png`, whose
    // `expect("an image payload has a readable PNG header")` is false for it.
    let shown = dashbuf::prefetch::assets_of_root(document, root);
    let painted = assets_of_every_root(document);
    // `shown` is a subset of `painted` by construction, so equal lengths mean
    // equal sets and the shown root is the only one that draws.
    let (fetch, bound) = if painted.len() == shown.len() {
        (shown, Bound::ShownRoot)
    } else {
        (painted, Bound::EveryRoot)
    };

    // Every row starts empty; the loop below fills in the ones being read. That
    // is the honest default — a row nobody fetched names no bytes — rather than
    // a range into the file, which would be a range into a region this host
    // does not have.
    let mut payloads: Vec<MappedPayload> = (0..wanted.len())
        .map(|_| MappedPayload::canonical(0..0))
        .collect();

    let mut at = 0u64;
    for &index in &fetch {
        // `assets_of_root` indexes the document's asset table, and `wanted` has
        // one row per asset entry, so the index is in range for a document that
        // passed the load gate. A document that did not is refused before this
        // runs.
        let Some(want) = wanted.get(index as usize) else {
            continue;
        };
        let len = want.range.end - want.range.start;
        payloads[index as usize] = MappedPayload::canonical(at..at + len);
        at += len;
    }

    Layout {
        fetch,
        payloads,
        bytes: at,
        bound,
    }
}

/// Every asset any root draws, ascending and without repeats.
///
/// The union rather than a per-root check, because an asset two roots share is
/// read once and is not a reason to widen anything. Matches
/// `assets_of_root`'s own ordering contract so the two can be compared by
/// length.
fn assets_of_every_root(document: &Document<'_>) -> Vec<u32> {
    let nodes = document.nodes().unwrap_or_default();
    let mut all: Vec<u32> = (0..nodes.len())
        .filter(|&index| nodes.get(index).parent() == dashbuf::NO_PARENT)
        .flat_map(|index| dashbuf::prefetch::assets_of_root(document, index as u32))
        .collect();
    all.sort_unstable();
    all.dedup();
    all
}

#[cfg(test)]
mod tests {
    use super::{Bound, Layout, layout};
    use dashbuf::{
        AssetEntry, AssetEntryArgs, AssetKind, Document, DocumentArgs, Fill, ImageFill,
        ImageFillArgs, ImageFormat, NO_PARENT, Node, NodeArgs, Paint, PaintArgs, Wanted,
    };
    use flatbuffers::FlatBufferBuilder;

    const PAYLOAD: u64 = 4096;

    /// Where the first payload sits in the file. Non-zero because a real file
    /// starts with an envelope and its hot run, and because a region range that
    /// happened to equal the file range would make
    /// `the_ranges_are_into_the_assembled_region_and_not_the_file` vacuous.
    const HEADER: u64 = 8192;

    /// `frames` roots, each its own frame, with `others_draw` deciding whether
    /// the frames after the first carry an image fill of their own.
    ///
    /// Both shapes are real and they behave differently, which is why the
    /// fixture takes it as a parameter rather than one of them being "the"
    /// many-frame document:
    ///
    /// - `false` — the extra frames draw no asset, so bounding the load by the
    ///   shown root is safe and R5 holds.
    /// - `true` — every frame draws its own, so every payload is needed the
    ///   moment the painter walks the committed table, and the load widens to
    ///   every root that draws ([`Bound::EveryRoot`]).
    ///
    /// Roots rather than a nested tree because R5 is about the *shown* root. A
    /// one-root document cannot tell "the shown root's assets" from "every asset
    /// in the file", which is the property that has to be falsifiable here.
    fn many_frames(frames: usize, others_draw: bool) -> Vec<u8> {
        let mut builder = FlatBufferBuilder::new();

        let entries: Vec<_> = (0..frames)
            .map(|frame| {
                let mut hash = [0u8; 32];
                hash[0] = frame as u8;
                let hash = builder.create_vector(&hash);
                AssetEntry::create(
                    &mut builder,
                    &AssetEntryArgs {
                        hash: Some(hash),
                        format: ImageFormat::Png,
                        kind: AssetKind::Image,
                        width: 8,
                        height: 8,
                    },
                )
            })
            .collect();
        let assets = builder.create_vector(&entries);

        let paints: Vec<_> = (0..frames)
            .map(|frame| {
                // Frame 0 always draws its asset. The rest draw one only when
                // the fixture asks for it; otherwise the paint carries no image,
                // which is what a solid-filled frame is.
                if frame == 0 || others_draw {
                    let fill = ImageFill::create(
                        &mut builder,
                        &ImageFillArgs {
                            image: frame as u32,
                            ..Default::default()
                        },
                    );
                    Paint::create(
                        &mut builder,
                        &PaintArgs {
                            fill_type: Fill::ImageFill,
                            fill: Some(fill.as_union_value()),
                            ..Default::default()
                        },
                    )
                } else {
                    Paint::create(&mut builder, &PaintArgs::default())
                }
            })
            .collect();
        let paints = builder.create_vector(&paints);

        let nodes: Vec<_> = (0..frames)
            .map(|frame| {
                Node::create(
                    &mut builder,
                    &NodeArgs {
                        parent: NO_PARENT,
                        paint_entry: frame as u32,
                        ..Default::default()
                    },
                )
            })
            .collect();
        let nodes = builder.create_vector(&nodes);

        let document = Document::create(
            &mut builder,
            &DocumentArgs {
                nodes: Some(nodes),
                paints: Some(paints),
                assets: Some(assets),
                ..Default::default()
            },
        );
        builder.finish(document, None);
        builder.finished_data().to_vec()
    }

    /// One `Wanted` per asset entry, `PAYLOAD` bytes each, laid end to end after
    /// a header — as a file holds them.
    fn wanted_for(count: usize) -> Vec<Wanted> {
        (0..count)
            .map(|index| {
                let start = HEADER + index as u64 * PAYLOAD;
                let mut hash = [0u8; 32];
                hash[0] = index as u8;
                Wanted {
                    section: index,
                    range: start..start + PAYLOAD,
                    hash,
                }
            })
            .collect()
    }

    fn layout_of(bytes: &[u8], wanted: &[Wanted], root: u32) -> Layout {
        let document = dashbuf::root_as_document(bytes).expect("the fixture parses");
        layout(&document, wanted, root)
    }

    /// **The criterion, in the form epic #594 asserts it**: the bytes a load
    /// reads are equal for a small-root document and a many-frame one showing
    /// the same root.
    ///
    /// The many-frame document's other frames draw no asset, which is the shape
    /// the bound is **safe** over — see [`Bound::EveryRoot`] for why the
    /// other shape cannot be bounded while the painter walks every root.
    ///
    /// This is what fails against the path this story replaced, which fetched
    /// every payload the document named.
    #[test]
    fn the_bytes_read_track_the_shown_root_and_not_the_file() {
        let small = many_frames(1, false);
        let many = many_frames(64, false);

        let small = layout_of(&small, &wanted_for(1), 0);
        let many = layout_of(&many, &wanted_for(64), 0);

        assert_eq!(small.bound, Bound::ShownRoot);
        assert_eq!(many.bound, Bound::ShownRoot);
        assert_eq!(
            small.bytes, PAYLOAD,
            "the one-frame document's only payload is the shown root's"
        );
        assert_eq!(
            many.bytes,
            small.bytes,
            "a document with 64 frames showing the same root must read the same bytes as a \
             document with one: R5 bounds cold start by what is shown, not by file size. \
             Reading all of them is {}x",
            many.bytes as f64 / small.bytes as f64
        );
    }

    /// **The guard, and the reason it exists.** When another root draws an
    /// asset, the load widens to the whole document and says so.
    ///
    /// Bounding it would leave that root's image row naming no bytes, and the
    /// runtime paints every root — `dashscene_engine` solves them all,
    /// `Arena::dfs_order` walks them all into one committed table, and a painter
    /// walks the whole table. `dashscene_gpu`'s `decode_png` would then be
    /// handed an empty slice, where its `expect` says an image payload has a
    /// readable header.
    ///
    /// So this asserts the opposite of the criterion above, deliberately: R5
    /// does not hold for this shape on this target, and the honest thing is to
    /// read everything rather than to crash. Issue #822 is what would remove the
    /// case.
    #[test]
    fn a_document_whose_other_roots_draw_reads_every_drawn_payload() {
        let many = many_frames(64, true);
        let layout = layout_of(&many, &wanted_for(64), 0);

        assert_eq!(layout.bound, Bound::EveryRoot);
        assert_eq!(
            layout.fetch.len(),
            64,
            "every frame draws, so every payload is read"
        );
        assert_eq!(layout.bytes, 64 * PAYLOAD);
        for (index, payload) in layout.payloads.iter().enumerate() {
            assert!(
                payload.range.end > payload.range.start,
                "entry {index} would be painted, so it must name bytes rather than an empty range"
            );
        }
    }

    /// **Showing a root that draws nothing still reads what is painted.**
    ///
    /// This is the assertion that says the bound is about what the runtime
    /// *draws*, not about what the host selected. Root 5 draws no asset, so a
    /// set computed from it alone would be empty and the load would fetch
    /// nothing — while root 0, which is painted alongside it, draws asset 0 and
    /// would then be handed an empty row.
    ///
    /// It also pins that the widened set is the union over the roots and not
    /// the whole table: entries 1 to 7 are drawn by nobody in this fixture and
    /// are still not read.
    #[test]
    fn showing_a_root_that_draws_nothing_still_reads_what_is_painted() {
        let many = many_frames(8, false);
        let layout = layout_of(&many, &wanted_for(8), 5);

        assert_eq!(layout.bound, Bound::EveryRoot);
        assert_eq!(layout.fetch, vec![0], "root 0's asset, and nothing else");
        assert_eq!(layout.bytes, PAYLOAD);
    }

    /// Every asset entry gets a row, whether or not it was read, because
    /// `load_document_mapped` zips the rows against the document's asset table
    /// and refuses a different count.
    #[test]
    fn every_asset_entry_gets_a_row_and_the_unread_ones_are_empty() {
        let many = many_frames(8, false);
        let layout = layout_of(&many, &wanted_for(8), 0);

        assert_eq!(layout.bound, Bound::ShownRoot);
        assert_eq!(layout.payloads.len(), 8, "one row per asset entry");
        for (index, payload) in layout.payloads.iter().enumerate() {
            let read = index == 0;
            assert_eq!(
                payload.range.end > payload.range.start,
                read,
                "entry {index} should {} have bytes",
                if read { "" } else { "not" }
            );
            assert!(
                payload.derived.is_none(),
                "every row this host binds is canonical, which is what keeps the baked-length \
                 assertion in push_mapped out of reach for an empty range"
            );
        }
    }

    /// The ranges are into the assembled region, laid end to end in fetch
    /// order — **not** the file's own offsets.
    ///
    /// This is the assertion that distinguishes the region from the file. The
    /// fixture puts the payloads after a header, so a loader that passed the
    /// file's offsets through would name a range past the end of a buffer that
    /// never held them.
    #[test]
    fn the_ranges_are_into_the_assembled_region_and_not_the_file() {
        let many = many_frames(8, false);
        let wanted = wanted_for(8);
        let layout = layout_of(&many, &wanted, 0);

        assert_eq!(
            wanted[0].range,
            HEADER..HEADER + PAYLOAD,
            "the file holds the payload after its header"
        );
        assert_eq!(
            layout.payloads[0].range,
            0..PAYLOAD,
            "the region holds it at the start, because it is the only payload in it"
        );
        assert_eq!(layout.bytes, PAYLOAD);
    }

    /// A shown root drawing several payloads packs them in fetch order,
    /// contiguously, with no gap and no overlap — which is what the fetch loop
    /// relies on when it appends each response to the buffer.
    #[test]
    fn several_payloads_pack_contiguously_in_fetch_order() {
        // Distinct lengths, so a swapped pair is visible in the offsets.
        let wanted: Vec<Wanted> = [100u64, 200, 400, 800]
            .into_iter()
            .enumerate()
            .scan(HEADER, |at, (index, len)| {
                let start = *at;
                *at += len;
                let mut hash = [0u8; 32];
                hash[0] = index as u8;
                Some(Wanted {
                    section: index,
                    range: start..start + len,
                    hash,
                })
            })
            .collect();

        let bytes = one_root_drawing_all(4);
        let layout = layout_of(&bytes, &wanted, 0);

        assert_eq!(layout.bound, Bound::ShownRoot);
        assert_eq!(layout.fetch, vec![0, 1, 2, 3]);
        assert_eq!(layout.bytes, 100 + 200 + 400 + 800);
        let ranges: Vec<_> = layout
            .payloads
            .iter()
            .map(|payload| payload.range.clone())
            .collect();
        assert_eq!(
            ranges,
            vec![0..100, 100..300, 300..700, 700..1500],
            "each payload starts where the previous one ended"
        );
    }

    /// One root whose subtree draws every one of `count` assets: the root plus
    /// `count - 1` children, one image fill each. One root, so the guard has no
    /// other root to find.
    fn one_root_drawing_all(count: usize) -> Vec<u8> {
        let mut builder = FlatBufferBuilder::new();

        let entries: Vec<_> = (0..count)
            .map(|index| {
                let mut hash = [0u8; 32];
                hash[0] = index as u8;
                let hash = builder.create_vector(&hash);
                AssetEntry::create(
                    &mut builder,
                    &AssetEntryArgs {
                        hash: Some(hash),
                        format: ImageFormat::Png,
                        kind: AssetKind::Image,
                        width: 8,
                        height: 8,
                    },
                )
            })
            .collect();
        let assets = builder.create_vector(&entries);

        let paints: Vec<_> = (0..count)
            .map(|index| {
                let fill = ImageFill::create(
                    &mut builder,
                    &ImageFillArgs {
                        image: index as u32,
                        ..Default::default()
                    },
                );
                Paint::create(
                    &mut builder,
                    &PaintArgs {
                        fill_type: Fill::ImageFill,
                        fill: Some(fill.as_union_value()),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let paints = builder.create_vector(&paints);

        let nodes: Vec<_> = (0..count)
            .map(|index| {
                Node::create(
                    &mut builder,
                    &NodeArgs {
                        // Node 0 is the root; every other node is its child, so
                        // the whole document is one subtree.
                        parent: if index == 0 { NO_PARENT } else { 0 },
                        paint_entry: index as u32,
                        ..Default::default()
                    },
                )
            })
            .collect();
        let nodes = builder.create_vector(&nodes);

        let document = Document::create(
            &mut builder,
            &DocumentArgs {
                nodes: Some(nodes),
                paints: Some(paints),
                assets: Some(assets),
                ..Default::default()
            },
        );
        builder.finish(document, None);
        builder.finished_data().to_vec()
    }

    /// A document with no assets plans no reads and no rows, rather than
    /// failing. Eight of the ten committed goldens are that shape.
    #[test]
    fn a_document_with_no_assets_reads_nothing() {
        let bytes = one_root_drawing_all(0);
        let layout = layout_of(&bytes, &[], 0);
        assert!(layout.fetch.is_empty());
        assert!(layout.payloads.is_empty());
        assert_eq!(layout.bytes, 0);
    }
}
