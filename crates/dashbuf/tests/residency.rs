//! What a load reads, and what it deliberately does not (story #597,
//! epic #594).
//!
//! `docs/decisions/verification-moves-from-open-to-touch.md` moves blob
//! verification off both readers and onto the touch that makes a payload
//! resident. Three claims follow from that, and each is asserted here on a
//! document built for it:
//!
//! - [`open_reads_no_payload_byte`] — the reader resolves every asset entry to
//!   where its payload lies and reads none of them. Asserted by **corrupting
//!   every payload in the file**: a reader that hashed one would refuse the
//!   file, and this one opens it.
//! - [`assets_of_root_is_bounded_by_the_subtree`] — the prefetch set is what one
//!   root's subtree draws, not what the document carries.
//! - [`prefetching_the_shown_root_reads_only_its_payloads`] — the two together,
//!   counted: showing one root reads that root's payload bytes and no others.
//!   This is the criterion of epic #594 in miniature.
//!
//! # The fixture, and why it has the shape it has
//!
//! Two root frames and three descendants, with **five payloads of five distinct
//! lengths**:
//!
//! ```text
//! node 0  root A            paint 0  image fill        -> asset 0  (89 B)
//! node 1    child of 0      paint 2  image fill        -> asset 2  (512 B)
//! node 2      child of 1    paint 3  extra fill layer  -> asset 3  (271 B)
//! node 3    child of 0      paint 4  shape field -> atlas 0 -> asset 4 (37 B)
//! node 4  root B            paint 1  image fill        -> asset 1  (1301 B)
//! ```
//!
//! Every row is doing work, and each one kills a different mutation.
//!
//! - **Two roots**, because a one-root document cannot tell "the shown root's
//!   assets" from "every asset".
//! - **A child under root A**, because a walk that took only the root node's own
//!   paint entry passes every assertion on a flat document.
//! - **A grandchild under that**, because a walk one level deep passes on a
//!   two-level one.
//! - **An extra fill layer and a baked vector shape.** Those are the other two
//!   ways a paint entry reaches an asset — `Paint.extra_fills` and
//!   `Paint.shape_field` through `VectorAtlas.image` — and a traversal following
//!   only `Paint.fill` returns a set that is short by two and still passes a
//!   subtree test built on plain image fills alone.
//! - **Five distinct lengths**, because a byte count over equal payloads is
//!   satisfied by reading the wrong ones.
//! - **The descendants' assets are 2, 3 and 4, not 1**, so a set built by taking
//!   a prefix of the asset table rather than by walking the tree fails.

use dashbuf::OpenError;
use dashbuf::bank::{ColdBank, assemble};
use dashbuf::container::{
    self, Container, ContainerError, FLAVOR_ASSET, FLAVOR_UI, Section, SectionKind,
};
use dashbuf::cost::LoadCost;
use dashbuf::prefetch::{assets_of_root, first_root};
use dashbuf::prefix::{self, Envelope};
use dashbuf::residency::BlobResidency;
use dashbuf::{
    AssetEntry, AssetEntryArgs, AssetKind, AtlasRect, Color, Document, DocumentArgs, Fill,
    FillLayer, FillLayerArgs, ImageFill, ImageFillArgs, ImageFormat, NO_PARENT, Node, NodeArgs,
    Paint, PaintArgs, PlaneBounds, SolidFill, SolidFillArgs, VectorAtlas, VectorAtlasArgs,
    VectorShape, VectorShapeArgs,
};
use flatbuffers::FlatBufferBuilder;

/// The five payloads, in asset-entry order. Distinct lengths and distinct byte
/// values, so a swapped pair is visible in a count as well as in a comparison.
fn payloads() -> Vec<Vec<u8>> {
    vec![
        vec![0xA1; 89],
        vec![0xB2; 1301],
        vec![0xC3; 512],
        vec![0xD4; 271],
        vec![0xE5; 37],
    ]
}

/// The asset entries root A's subtree draws: its own image fill, its child's,
/// its grandchild's stacked fill layer, and the atlas behind its other child's
/// baked vector shape.
const ROOT_A_ASSETS: [u32; 4] = [0, 2, 3, 4];
/// The asset entry root B draws.
const ROOT_B_ASSETS: [u32; 1] = [1];

/// The node index of root B — the second root, which nothing shows.
const ROOT_B_NODE: u32 = 4;

/// A ui-document flatbuffer with the shape the module doc draws.
fn ui_section(payloads: &[Vec<u8>]) -> Vec<u8> {
    let mut builder = FlatBufferBuilder::new();

    let entries: Vec<_> = payloads
        .iter()
        .map(|payload| {
            let hash = builder.create_vector(blake3::hash(payload).as_bytes());
            AssetEntry::create(
                &mut builder,
                &AssetEntryArgs {
                    hash: Some(hash),
                    format: ImageFormat::Png,
                    kind: AssetKind::Image,
                    width: 16,
                    height: 16,
                },
            )
        })
        .collect();
    let assets = builder.create_vector(&entries);

    // One atlas over asset 4, and one baked shape in it — the third way a paint
    // entry reaches an asset, and the only one that goes through two pools.
    let atlas = VectorAtlas::create(
        &mut builder,
        &VectorAtlasArgs {
            image: 4,
            px_per_em: 48.0,
            distance_range: 4.0,
        },
    );
    let vector_atlases = builder.create_vector(&[atlas]);
    let shape = VectorShape::create(
        &mut builder,
        &VectorShapeArgs {
            atlas: 0,
            atlas_rect: Some(&AtlasRect::new(0, 0, 8, 8)),
            plane_bounds: Some(&PlaneBounds::new(0.0, 0.0, 8.0, 8.0)),
        },
    );
    let vector_shapes = builder.create_vector(&[shape]);

    // Paints 0, 1 and 2 are plain image fills over assets 0, 1 and 2.
    let mut paints: Vec<_> = [0u32, 1, 2]
        .into_iter()
        .map(|image| {
            let fill = ImageFill::create(
                &mut builder,
                &ImageFillArgs {
                    image,
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

    // Paint 3: a solid bottom fill with asset 3 stacked over it, so the only
    // route to that asset is `extra_fills`.
    let solid = SolidFill::create(
        &mut builder,
        &SolidFillArgs {
            color: Some(&Color::new(1.0, 1.0, 1.0, 1.0)),
        },
    );
    let stacked = ImageFill::create(
        &mut builder,
        &ImageFillArgs {
            image: 3,
            ..Default::default()
        },
    );
    let layer = FillLayer::create(
        &mut builder,
        &FillLayerArgs {
            fill_type: Fill::ImageFill,
            fill: Some(stacked.as_union_value()),
        },
    );
    let extra_fills = builder.create_vector(&[layer]);
    paints.push(Paint::create(
        &mut builder,
        &PaintArgs {
            fill_type: Fill::SolidFill,
            fill: Some(solid.as_union_value()),
            extra_fills: Some(extra_fills),
            ..Default::default()
        },
    ));

    // Paint 4: a baked vector shape and no fill of its own, so the only route
    // to asset 4 is `shape_field` -> shape 0 -> atlas 0 -> image.
    paints.push(Paint::create(
        &mut builder,
        &PaintArgs {
            shape_field: 0,
            ..Default::default()
        },
    ));
    let paints = builder.create_vector(&paints);

    let nodes: Vec<_> = [
        (NO_PARENT, 0u32), // node 0: root A
        (0, 2),            // node 1: child of root A
        (1, 3),            // node 2: grandchild, stacked fill
        (0, 4),            // node 3: child of root A, baked vector shape
        (NO_PARENT, 1),    // node 4: root B
    ]
    .into_iter()
    .map(|(parent, paint_entry)| {
        Node::create(
            &mut builder,
            &NodeArgs {
                parent,
                paint_entry,
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
            vector_atlases: Some(vector_atlases),
            vector_shapes: Some(vector_shapes),
            assets: Some(assets),
            ..Default::default()
        },
    );
    builder.finish(document, None);
    builder.finished_data().to_vec()
}

/// The fixture assembled into a real `.dsb`, RAW — the null binding, so an
/// asset's canonical hash is the hash of the section carrying it.
fn fixture() -> Vec<u8> {
    let payloads = payloads();
    let ui = ui_section(&payloads);
    let bank = ColdBank::raw(payloads.iter().map(Vec::as_slice));
    assemble(&ui, &bank).expect("the fixture assembles")
}

/// `file` with one byte of every blob payload flipped.
///
/// The section table is untouched, so it still records what each payload
/// *should* hash to and nothing before the blobs notices: the root hash covers
/// the table, not the file.
fn with_every_payload_corrupted(file: &[u8]) -> Vec<u8> {
    let container = Container::parse(file).expect("the fixture parses");
    let mut corrupted = file.to_vec();
    let mut hit = 0;
    for entry in container.sections() {
        if entry.kind == SectionKind::Blob as u16 {
            corrupted[entry.offset as usize] ^= 0xFF;
            hit += 1;
        }
    }
    assert_eq!(
        hit,
        payloads().len(),
        "every payload must be corrupted, or the assertions below prove less than they say"
    );
    corrupted
}

/// `open` resolves every asset entry and reads no payload byte.
///
/// The assertion with teeth is the corrupted file: **`open` opens it**. A
/// reader that hashed a payload — as `open` did before this story, and as
/// `open_verified` still does — cannot, and the second half of this test shows
/// that by refusing the same bytes. Nothing weaker distinguishes the two: both
/// readers return the same entries in the same order over a *good* file.
#[test]
fn open_reads_no_payload_byte() {
    let file = fixture();
    let corrupted = with_every_payload_corrupted(&file);

    let (document, wanted) = dashbuf::open(&corrupted).expect("open reads no payload, so it opens");
    assert_eq!(document.assets().expect("assets").len(), payloads().len());
    assert_eq!(wanted.len(), payloads().len(), "one Wanted per asset entry");

    dashbuf::open_verified(&corrupted)
        .expect_err("the eager reader hashes every payload, so it refuses the same file");

    // And the ranges are real: each names its payload's own extent, and the
    // bytes there are the corrupted ones the touch will refuse.
    let residency = BlobResidency::new();
    for (want, payload) in wanted.iter().zip(payloads()) {
        assert_eq!(
            want.range.end - want.range.start,
            payload.len() as u64,
            "a Wanted's range is the payload's own extent"
        );
        let bytes = &corrupted[want.range.start as usize..want.range.end as usize];
        residency
            .touch(want, bytes)
            .expect_err("a corrupted payload is refused at the touch");
    }
    assert_eq!(residency.ready_count(), 0);
}

/// The prefetch set is what one root's subtree draws.
///
/// Both roots are asserted, in both directions: a walk that returned everything
/// would fail root B's assertion, and a walk that returned only the root node's
/// own paint entry would fail root A's.
#[test]
fn assets_of_root_is_bounded_by_the_subtree() {
    let file = fixture();
    let (document, _) = dashbuf::open(&file).expect("the fixture opens");

    let shown = first_root(&document).expect("the fixture has a root");
    assert_eq!(shown, 0, "the first root is node 0");

    assert_eq!(
        assets_of_root(&document, shown),
        ROOT_A_ASSETS,
        "root A draws its own asset and its child's, and no other"
    );
    assert_eq!(
        assets_of_root(&document, ROOT_B_NODE),
        ROOT_B_ASSETS,
        "root B draws its own asset alone; a subtree is not the document"
    );
}

/// Showing one root reads that root's payload bytes and no others.
///
/// The criterion of epic #594, in miniature and over one document rather than
/// two. Asserted as an exact byte count rather than as "fewer than all": the
/// three payloads have three distinct lengths, so reading the wrong two would
/// give a different number, and reading all three would give a third.
#[test]
fn prefetching_the_shown_root_reads_only_its_payloads() {
    let file = fixture();
    let payloads = payloads();
    let (document, wanted) = dashbuf::open(&file).expect("the fixture opens");

    let residency = BlobResidency::new();
    let cost = LoadCost::new();
    let shown = first_root(&document).expect("the fixture has a root");
    for index in assets_of_root(&document, shown) {
        let want = &wanted[index as usize];
        let bytes = &file[want.range.start as usize..want.range.end as usize];
        residency
            .touch_with_cost(want, bytes, &cost)
            .expect("the shown root's payload is the one the table names");
    }

    let shown_bytes: u64 = ROOT_A_ASSETS
        .iter()
        .map(|index| payloads[*index as usize].len() as u64)
        .sum();
    let every_byte: u64 = payloads.iter().map(|p| p.len() as u64).sum();

    assert_eq!(cost.hashed(), shown_bytes, "the shown root's payloads");
    assert_eq!(cost.copied(), 0, "the mapped path copies nothing");
    assert!(
        shown_bytes < every_byte,
        "the fixture must leave something unread, or this test cannot fail"
    );
    assert_eq!(residency.ready_count(), ROOT_A_ASSETS.len());
    assert!(
        !residency.is_ready(wanted[ROOT_B_ASSETS[0] as usize].section),
        "the root nobody is showing stays cold"
    );
}

/// **Both** readers verify every structured section, not only the two they
/// read.
///
/// `Container::verify_hot` is D1's clause, and it is the only thing covering a
/// structured section a reader does not read. A compiled `.dsb` carries none:
/// `ui_document` and `bindings_manifest` verify the two flavours that exist
/// today, so removing the sweep leaves every other test in the tree passing. A
/// file carrying a third flavour is written here for that reason, and it is
/// what makes the clause falsifiable rather than decorative.
///
/// `prefix::plan` is asserted beside `open` because
/// `docs/decisions/container-parse-reads-a-prefix-through-a-host-reader.md`
/// says the two readers "do **not** apply different rules". Checking only
/// `open` would let the same file be trusted over a mapping and unchecked over
/// a fetch — one rule per target, which is the thing that record exists to
/// prevent.
#[test]
fn both_readers_verify_a_structured_section_they_do_not_read() {
    /// A flavour no reader knows — the case the sweep exists for.
    const UNKNOWN_FLAVOR: u16 = 0x5FFF;

    let payloads = payloads();
    let ui = ui_section(&payloads);
    let unread = vec![0x77; 96];
    let mut sections = vec![
        Section::structured(FLAVOR_UI, &ui),
        Section::structured(UNKNOWN_FLAVOR, &unread),
    ];
    sections.extend(payloads.iter().map(|p| Section::blob(FLAVOR_ASSET, p)));
    let file = container::write(&sections).expect("the sections write");

    dashbuf::open(&file).expect("the file opens as written");

    // One byte of the section nothing reads. Its recorded hash is untouched, so
    // only a sweep over every structured section can notice.
    let container = Container::parse(&file).expect("the written file parses");
    let mut corrupted = file.clone();
    corrupted[container.section(1).offset as usize] ^= 0xFF;

    let error = dashbuf::open(&corrupted).expect_err("a corrupted hot section is refused");
    assert!(
        matches!(
            error,
            OpenError::Container(ContainerError::SectionHashMismatch { index: 1 })
        ),
        "the sweep must be what refuses it, and by section: got {error:?}"
    );

    // The same file through the prefix reader, which is the same rule read from
    // a fetched run rather than from a held file.
    let envelope = Envelope::read(&corrupted, corrupted.len() as u64)
        .expect("the envelope is covered by the root hash and is untouched");
    let hot = &corrupted[..envelope.hot_len() as usize];
    let error = prefix::plan(&envelope, hot).expect_err("the prefix reader refuses it too");
    assert!(
        matches!(
            error,
            OpenError::Container(ContainerError::SectionHashMismatch { index: 1 })
        ),
        "both readers must name it the same way: got {error:?}"
    );
}

/// The two readers name the same payloads.
///
/// `open` holds the file and `prefix::plan` holds a prefix of it, and since this
/// story they answer in the same type. If they ever disagree about a range, a
/// hash or an order, one of the two hosts is loading a different document from
/// the other — and this is the only place that would say so.
#[test]
fn the_two_readers_name_the_same_payloads() {
    let file = fixture();

    let (_, by_open) = dashbuf::open(&file).expect("the fixture opens");

    let envelope = Envelope::read(&file, file.len() as u64).expect("the envelope reads");
    let plan =
        prefix::plan(&envelope, &file[..envelope.hot_len() as usize]).expect("the document plans");

    assert_eq!(by_open, plan.wanted(), "the two readers must not disagree");
}
