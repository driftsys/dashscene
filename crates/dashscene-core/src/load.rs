//! Loading a `.dsb` document into the arena — the document→runtime path
//! (docs/design/dashbuf.md, docs/design/dashc.md).
//!
//! A `.dsb` is the serialized *intent* (P1), and the arena is the runtime's
//! intent model, so loading is a straight replay of the document's nodes
//! through the ordinary producer API: `add_node` + `set_prop` + `commit`. It
//! adds no semantics — a loaded scene is indistinguishable from the same
//! scene staged by hand, and a test pins exactly that.
//!
//! # This function assumes a validated document (P4)
//!
//! It does not re-check referential integrity, and it panics on an index
//! that misses — the same contract as `PaintTable::resolve` and the
//! `Painter` trait. The caller runs the gates first, and there are two of
//! them.
//!
//! **There are also two read contracts, and which one a host takes decides
//! what its cold start costs** (`docs/decisions/verification-moves-from-open-to-touch.md`).
//! A host that holds bytes it cannot borrow from — an embedded document, a
//! browser's fetched buffers — reads eagerly and copies into the arena:
//!
//! ```text
//! // `dashbuf::open_verified` runs the envelope check, hashes every payload,
//! // and binds them to the document's asset entries.
//! let (doc, payloads) = dashbuf::open_verified(file_bytes)?;  // container + verifier
//! let report = dashscene_validator::validate_document(&doc);  // load gate: references
//! if report.has_errors() { /* refuse; never load */ }
//! load_document(&doc, &payloads, &mut arena);    // safe iff the gate passed
//! ```
//!
//! A host that mapped the file reads where each payload lies, makes resident
//! only what the shown root draws, and binds ranges — so no payload it does not
//! draw is ever read, and none is copied:
//!
//! ```text
//! let (doc, wanted) = dashbuf::open(file_bytes)?;   // reads no payload byte
//! let report = dashscene_validator::validate_document(&doc);
//! if report.has_errors() { /* refuse; never load */ }
//! let residency = dashbuf::residency::BlobResidency::new();
//! let root = dashbuf::prefetch::resolve(&doc, shown_root)?;  // which artboard
//! for index in dashbuf::prefetch::assets_of_root(&doc, root) {
//!     let want = &wanted[index as usize];        // touch + hash + mark ready
//!     residency.touch(want, &file_bytes[want.range.start as usize..want.range.end as usize])?;
//! }
//! load_document_mapped(&doc, region, &payloads, &mut arena);
//! ```
//!
//! `dashscene-validator` is published *after* `dashscene-core`, so this
//! crate cannot call it — which is exactly why the contract is stated here
//! rather than enforced here.

use std::ops::Range;
use std::sync::Arc;

use dashbuf::cost::LoadCost;
use dashbuf::prefetch::ShownRoot;
use dashbuf::{
    BindingTransform, Document, Fill, NO_CONTRIBUTION, NO_FIELD, NO_FRAGMENT, NO_PAINT, NO_PARENT,
    NO_TEXT, NO_TEXT_STYLE, VariantPropValue, Wanted,
};
use dashpaint::Region;

use crate::arena::{
    Arena, AxisSizing, CrossAxisAlign, Easing, GridTrack, Keyframe, LayoutMode, LoopTrack,
    MainAxisAlign, NodeId, Placeholder, Prop, PropTransition, TextAlign, TextAlignV, TextStyle,
    TransitionSpec, VariantMember, VariantTransition, VariantValue,
};
use crate::bindings::{Channel, ScalarTransform, SignalId};
use crate::committed::{
    Blur, BlurKind, Color, FillSpec, Gradient, GradientKind, GradientStop, ImageAsset, ImageFormat,
    Mat23, ScaleMode, Shadow, ShadowKind, Stroke, StrokeAlign, Vec2, VectorField,
};

/// One row's curve (story #771), the same shape as [`variant_value`]
/// above: every arm the schema names has a case, and an unknown one is
/// unreachable because the load gate refused the document first (P4).
///
/// A macro rather than a function, because two schema tables carry a
/// `TransitionSpec` union — `PropTransition` and `LoopTrack` (story
/// #772) — and `flatc` generates the `spec_type` / `spec_as_*` accessors
/// as inherent methods on each table rather than through a trait, so
/// there is no bound a function could take. One copy of the arm-for-arm
/// conversion is the point: a fourth union arm must not be able to land
/// in one reader and not the other.
macro_rules! transition_spec {
    ($row:expr) => {{
        let row = $row;
        match row.spec_type() {
            dashbuf::TransitionSpec::TweenSpec => {
                let t = row.spec_as_tween_spec().expect("TweenSpec present");
                TransitionSpec::Tween {
                    duration: t.duration(),
                    easing: easing_of(t.easing()),
                }
            }
            dashbuf::TransitionSpec::SpringSpec => {
                let s = row.spec_as_spring_spec().expect("SpringSpec present");
                TransitionSpec::Spring {
                    stiffness: s.stiffness(),
                    damping_ratio: s.damping_ratio(),
                }
            }
            dashbuf::TransitionSpec::KeyframesSpec => {
                let k = row.spec_as_keyframes_spec().expect("KeyframesSpec present");
                TransitionSpec::Keyframes {
                    duration: k.duration(),
                    frames: k
                        .frames()
                        .unwrap_or_default()
                        .iter()
                        .map(|frame| Keyframe {
                            t: frame.t(),
                            value: frame.value(),
                        })
                        .collect(),
                }
            }
            other => {
                unreachable!("unknown TransitionSpec {other:?}: rejected by the load gate (P4)")
            }
        }
    }};
}

/// Replays a validated `.dsb` document into `arena` and commits it,
/// returning the commit's generation.
///
/// `payloads` binds the document's asset entries to their bytes, one per entry
/// in entry order. `dashbuf::open_verified` produces exactly that from a `.dsb`
/// file, and
/// the panic below fires if the two lengths disagree — a caller that bound the
/// wrong set would otherwise repaint nodes with another document's assets.
///
/// The document's nodes are appended to whatever the arena already holds —
/// the loader is a producer, not an owner, matching `dashlang::Scene::build`.
///
/// # Panics
///
/// On any index the document carries that does not resolve (a paint entry,
/// an image asset, a string, a text style, a parent). Those are precisely
/// what `dashscene_validator::validate_document` reports as errors, so a
/// panic here means the caller skipped the gate.
pub fn load_document(doc: &Document<'_>, payloads: &[&[u8]], arena: &mut Arena) -> u64 {
    let bound: Vec<BoundPayload<'_>> = payloads
        .iter()
        .map(|bytes| BoundPayload::canonical(bytes))
        .collect();
    load_document_bound(doc, &bound, arena)
}

/// One asset's payload, as a host binds it.
///
/// # Why a binding may disagree with the document
///
/// A document records the **canonical** payload's format and never carries a
/// derivation: `dashpack` writes derived payloads beside the document and does
/// not rewrite it
/// (`docs/decisions/asset-model-content-addressed-blobs.md`). So the format a
/// document states is the format of the bytes it was *authored* from, and a
/// host that binds a derived payload — the ASTC rung its profile selected — is
/// binding bytes the document has no name for.
///
/// Until story #640 the loader took the format from the document entry and the
/// bytes from the binding, with nothing checking that the two described the
/// same thing. A host binding an ASTC payload got an asset tagged `Png`, and
/// the painter had no way to find out. This type is where the two are stated
/// together.
#[derive(Debug, Clone, Copy)]
pub struct BoundPayload<'a> {
    /// The bytes bound to this asset entry.
    pub bytes: &'a [u8],
    /// What those bytes are, when they are a derivation. `None` means they are
    /// the document's own canonical payload, and the format is read from the
    /// entry — which is the only case that existed before story #640.
    pub derived: Option<ImageFormat>,
}

impl<'a> BoundPayload<'a> {
    /// The document's own payload: bytes whose format the entry already names.
    pub fn canonical(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            derived: None,
        }
    }

    /// A derived payload in `format` — the rung a profile selected.
    pub fn derived(bytes: &'a [u8], format: ImageFormat) -> Self {
        Self {
            bytes,
            derived: Some(format),
        }
    }
}

/// [`load_document`], with each payload free to state its own format.
///
/// The entry point a host uses when it binds `dashpack`'s output rather than
/// the document's canonical payloads — see [`BoundPayload`]. Everything else is
/// identical, and `load_document` is this function with every payload bound as
/// canonical.
///
/// # Panics
///
/// As [`load_document`].
pub fn load_document_bound(
    doc: &Document<'_>,
    payloads: &[BoundPayload<'_>],
    arena: &mut Arena,
) -> u64 {
    load_document_bound_with_cost(doc, payloads, arena, &LoadCost::new())
}

/// [`load_document_bound`], recording into `cost` the asset payload bytes it
/// copies.
///
/// The loader's half of the startup-scaling counter
/// (`docs/decisions/startup-scaling-is-measured-by-a-counter.md`): D2 counts a
/// payload's bytes whether they are read to hash them or read to copy them,
/// because each alone makes cold start scale with file size and a counter
/// seeing only one cannot falsify the other.
/// `dashbuf::residency::BlobResidency::touch_with_cost` records the hash, at the
/// moment a payload is made resident; this records the copy into [`ImageAsset`],
/// one payload at a time.
///
/// [`load_document_bound`] is this call with the count discarded, so there is
/// one implementation and not two that could drift.
///
/// # Panics
///
/// As [`load_document`].
pub fn load_document_bound_with_cost(
    doc: &Document<'_>,
    payloads: &[BoundPayload<'_>],
    arena: &mut Arena,
    cost: &LoadCost,
) -> u64 {
    load_inner(doc, Payloads::Bound(payloads), arena, cost)
}

/// One asset's payload, as a host binds it out of a **mapped** file: where the
/// bytes are, rather than what they are.
///
/// The mapped counterpart of [`BoundPayload`], and the shape
/// `dashbuf::prefix::Plan::wanted` already produces — one range per asset entry,
/// in entry order. `docs/decisions/assets-borrow-from-the-mapping.md` D6 takes
/// ranges rather than slices deliberately: recovering an offset by subtracting
/// one slice's pointer from another's works, and is correct only until someone
/// passes a slice from somewhere else.
#[derive(Debug, Clone)]
pub struct MappedPayload {
    /// Where the payload lies in the region, as a byte range.
    pub range: Range<u64>,
    /// What those bytes are, when they are a derivation. `None` means the
    /// document's own canonical payload, whose format the entry names — the
    /// same rule [`BoundPayload::derived`] states, and stated again here so that
    /// the format and the bytes stay one decision rather than two (issue #640).
    pub derived: Option<ImageFormat>,
}

impl MappedPayload {
    /// The document's own payload: a range whose format the entry already
    /// names.
    pub fn canonical(range: Range<u64>) -> Self {
        Self {
            range,
            derived: None,
        }
    }

    /// A derived payload in `format` — the rung a profile selected.
    pub fn derived(range: Range<u64>, format: ImageFormat) -> Self {
        Self {
            range,
            derived: Some(format),
        }
    }
}

/// [`load_document`] for a document whose payloads stay in the region they were
/// mapped from.
///
/// The arm epic #594 exists to reach: no asset payload is copied between the
/// mapping and the painter. The arena's image table adopts `region` and each
/// row is a range into it, so the bytes a painter resolves are the file's own
/// pages (`docs/decisions/assets-borrow-from-the-mapping.md`).
///
/// `region` is whatever the ranges are relative to, and is held by the table
/// for as long as the arena holds the table. There are two callers and they
/// differ on exactly that point: the native host's region is the whole mapped
/// file, so its ranges are the file's own offsets, while the browser host's is
/// a buffer it packed from the payloads it fetched, so its ranges are relative
/// to that (story #792).
///
/// **It takes no [`LoadCost`]**, unlike [`load_document_bound_with_cost`], and
/// that is the point rather than an omission: this path reads no payload byte,
/// so there is nothing to record and a counter here could only ever report
/// zero. What holds the claim instead is a test that resolves the arena's own
/// image bytes and asserts they are pointers into the region — an address, not
/// a count, because a copy has equal bytes.
///
/// # Panics
///
/// As [`load_document`], and additionally if `arena` already holds image
/// assets: a table is owned or mapped and never both (D1).
pub fn load_document_mapped(
    doc: &Document<'_>,
    region: Arc<dyn Region>,
    payloads: &[MappedPayload],
    arena: &mut Arena,
) -> u64 {
    load_inner(
        doc,
        Payloads::Mapped { region, payloads },
        arena,
        &LoadCost::new(),
    )
}

/// The first asset entry whose bound payload is **not** the document's own.
///
/// A `.dsb` records the **canonical** payload's hash and never carries a
/// derivation, so a host binding `dashpack`'s output is binding bytes the
/// document has no name for. [`load_document_bound`] finds that out by parsing
/// the payload's header; a mapped load reads no payload header by design, so
/// nothing downstream would catch a KTX2 tagged as a `Png` — the mistake issue
/// #640 exists to prevent. A host that cannot name a rung refuses the file
/// instead, and this is the comparison it refuses on.
///
/// Returns the **index** rather than an error so that each host names its own
/// source in its own error type: `dashscene-desktop` reports a path,
/// `dashscene-web` a URL, and `dashscene-ffi` a path again. Three error types,
/// one comparison — which is the point, because the comparison was written
/// twice before this existed.
///
/// `wanted` is what `dashbuf::open` or `dashbuf::prefix::Plan` resolved, one
/// per asset entry in entry order. A `wanted` shorter than the document's asset
/// table reports only over the pairs that exist; the length disagreement itself
/// is [`load_document_mapped`]'s to refuse, and it does.
pub fn first_derived_payload(doc: &Document<'_>, wanted: &[Wanted]) -> Option<u32> {
    let entries = doc.assets().unwrap_or_default();
    wanted
        .iter()
        .zip(entries.iter())
        .position(|(want, entry)| want.hash != entry.hash().bytes())
        .map(|index| index as u32)
}

/// Confines the traversal to the root `shown_root` names, correcting the
/// document ordinal into the arena node it actually named.
///
/// `roots_before` is how many roots the arena held **before** this load. The
/// loader appends — a document is every artboard it carries, and dropping one
/// at load would make the file unreadable rather than unshown — so a document
/// ordinal and an arena index agree only when the arena held nothing. Passing
/// the ordinal straight through would confine the traversal to the *first*
/// document's root while the prefetch read this one's: the wrong artboard,
/// solved and painted, with nothing to report it (issue #943).
///
/// Named by node, because [`crate::Txn::show_root`] takes the arena's own
/// vocabulary and this is the one place holding both the document and the arena
/// it was appended to.
///
/// A commit of its own rather than a parameter on the loader: the load has
/// already committed by the time this runs, and one extra commit per load is
/// the cheaper of the two ways to say it — the other being a signature change
/// on three public loaders and every call site.
///
/// `source` is what the diagnostic below names — a path, a URL, whatever the
/// caller loaded from.
///
/// # Panics
///
/// If the ordinal names no appended root.
///
/// **A named panic rather than a typed error, deliberately.** Every caller has
/// already proved the document carries that root, through
/// `dashbuf::prefetch::resolve`, so no honest error value can describe this
/// arm: a count of the roots the document carries would render "carries 2
/// roots, and root 1 was asked for" — an in-range ask reported as a failure.
///
/// What it is instead is this crate promising one arena root per document root
/// and not delivering. That is an invariant of `dashscene-core` rather than an
/// embedder error, and P4's answer to a broken invariant is a diagnostic that
/// names it. [`crate::Txn::use_mapped_pool`] already panics by name on this
/// same path, so an abort here is the established behaviour of this loader
/// rather than a new one.
///
/// That reversed a review finding on `dashscene-desktop`'s first cut, which
/// read the abort as the one panic on a path where every other failure is a
/// `Result`. It is not, for the reason above, and the finding is recorded here
/// so the next reader does not re-raise it.
pub fn show_appended_root(
    doc: &Document<'_>,
    shown_root: ShownRoot,
    roots_before: usize,
    source: &dyn std::fmt::Display,
    arena: &mut Arena,
) {
    let shown = *arena
        .roots()
        .get(roots_before + shown_root.ordinal() as usize)
        .unwrap_or_else(|| {
            // Inside the closure: nothing here runs on the ordinary path, and
            // `saturating_sub` so a shrunken root list cannot replace this
            // diagnostic with a bare subtraction overflow.
            let appended = arena.roots().len().saturating_sub(roots_before);
            panic!(
                "{source} declares {} root(s) and this load appended {appended} to the arena, so \
                 ordinal {} names no node: `load_document_mapped` appends one arena root per \
                 document root, and `dashbuf::prefetch::resolve` already proved this document \
                 carries that root",
                dashbuf::prefetch::root_count(doc),
                shown_root.ordinal(),
            )
        });
    let mut txn = arena.open();
    txn.show_root(Some(shown));
    txn.commit();
}

/// How the caller bound this document's asset payloads: by value, or by range
/// into a region.
///
/// The two arms differ in the asset step and nowhere else, which is why they
/// are an argument to one loader rather than two loaders. Everything after the
/// assets — nodes, paints, strings, text styles, the vector pools, the variant
/// replay — is the same replay through the same producer API.
enum Payloads<'a> {
    /// Bytes in hand, copied into the arena's own pool.
    Bound(&'a [BoundPayload<'a>]),
    /// Ranges into a region the arena's table points at and does not own.
    Mapped {
        region: Arc<dyn Region>,
        payloads: &'a [MappedPayload],
    },
}

impl Payloads<'_> {
    /// How many payloads the caller bound — checked against the document's
    /// asset count before anything is staged.
    fn len(&self) -> usize {
        match self {
            Payloads::Bound(payloads) => payloads.len(),
            Payloads::Mapped { payloads, .. } => payloads.len(),
        }
    }
}

fn load_inner(
    doc: &Document<'_>,
    payloads: Payloads<'_>,
    arena: &mut Arena,
    cost: &LoadCost,
) -> u64 {
    let nodes = doc.nodes().unwrap_or_default();
    let paints = doc.paints().unwrap_or_default();
    let strings = doc.strings().unwrap_or_default();
    let text_styles = doc.text_styles().unwrap_or_default();

    let mut txn = arena.open();

    // Assets first: a paint entry's image fill references them by index, so
    // they must exist before any paint prop is staged.
    //
    // Since story #107 the document carries asset *identity and metadata*, not
    // bytes (P1 applied to assets). `payloads` is the caller's binding of each
    // entry's content hash to the bytes it names — resolved from the file's blob
    // sections, which `dashbuf::open_verified` does for the ordinary case. The
    // arena is
    // where the two rejoin.
    //
    // The document's indices are 0..n, but the arena may already hold assets
    // from an earlier load, so the document's index is NOT the arena's. Keep
    // the mapping and rewrite every image fill through it — assuming they
    // coincide would silently repaint one document's nodes with another
    // document's assets.
    let entries = doc.assets().unwrap_or_default();
    assert_eq!(
        payloads.len(),
        entries.len(),
        "the caller bound {} payloads for {} asset entries: every entry's hash resolves to \
         exactly one payload through the binding, and the null binding is the identity map \
         (docs/decisions/asset-model-content-addressed-blobs.md)",
        payloads.len(),
        entries.len()
    );
    // Both arms decide the format the same way: the binding's when it states
    // one, the document's otherwise. A derivation is what the host resolved for
    // its profile and its painter; the document only ever knows what the asset
    // was authored as.
    let image_of: Vec<u32> = match &payloads {
        Payloads::Bound(bound) => entries
            .iter()
            .zip(*bound)
            .map(|(entry, payload)| {
                // The copy the startup-scaling counter exists to see: every
                // bound payload's bytes are read out of the file and into an
                // owned `Vec`, so cold-start cost tracks total asset bytes
                // rather than the shown root. Recorded beside the copy rather
                // than summed from `payloads` by the caller, so that a change
                // to what is copied moves the count with it.
                cost.record_copied(payload.bytes.len() as u64);
                let asset = ImageAsset {
                    format: payload
                        .derived
                        .unwrap_or_else(|| image_format(entry.format())),
                    bytes: payload.bytes.to_vec(),
                };
                // An encoded payload's extent is in its own header and the
                // table reads it there. A baked one has no header, so the
                // document's recorded extent is passed through — the same
                // number, since a rung is a block footprint and not a mip
                // level, and since `dashscene-validator`'s
                // `asset.extent-mismatch` has already checked it against the
                // canonical payload (issue #716).
                //
                // The branch is on the *format*, not on whether the payload was
                // bound as a derivation: what decides is whether the bytes
                // carry a header, and that is what the format says. A
                // document's own payload is always encoded — `dashbuf` carries
                // no baked variant and `dashc`'s emitter refuses one by name —
                // so only a binding can reach the second arm.
                if asset.format.is_encoded() {
                    txn.add_image(asset)
                } else {
                    txn.add_baked_image(asset, entry.width(), entry.height())
                }
            })
            .collect(),
        Payloads::Mapped { region, payloads } => {
            // The table adopts the region before any row names a range into it,
            // and refuses to do so over already-staged assets — a table is
            // owned or mapped and never both (D1).
            txn.use_mapped_pool(Arc::clone(region));
            entries
                .iter()
                .zip(*payloads)
                .map(|(entry, payload)| {
                    let format = payload
                        .derived
                        .unwrap_or_else(|| image_format(entry.format()));
                    // The extent comes from the entry for **both** formats
                    // here, where the bound arm reads an encoded payload's own
                    // header. Reading that header means touching the payload,
                    // which is the one thing this path exists not to do.
                    //
                    // So the entry is trusted, and what that trust rests on
                    // differs by arm and is worth stating rather than blurring.
                    // For a canonical payload it rests on the producer:
                    // `dashscene-validator`'s `asset.extent-mismatch` compares
                    // the entry against the payload's own header, and `dashc`
                    // runs it at compile time over the bytes it is emitting
                    // (issue #716). **No host runs it at load** — the two
                    // load-time gates are the envelope's hashes and
                    // `validate_document` — so this is a property of a
                    // well-formed file rather than something checked here.
                    //
                    // For a derived payload nothing could check it anyway: the
                    // document records no derivation, and `asset.extent-mismatch`
                    // reads headers `image_id` recognises, which a baked rung
                    // has none of. What does hold there is
                    // `ImageTable::push_mapped`'s own assertion that the range
                    // is the length the format and extent require — arithmetic,
                    // no page fault, and the only place the disagreement can be
                    // named.
                    txn.add_mapped_image(
                        format,
                        payload.range.start,
                        payload.range.end - payload.range.start,
                        entry.width(),
                        entry.height(),
                    )
                })
                .collect()
        }
    };

    // The baked-vector pools (story B1). Each `VectorShape` resolves to a
    // flat, self-contained `VectorField` the painter samples: its atlas's
    // asset index is rewritten through `image_of` (the atlas PNG is an
    // ordinary asset-table entry), and the atlas's `distance_range` folds in
    // beside the shape's own rect and plane bounds. The atlas's `px_per_em`
    // does not — the painter derives scale from the rect/bounds ratio
    // directly, so carrying the bake resolution past load time would be
    // dead weight on the boundary-B struct (debt #358). A paint entry's
    // `shape_field` then indexes this vector directly. All indices are
    // validated upstream (P4), so a miss is a panic, matching the
    // paint/image resolution above.
    let vector_atlases = doc.vector_atlases().unwrap_or_default();
    let shape_of: Vec<VectorField> = doc
        .vector_shapes()
        .unwrap_or_default()
        .iter()
        .map(|shape| {
            let atlas = vector_atlases.get(shape.atlas() as usize);
            let rect = shape
                .atlas_rect()
                .expect("vector shape carries an atlas rect (validated upstream, P4)");
            let plane = shape
                .plane_bounds()
                .expect("vector shape carries plane bounds (validated upstream, P4)");
            VectorField {
                image: image_of[atlas.image() as usize],
                atlas_rect: [rect.x(), rect.y(), rect.width(), rect.height()],
                plane_bounds: [plane.left(), plane.top(), plane.right(), plane.bottom()],
                distance_range: atlas.distance_range(),
            }
        })
        .collect();

    // The node array is in DFS order, so a parent is always staged before
    // its children and `ids[parent]` is always populated by the time a child
    // reads it (the load gate's `node.parent-not-before-child` rule is what
    // makes this safe to assume).
    let mut ids: Vec<NodeId> = Vec::with_capacity(nodes.len());

    for node in nodes.iter() {
        let parent = match node.parent() {
            NO_PARENT => None,
            index => Some(ids[index as usize]),
        };
        let id = txn.add_node(parent, node.name());
        ids.push(id);

        if let Some(layout) = node.layout() {
            txn.set_prop(id, Prop::X(layout.x()));
            txn.set_prop(id, Prop::Y(layout.y()));
            txn.set_prop(id, Prop::Width(layout.width()));
            txn.set_prop(id, Prop::Height(layout.height()));
        }

        // `paint_entry` supersedes the v0.1 `paint` shorthand — the load
        // gate rejects a node that sets both (`paint.conflicting-representation`).
        if node.paint_entry() != NO_PAINT {
            let paint = paints.get(node.paint_entry() as usize);
            load_paint(&mut txn, id, &paint, &image_of, &shape_of);
        } else if let Some(solid) = node.paint()
            && let Some(color) = solid.color()
        {
            txn.set_prop(id, Prop::Fill(color_of(color)));
        }

        // Story #1126: the placeholder surface. Presence of the table is
        // what makes the node a placeholder, so an absent one stages nothing
        // and the node reads back as ordinary. Nothing resolves it — carried
        // intent, like `Prop::Text` at v0.5.
        if let Some(declared) = node.placeholder() {
            // Each index read once and named: the guard and the lookup must be
            // the same field, and two reads of two accessors is exactly the
            // shape where editing one leaves the other behind.
            let contribution = declared.contribution_id();
            let fragment = declared.fragment_ref();
            txn.set_prop(
                id,
                Prop::Placeholder(Placeholder {
                    contribution_id: (contribution != NO_CONTRIBUTION)
                        .then(|| strings.get(contribution as usize).to_owned()),
                    fragment_ref: (fragment != NO_FRAGMENT)
                        .then(|| strings.get(fragment as usize).to_owned()),
                    declared_size: declared.declared_size().map(|v| (v.x(), v.y())),
                    interim_fill: fill_spec_of(&declared, &image_of),
                }),
            );
        }

        if node.text() != NO_TEXT {
            txn.set_prop(id, Prop::Text(strings.get(node.text() as usize).to_owned()));
        }
        if node.text_style() != NO_TEXT_STYLE {
            let style = text_styles.get(node.text_style() as usize);
            txn.set_prop(
                id,
                Prop::TextStyle(TextStyle {
                    family: style.family().to_owned(),
                    size: style.size(),
                    weight: style.weight(),
                    // Never defaulted. An absent color is `text.style-no-color`
                    // at the load gate, so it cannot reach here — and inventing
                    // one would be exactly the silent discovery P4 forbids.
                    color: color_of(style.color().expect(
                        "text style carries a color (validated upstream, P4: text.style-no-color)",
                    )),
                    // The four v0.9 axes (story #310). Absent reads back as the
                    // schema default (auto line height, zero spacing, Left/Top),
                    // so a pre-#310 document loads unchanged.
                    line_height_px: style.line_height_px(),
                    letter_spacing: style.letter_spacing(),
                    text_align: text_align_of(style.text_align()),
                    text_align_v: text_align_v_of(style.text_align_v()),
                    // Story #341: standard ligatures off. Absent reads back
                    // `false` (ligatures on, the pre-#341 default), so an
                    // older document loads unchanged.
                    ligatures_off: style.ligatures_off(),
                }),
            );
        }

        if let Some(flex) = node.flex() {
            txn.set_prop(id, Prop::Mode(layout_mode(flex.mode())));
            txn.set_prop(id, Prop::Gap(flex.gap()));
            if let Some(p) = flex.padding() {
                txn.set_prop(
                    id,
                    Prop::Padding {
                        left: p.left(),
                        top: p.top(),
                        right: p.right(),
                        bottom: p.bottom(),
                    },
                );
            }
            txn.set_prop(id, Prop::MainAlign(main_align(flex.main_align())));
            txn.set_prop(id, Prop::CrossAlign(cross_align(flex.cross_align())));
            // Absent cross gap means follows-`gap`, absent track lists
            // mean implicit auto tracks — absence of intent stages no
            // prop (P1), like min/max below.
            if let Some(v) = flex.cross_gap() {
                txn.set_prop(id, Prop::CrossGap(v));
            }
            if let Some(rows) = flex.grid_rows() {
                txn.set_prop(id, Prop::GridRows(rows.iter().map(grid_track).collect()));
            }
            if let Some(columns) = flex.grid_columns() {
                txn.set_prop(
                    id,
                    Prop::GridColumns(columns.iter().map(grid_track).collect()),
                );
            }
        }

        if let Some(c) = node.constraints() {
            txn.set_prop(id, Prop::SizingH(axis_sizing(c.sizing_h())));
            txn.set_prop(id, Prop::SizingV(axis_sizing(c.sizing_v())));
            // Absent min/max means unconstrained — absence of intent is not a
            // value of intent (P1), so an absent bound stages no prop at all
            // rather than a sentinel.
            if let Some(v) = c.min_width() {
                txn.set_prop(id, Prop::MinWidth(v));
            }
            if let Some(v) = c.max_width() {
                txn.set_prop(id, Prop::MaxWidth(v));
            }
            if let Some(v) = c.min_height() {
                txn.set_prop(id, Prop::MinHeight(v));
            }
            if let Some(v) = c.max_height() {
                txn.set_prop(id, Prop::MaxHeight(v));
            }
            if let Some(m) = c.margin() {
                txn.set_prop(
                    id,
                    Prop::Margin {
                        left: m.left(),
                        top: m.top(),
                        right: m.right(),
                        bottom: m.bottom(),
                    },
                );
            }
            // Grid placement (v0.8, story #43). An absent anchor is
            // auto-placement, so it stages no prop; the spans default
            // to 1 in the schema and in `Layout`, so replaying the
            // value unconditionally is a no-op for old documents.
            if let Some(v) = c.grid_row() {
                txn.set_prop(id, Prop::GridRow(v));
            }
            if let Some(v) = c.grid_column() {
                txn.set_prop(id, Prop::GridColumn(v));
            }
            txn.set_prop(id, Prop::GridRowSpan(c.grid_row_span()));
            txn.set_prop(id, Prop::GridColumnSpan(c.grid_column_span()));
        }

        // v0.8 masks + group opacity (story #44). Each stages only when it
        // differs from the arena default, the same absence-is-not-intent
        // rule as the min/max constraints above — a fully-opaque, unmasked,
        // visible node stages nothing.
        if node.opacity() != 1.0 {
            txn.set_prop(id, Prop::Opacity(node.opacity()));
        }
        if node.mask() {
            txn.set_prop(id, Prop::Mask(true));
        }
        if !node.visible() {
            txn.set_prop(id, Prop::Visible(false));
        }

        // v0.18 rotation (story #770,
        // `docs/decisions/rotation-is-paint-only-and-anchored-explicitly.md`).
        // Same absence-is-not-intent rule: an unrotated node stages nothing.
        //
        // The anchor is part of the test rather than the angle alone. An
        // anchor is meaningless at a zero angle *for drawing*, but it is
        // still the node's stated turning point, and a binding that later
        // drives only the angle reads it back through `Arena::rotation`. A
        // node authored at `(0.0, (w/2, h/2))` that staged nothing would
        // start turning about its top-left the moment that binding fired.
        let anchor = (node.rotation_anchor_x(), node.rotation_anchor_y());
        if node.rotation() != 0.0 || anchor != (0.0, 0.0) {
            txn.set_prop(
                id,
                Prop::Rotation {
                    angle: node.rotation(),
                    anchor,
                },
            );
        }
    }

    // The variant table (v0.4, story #20) replays the same way: each
    // VariantSet becomes an add_variant_set call, and a document that
    // was authored (or last committed) mid-switch replays that switch
    // through set_variant rather than staying pinned to member 0.
    for set in doc.variant_sets().unwrap_or_default().iter() {
        let members = set
            .members()
            .unwrap_or_default()
            .iter()
            .map(|member| VariantMember {
                name: member.name().map(str::to_owned),
                overrides: member
                    .overrides()
                    .unwrap_or_default()
                    .iter()
                    .map(|o| (ids[o.node() as usize], variant_value(&o)))
                    .collect(),
            })
            .collect();
        let id = txn.add_variant_set(members);

        // The v0.18 motion rows (story #771). A member carrying no
        // transition stages none, so a pre-v0.18 document replays exactly as
        // it did — the same absence-is-not-intent rule the node props above
        // follow. Indices resolve through this load's own `ids` mapping,
        // never raw: the arena may already hold nodes from an earlier load.
        for (index, member) in set.members().unwrap_or_default().iter().enumerate() {
            let Some(transition) = member.transition() else {
                continue;
            };
            txn.set_variant_transition(
                id,
                index,
                VariantTransition {
                    tracks: transition
                        .tracks()
                        .unwrap_or_default()
                        .iter()
                        .map(|track| PropTransition {
                            node: ids[track.node() as usize],
                            channel: channel_of(track.channel()),
                            spec: transition_spec!(&track),
                        })
                        .collect(),
                    stagger: transition.stagger(),
                },
            );
        }

        let active = set.active_member() as usize;
        if active != 0 {
            txn.set_variant(id, active);
        }
    }

    // The binding tables (v0.7, story #167) replay through the same
    // producer API: every declaration, then every row, in document
    // order. Indices resolve through this load's own mappings (`ids`,
    // `signal_ids`), never raw — the arena may already hold nodes and
    // signals from an earlier load.
    let signal_ids: Vec<SignalId> = doc
        .signals()
        .unwrap_or_default()
        .iter()
        .map(|signal| txn.declare_signal(signal.name(), signal.initial()))
        .collect();
    for row in doc.bindings().unwrap_or_default().iter() {
        txn.bind(
            ids[row.node() as usize],
            channel_of(row.channel()),
            signal_ids[row.signal() as usize],
            transform_of(&row),
        );
    }

    // Loop tracks (story #772), staged in declaration order like the
    // binding rows above and through the same `ids` mapping. A runtime
    // starts one scheduler track per row when it attaches; nothing here
    // runs them, the same division `bindings` already follows (P3).
    for row in doc.loops().unwrap_or_default().iter() {
        txn.add_loop_track(LoopTrack {
            node: ids[row.node() as usize],
            channel: channel_of(row.channel()),
            from: row.from(),
            to: row.to(),
            spec: transition_spec!(&row),
            phase_offset: row.phase_offset(),
        });
    }

    txn.commit()
}

/// One binding row's channel, converted from the wire enum. An unknown
/// value is `binding.unknown-channel` at the load gate, so it never
/// reaches here (the same posture as the layout enums below).
fn channel_of(channel: dashbuf::BindingChannel) -> Channel {
    Channel::from_code(channel.0).unwrap_or_else(|| {
        unreachable!("unknown BindingChannel {channel:?}: rejected by the load gate (P4)")
    })
}

/// One binding row's transform, converted from the `BindingTransform`
/// union. Union NONE is the identity transform by schema contract.
fn transform_of(row: &dashbuf::Binding<'_>) -> ScalarTransform {
    match row.transform_type() {
        BindingTransform::NONE => ScalarTransform::Identity,
        BindingTransform::TransformScale => ScalarTransform::Scale(
            row.transform_as_transform_scale()
                .expect("TransformScale present")
                .factor(),
        ),
        BindingTransform::TransformMapRange => {
            let m = row
                .transform_as_transform_map_range()
                .expect("TransformMapRange present");
            ScalarTransform::MapRange {
                in_lo: m.in_lo(),
                in_hi: m.in_hi(),
                out_lo: m.out_lo(),
                out_hi: m.out_hi(),
            }
        }
        BindingTransform::TransformClamp => {
            let c = row
                .transform_as_transform_clamp()
                .expect("TransformClamp present");
            ScalarTransform::Clamp {
                lo: c.lo(),
                hi: c.hi(),
            }
        }
        other => unreachable!("unknown BindingTransform {other:?}: rejected by the load gate (P4)"),
    }
}

/// One `VariantOverride`'s value, converted from the `VariantPropValue`
/// union to the arena's narrow `VariantValue` (the same five-prop slice
/// — docs/decisions/variant-set-flat-index.md).
fn variant_value(o: &dashbuf::VariantOverride<'_>) -> VariantValue {
    match o.value_type() {
        VariantPropValue::VariantX => {
            VariantValue::X(o.value_as_variant_x().expect("VariantX present").value())
        }
        VariantPropValue::VariantY => {
            VariantValue::Y(o.value_as_variant_y().expect("VariantY present").value())
        }
        VariantPropValue::VariantWidth => VariantValue::Width(
            o.value_as_variant_width()
                .expect("VariantWidth present")
                .value(),
        ),
        VariantPropValue::VariantHeight => VariantValue::Height(
            o.value_as_variant_height()
                .expect("VariantHeight present")
                .value(),
        ),
        VariantPropValue::VariantFill => VariantValue::Fill(color_of(
            o.value_as_variant_fill()
                .expect("VariantFill present")
                .color(),
        )),
        VariantPropValue::VariantVisible => VariantValue::Visible(
            o.value_as_variant_visible()
                .expect("VariantVisible present")
                .value(),
        ),
        VariantPropValue::VariantRotation => {
            let r = o
                .value_as_variant_rotation()
                .expect("VariantRotation present");
            VariantValue::Rotation {
                angle: r.angle(),
                anchor: (r.anchor_x(), r.anchor_y()),
            }
        }
        other => unreachable!("unknown VariantPropValue {other:?}: rejected by the load gate (P4)"),
    }
}

/// One easing curve, the schema enum to the arena's mirror of it.
fn easing_of(easing: dashbuf::Easing) -> Easing {
    match easing {
        dashbuf::Easing::Linear => Easing::Linear,
        dashbuf::Easing::EaseIn => Easing::EaseIn,
        dashbuf::Easing::EaseOut => Easing::EaseOut,
        dashbuf::Easing::EaseInOut => Easing::EaseInOut,
        other => unreachable!("unknown Easing {other:?}: rejected by the load gate (P4)"),
    }
}

/// One document gradient's handles and kind, with its stops left to
/// [`gradient_stops_of`] — the split story #578 introduced, since a
/// `Gradient` now names its stops by a range the paint table assigns.
fn gradient_of(g: &dashbuf::Gradient<'_>) -> Gradient {
    Gradient {
        kind: gradient_kind(g.kind()),
        handle_origin: vec2_of(g.handle_origin()),
        handle_primary: vec2_of(g.handle_primary()),
        handle_secondary: vec2_of(g.handle_secondary()),
        stops: dashpaint::StopRange::NONE,
    }
}

/// One document gradient's stops, owned, in document order.
fn gradient_stops_of(g: &dashbuf::Gradient<'_>) -> Vec<GradientStop> {
    g.stops()
        .iter()
        .map(|s| GradientStop {
            offset: s.offset(),
            color: color_of(s.color()),
        })
        .collect()
}

/// One document image fill. `transform` carries [`Mat23::IDENTITY`] where
/// the document names none — story #578 removed the `Option`, and identity
/// is what its `None` meant.
fn image_fill_of(f: &dashbuf::ImageFill<'_>, image_of: &[u32]) -> dashpaint::ImageFill {
    dashpaint::ImageFill {
        // Through the mapping, never the document's own index.
        image: image_of[f.image() as usize],
        scale_mode: scale_mode(f.scale_mode()),
        transform: f.transform().map(mat23_of).unwrap_or(Mat23::IDENTITY),
        tile_scale: f.tile_scale(),
    }
}

/// The `Fill` union as its carriers expose it. `flatc` names a union's
/// accessors after the field, so a stacked layer's `fill_as_gradient` and a
/// placeholder's `interim_fill_as_gradient` are different methods over the
/// same union — this trait is what lets one reader serve both, the same shape
/// `dashscene_validator`'s own `FillUnion` uses to hold every carrier to one
/// set of rules.
trait FillUnion<'a> {
    fn fill_type(&self) -> Fill;
    fn fill_as_solid_fill(&self) -> Option<dashbuf::SolidFill<'a>>;
    fn fill_as_gradient(&self) -> Option<dashbuf::Gradient<'a>>;
    fn fill_as_image_fill(&self) -> Option<dashbuf::ImageFill<'a>>;
}

impl<'a> FillUnion<'a> for dashbuf::FillLayer<'a> {
    fn fill_type(&self) -> Fill {
        dashbuf::FillLayer::fill_type(self)
    }
    fn fill_as_solid_fill(&self) -> Option<dashbuf::SolidFill<'a>> {
        dashbuf::FillLayer::fill_as_solid_fill(self)
    }
    fn fill_as_gradient(&self) -> Option<dashbuf::Gradient<'a>> {
        dashbuf::FillLayer::fill_as_gradient(self)
    }
    fn fill_as_image_fill(&self) -> Option<dashbuf::ImageFill<'a>> {
        dashbuf::FillLayer::fill_as_image_fill(self)
    }
}

impl<'a> FillUnion<'a> for dashbuf::Placeholder<'a> {
    fn fill_type(&self) -> Fill {
        dashbuf::Placeholder::interim_fill_type(self)
    }
    fn fill_as_solid_fill(&self) -> Option<dashbuf::SolidFill<'a>> {
        dashbuf::Placeholder::interim_fill_as_solid_fill(self)
    }
    fn fill_as_gradient(&self) -> Option<dashbuf::Gradient<'a>> {
        dashbuf::Placeholder::interim_fill_as_gradient(self)
    }
    fn fill_as_image_fill(&self) -> Option<dashbuf::ImageFill<'a>> {
        dashbuf::Placeholder::interim_fill_as_image_fill(self)
    }
}

/// One `Fill` union, as the spec a producer staged. Serves a stacked
/// `FillLayer` (story C1, debt #146) and a placeholder's `interim_fill`
/// (story #1126) alike.
///
/// `None` for `Fill::NONE`, which means different things to the two carriers
/// and is dropped the same way for both: a stacked layer with no fill is
/// malformed, where a placeholder with no interim fill is the ordinary case —
/// the schema's own default, a box that reserves space and shows nothing
/// while it waits. Neither is diagnosed here; this assumes a validated
/// document (P4), the same contract as the rest of this module.
fn fill_spec_of<'a>(fill: &impl FillUnion<'a>, image_of: &[u32]) -> Option<FillSpec> {
    match fill.fill_type() {
        Fill::SolidFill => fill
            .fill_as_solid_fill()
            .and_then(|s| s.color())
            .map(|c| FillSpec::Solid { color: color_of(c) }),
        Fill::Gradient => fill.fill_as_gradient().map(|g| FillSpec::Gradient {
            gradient: gradient_of(&g),
            stops: gradient_stops_of(&g),
        }),
        Fill::ImageFill => fill
            .fill_as_image_fill()
            .map(|f| FillSpec::Image(image_fill_of(&f, image_of))),
        _ => None,
    }
}

/// One pool entry's fill, stroke, corners, clip, and baked-vector shape,
/// staged onto `id`.
fn load_paint(
    txn: &mut crate::arena::Txn<'_>,
    id: NodeId,
    paint: &dashbuf::Paint<'_>,
    image_of: &[u32],
    shape_of: &[VectorField],
) {
    match paint.fill_type() {
        Fill::SolidFill => {
            if let Some(solid) = paint.fill_as_solid_fill()
                && let Some(color) = solid.color()
            {
                txn.set_prop(id, Prop::Fill(color_of(color)));
            }
        }
        Fill::Gradient => {
            if let Some(g) = paint.fill_as_gradient() {
                txn.set_prop(
                    id,
                    Prop::FillWith(FillSpec::Gradient {
                        gradient: gradient_of(&g),
                        stops: gradient_stops_of(&g),
                    }),
                );
            }
        }
        Fill::ImageFill => {
            if let Some(f) = paint.fill_as_image_fill() {
                txn.set_prop(
                    id,
                    Prop::FillWith(FillSpec::Image(image_fill_of(&f, image_of))),
                );
            }
        }
        // A pool entry with no fill is a stroke-only or clip-only entry — a
        // legitimate shape, not a missing one.
        _ => {}
    }

    // Stacked fills (story C1, debt #146): every layer above the bottom
    // fill, in the same array order it paints. Absent (the pre-C1 default)
    // means a single fill, so an old document stages nothing here — matching
    // `PaintEntry::extra_fills`'s empty default.
    if let Some(layers) = paint.extra_fills()
        && !layers.is_empty()
    {
        txn.set_prop(
            id,
            Prop::ExtraFills(
                layers
                    .iter()
                    .filter_map(|layer| fill_spec_of(&layer, image_of))
                    .collect(),
            ),
        );
    }

    if let Some(s) = paint.stroke() {
        // `Stroke.color` is `(required)` in the schema, so the accessor is
        // not an Option.
        txn.set_prop(
            id,
            Prop::Stroke(Stroke {
                width: s.width(),
                align: stroke_align(s.align()),
                color: color_of(s.color()),
            }),
        );
    }

    if let Some(c) = paint.corners() {
        txn.set_prop(
            id,
            Prop::Corners {
                top_left: c.top_left(),
                top_right: c.top_right(),
                bottom_right: c.bottom_right(),
                bottom_left: c.bottom_left(),
            },
        );
    }

    // v0.8 shadows (story #45). Absent means none; the prop replaces the
    // whole list, so an empty vector would clear it — set the prop only
    // when the document carries a non-empty list, matching the corners
    // and stroke omissions above.
    if let Some(shadows) = paint.shadows()
        && !shadows.is_empty()
    {
        txn.set_prop(
            id,
            Prop::Shadows(
                shadows
                    .iter()
                    .map(|s| Shadow {
                        kind: shadow_kind(s.kind()),
                        // An absent `offset` struct is a zero (centered)
                        // shadow — Figma always writes one, but the schema
                        // leaves the struct optional.
                        offset: s.offset().map_or(Vec2 { x: 0.0, y: 0.0 }, vec2_of),
                        blur: s.blur(),
                        spread: s.spread(),
                        // `Shadow.color` is `(required)`, so the accessor
                        // is not an Option (like `Stroke.color`).
                        color: color_of(s.color()),
                    })
                    .collect(),
            ),
        );
    }

    // v0.11 blurs (story #393). Same shape as the shadows above: absent means
    // none, and the prop replaces the whole list, so it is set only when the
    // document carries a non-empty one. Loading it matters even before a
    // painter draws it — a schema field the loader ignored would drop the
    // node's blur silently, which is exactly what P4 forbids.
    if let Some(blurs) = paint.blurs()
        && !blurs.is_empty()
    {
        txn.set_prop(
            id,
            Prop::Blurs(
                blurs
                    .iter()
                    .map(|b| Blur {
                        kind: blur_kind(b.kind()),
                        radius: b.radius(),
                    })
                    .collect(),
            ),
        );
    }

    // The document pools clip with the paint entry; the arena carries it as
    // node intent (issue #97). Two nodes sharing a style but differing in
    // clip therefore need two pool entries in the document, which is what
    // the emitter's pool key accounts for.
    if paint.clip() {
        txn.set_prop(id, Prop::Clip(true));
    }

    // The baked-vector shape channel (story B1). `NO_FIELD` is the implicit
    // parametric shape, so an old document (which carries no shape field)
    // stages nothing here and loads unchanged. A valid index resolves to the
    // pre-built `VectorField` and stages it as paint intent.
    if paint.shape_field() != NO_FIELD {
        txn.set_prop(id, Prop::ShapeField(shape_of[paint.shape_field() as usize]));
    }
}

fn color_of(c: &dashbuf::Color) -> Color {
    Color {
        r: c.r(),
        g: c.g(),
        b: c.b(),
        a: c.a(),
    }
}

fn vec2_of(v: &dashbuf::Vec2) -> Vec2 {
    Vec2 { x: v.x(), y: v.y() }
}

fn mat23_of(m: &dashbuf::Mat23) -> Mat23 {
    Mat23 {
        a: m.a(),
        b: m.b(),
        c: m.c(),
        d: m.d(),
        tx: m.tx(),
        ty: m.ty(),
    }
}

// The enum maps are exhaustive over the values this build knows. A value it
// does not know is `vocabulary.unknown-enum` at the load gate, so it never
// reaches here — the wildcard arm exists because `flatc` models an
// append-only enum as a newtype over `u8`, which has no exhaustive match.

fn image_format(f: dashbuf::ImageFormat) -> ImageFormat {
    match f {
        dashbuf::ImageFormat::Png => ImageFormat::Png,
        dashbuf::ImageFormat::Jpeg => ImageFormat::Jpeg,
        dashbuf::ImageFormat::Gif => ImageFormat::Gif,
        other => unreachable!("unknown ImageFormat {other:?}: rejected by the load gate (P4)"),
    }
}

fn gradient_kind(k: dashbuf::GradientKind) -> GradientKind {
    match k {
        dashbuf::GradientKind::Linear => GradientKind::Linear,
        dashbuf::GradientKind::Radial => GradientKind::Radial,
        dashbuf::GradientKind::Angular => GradientKind::Angular,
        dashbuf::GradientKind::Diamond => GradientKind::Diamond,
        other => unreachable!("unknown GradientKind {other:?}: rejected by the load gate (P4)"),
    }
}

fn scale_mode(m: dashbuf::ScaleMode) -> ScaleMode {
    match m {
        dashbuf::ScaleMode::Fill => ScaleMode::Fill,
        dashbuf::ScaleMode::Fit => ScaleMode::Fit,
        dashbuf::ScaleMode::Crop => ScaleMode::Crop,
        dashbuf::ScaleMode::Tile => ScaleMode::Tile,
        other => unreachable!("unknown ScaleMode {other:?}: rejected by the load gate (P4)"),
    }
}

fn stroke_align(a: dashbuf::StrokeAlign) -> StrokeAlign {
    match a {
        dashbuf::StrokeAlign::Inside => StrokeAlign::Inside,
        dashbuf::StrokeAlign::Center => StrokeAlign::Center,
        dashbuf::StrokeAlign::Outside => StrokeAlign::Outside,
        other => unreachable!("unknown StrokeAlign {other:?}: rejected by the load gate (P4)"),
    }
}

/// The document's blur kind as the arena's (story #393).
///
/// The catch-all is `unreachable!`, not a coercion to `Backdrop`, and the
/// difference matters: the generated binding is a newtype over `u8` whose
/// verifier does no range check, so an out-of-range discriminant does reach
/// here. Coercing it would turn a future kind — a progressive blur, say —
/// into a backdrop blur, and `PaintEntry::samples_backdrop` would then impose
/// a painter ordering barrier for an effect that needs none. That is a silent
/// semantic substitution, which is exactly what P4 forbids. The load gate
/// range-checks `Blur.kind` before this runs, so reaching the arm means the
/// gate was bypassed. Same shape as [`shadow_kind`] below.
fn blur_kind(k: dashbuf::BlurKind) -> BlurKind {
    match k {
        dashbuf::BlurKind::Layer => BlurKind::Layer,
        dashbuf::BlurKind::Backdrop => BlurKind::Backdrop,
        other => unreachable!("unknown BlurKind {other:?}: rejected by the load gate (P4)"),
    }
}

fn shadow_kind(k: dashbuf::ShadowKind) -> ShadowKind {
    match k {
        dashbuf::ShadowKind::Drop => ShadowKind::Drop,
        dashbuf::ShadowKind::Inner => ShadowKind::Inner,
        other => unreachable!("unknown ShadowKind {other:?}: rejected by the load gate (P4)"),
    }
}

fn layout_mode(m: dashbuf::LayoutMode) -> LayoutMode {
    match m {
        dashbuf::LayoutMode::None => LayoutMode::None,
        dashbuf::LayoutMode::Horizontal => LayoutMode::Horizontal,
        dashbuf::LayoutMode::Vertical => LayoutMode::Vertical,
        dashbuf::LayoutMode::Wrap => LayoutMode::Wrap,
        dashbuf::LayoutMode::Grid => LayoutMode::Grid,
        other => unreachable!("unknown LayoutMode {other:?}: rejected by the load gate (P4)"),
    }
}

fn main_align(a: dashbuf::MainAxisAlign) -> MainAxisAlign {
    match a {
        dashbuf::MainAxisAlign::Start => MainAxisAlign::Start,
        dashbuf::MainAxisAlign::Center => MainAxisAlign::Center,
        dashbuf::MainAxisAlign::End => MainAxisAlign::End,
        dashbuf::MainAxisAlign::SpaceBetween => MainAxisAlign::SpaceBetween,
        other => unreachable!("unknown MainAxisAlign {other:?}: rejected by the load gate (P4)"),
    }
}

fn cross_align(a: dashbuf::CrossAxisAlign) -> CrossAxisAlign {
    match a {
        dashbuf::CrossAxisAlign::Start => CrossAxisAlign::Start,
        dashbuf::CrossAxisAlign::Center => CrossAxisAlign::Center,
        dashbuf::CrossAxisAlign::End => CrossAxisAlign::End,
        dashbuf::CrossAxisAlign::Baseline => CrossAxisAlign::Baseline,
        other => unreachable!("unknown CrossAxisAlign {other:?}: rejected by the load gate (P4)"),
    }
}

fn text_align_of(a: dashbuf::TextAlign) -> TextAlign {
    match a {
        dashbuf::TextAlign::Left => TextAlign::Left,
        dashbuf::TextAlign::Center => TextAlign::Center,
        dashbuf::TextAlign::Right => TextAlign::Right,
        other => unreachable!("unknown TextAlign {other:?}: rejected by the load gate (P4)"),
    }
}

fn text_align_v_of(a: dashbuf::TextAlignV) -> TextAlignV {
    match a {
        dashbuf::TextAlignV::Top => TextAlignV::Top,
        dashbuf::TextAlignV::Center => TextAlignV::Center,
        dashbuf::TextAlignV::Bottom => TextAlignV::Bottom,
        other => unreachable!("unknown TextAlignV {other:?}: rejected by the load gate (P4)"),
    }
}

fn grid_track(t: dashbuf::GridTrack<'_>) -> GridTrack {
    match t.sizing() {
        dashbuf::GridTrackSizing::Fixed => GridTrack::Fixed(t.value()),
        dashbuf::GridTrackSizing::Fraction => GridTrack::Fraction(t.value()),
        other => unreachable!("unknown GridTrackSizing {other:?}: rejected by the load gate (P4)"),
    }
}

fn axis_sizing(s: dashbuf::AxisSizing) -> AxisSizing {
    match s {
        dashbuf::AxisSizing::Fixed => AxisSizing::Fixed,
        dashbuf::AxisSizing::Hug => AxisSizing::Hug,
        dashbuf::AxisSizing::Fill => AxisSizing::Fill,
        other => unreachable!("unknown AxisSizing {other:?}: rejected by the load gate (P4)"),
    }
}
