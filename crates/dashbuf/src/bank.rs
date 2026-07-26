//! The cold bank — the payloads one quality profile binds a document's
//! canonical asset hashes to — and the assembly that turns one document plus
//! one bank into a `.dsb` file.
//!
//! Specified by `docs/decisions/asset-model-content-addressed-blobs.md` (what a
//! canonical hash means and what a binding is) and laid out by
//! `docs/design/dsb-container-format.md` (where the bytes go).
//!
//! # What a bank is
//!
//! An asset has one canonical payload and one canonical hash. A **profile**
//! binds that hash to the bytes a file actually carries. RAW is the **null
//! binding** — the identity map — so under RAW the resident payload *is* the
//! canonical payload. A production profile binds the same canonical hash to a
//! payload a packer derived from it, and only the binding changes.
//!
//! A [`ColdBank`] is one profile's side of that binding for one document: the
//! payloads, each keyed by the canonical hash it stands for. [`assemble`] puts
//! them in the file.
//!
//! # Why assembly reads the document
//!
//! [`assemble`] takes the ui section as bytes and reads the asset entries out
//! of the very section it is about to write. That is what makes the ui section
//! an **input** to assembly and never an output, and it is why hot sections are
//! byte-identical across assemblies of one document: nothing in the assembly
//! path can write into them. The alternative — pairing a caller-supplied
//! payload list positionally against the entries — makes the same guarantee
//! only as long as the caller keeps the two lists in the same order.
//!
//! An `AssetEntry` names a hash and never a section index
//! (`docs/decisions/asset-model-content-addressed-blobs.md`), so the resolution
//! here is a hash lookup, not an index. [`crate::open`] is the read-side
//! inverse: it resolves the same entry hashes back to the same blob sections.
//!
//! # The derivation manifest
//!
//! Under a derived binding an entry's canonical hash is not the hash of any
//! payload in the file, so a reader cannot find the payload by that hash alone.
//! The mapping that closes the gap is the **derivation manifest**: a
//! `dashbuf::AssetBindings` flatbuffer in its own section, carrying one
//! canonical-to-resident hash pair per non-identity binding
//! (`docs/decisions/derivation-manifest-section.md`).
//!
//! Assembly emits it, and derives it from the bank rather than taking it as a
//! second argument: a row is needed exactly where `blake3(resident)` differs
//! from the canonical hash, which the bank alone already determines. RAW is the
//! identity map, so a RAW assembly produces no rows and writes no manifest
//! section at all — which is why the committed goldens did not move when this
//! landed.
//!
//! # This module parses; [`crate::container`] does not
//!
//! `container` is deliberately parser-free — it exists to validate a file
//! before any parser is trusted, so it cannot depend on one. That constraint is
//! about *reading* an untrusted file. Assembly is a writer, running on bytes
//! its caller just produced, so it is free to use the schema, and it lives here
//! rather than in `container` to keep that boundary where it is.

use std::error::Error;
use std::fmt;

use crate::container::{
    self, FLAVOR_ASSET, FLAVOR_BINDINGS, FLAVOR_UI, HASH_LEN, Section, WriteError,
};
use crate::{AssetBinding, AssetBindingArgs, AssetBindings, AssetBindingsArgs, root_as_document};

/// The payloads one profile binds a document's canonical asset hashes to.
///
/// Borrowing, not owning: assembly copies each payload into the output exactly
/// once, so a bank over memory-mapped or already-loaded bytes needs no second
/// copy.
#[derive(Debug, Clone)]
pub struct ColdBank<'a> {
    /// Canonical hash, and the payload this profile binds it to.
    ///
    /// A `Vec` rather than a map, which makes assembly O(assets squared): one
    /// linear scan per entry to resolve it, and one per binding to find the
    /// unnamed ones. That is deliberate, and measured rather than assumed —
    /// release build, hashes already computed, so this is 32-byte comparisons
    /// and no hashing:
    ///
    /// | assets | resolve + unused-check |
    /// | ------ | ---------------------- |
    /// | 150    | 13 us                  |
    /// | 1000   | 0.5 ms                 |
    /// | 5000   | 12.5 ms                |
    ///
    /// A compile step reaching 5000 assets in one document would pay 12.5 ms
    /// here, against the BLAKE3 and image-decode work the same document already
    /// costs. A map would win only past that, and it would have to keep this
    /// vector anyway: blob section order follows entry order and R7 covers the
    /// whole file, so iteration order cannot come from a hash map
    /// (the constraint `dashc` debt #418 records for the same reason).
    bindings: Vec<([u8; HASH_LEN], &'a [u8])>,
}

impl<'a> ColdBank<'a> {
    /// **RAW — the null binding.** Each payload bound to its own hash.
    ///
    /// This one line is the whole of RAW: the identity map, so the resident
    /// payload is the canonical payload and the file carries the imported bytes
    /// unchanged. It is what makes a RAW assembly checkable in the strongest
    /// form available — nothing is derived, so nothing may move.
    ///
    /// RAW is not a shipping profile. It is the qualification baseline, the
    /// oracle lane, and the developer preview
    /// (`docs/decisions/asset-quality-profile-naming.md`).
    pub fn raw<I>(payloads: I) -> Self
    where
        I: IntoIterator<Item = &'a [u8]>,
    {
        Self::derived(
            payloads
                .into_iter()
                .map(|payload| (blake3::hash(payload).into(), payload)),
        )
    }

    /// A binding that is not the identity map: each canonical hash paired with
    /// the payload a profile derived from it.
    ///
    /// The general form — [`ColdBank::raw`] is this one with the identity map.
    /// The pairing is the packer's per-asset choice, and [`assemble`] records
    /// it in the file's derivation manifest — which is what lets a reader
    /// resolve a canonical hash to bytes that are not its own preimage.
    ///
    /// Nothing here has to say which pairs are derived and which are the
    /// identity: `blake3(payload) == canonical` answers that per binding, so a
    /// bank built with this constructor over canonical payloads is exactly
    /// [`ColdBank::raw`] and assembles to the same bytes.
    pub fn derived<I>(bindings: I) -> Self
    where
        I: IntoIterator<Item = ([u8; HASH_LEN], &'a [u8])>,
    {
        Self {
            bindings: bindings.into_iter().collect(),
        }
    }

    /// How many payloads the bank holds.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Whether the bank binds nothing — the state of every document with no
    /// assets, which is six of the seven committed `.dsb` goldens.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// The position of the binding for `canonical`, if the bank has one.
    ///
    /// The **first** such binding. A bank is conceptually a map from canonical
    /// hash to payload, so two bindings under one hash are either redundant
    /// (the same payload twice, which resolves identically either way) or
    /// contradictory. [`ColdBank::contradiction`] refuses the second case
    /// before this is ever called, so "the first" is only ever a choice between
    /// bindings that are the same bytes.
    fn position_of(&self, canonical: &[u8]) -> Option<usize> {
        self.bindings
            .iter()
            .position(|(hash, _)| hash.as_slice() == canonical)
    }

    /// The first canonical hash this bank binds to two payloads that are not
    /// the same bytes, if it has one.
    ///
    /// Two payloads claiming one canonical identity is a manifest that
    /// disagrees with itself: the file could carry only one of them under that
    /// identity, so writing it would silently discard the other's claim. Under
    /// [`ColdBank::raw`] this is unreachable by construction — every payload is
    /// bound to its own hash, so two bindings sharing a hash are two identical
    /// payloads — which is why it is checked here and not on the read side.
    ///
    /// The same O(bindings squared) shape as [`ColdBank::position_of`], and the
    /// same 32-byte comparisons: the payload bytes are compared only when two
    /// canonical hashes already match.
    fn contradiction(&self) -> Option<[u8; HASH_LEN]> {
        self.bindings
            .iter()
            .enumerate()
            .find_map(|(at, (canonical, payload))| {
                self.bindings[at + 1..]
                    .iter()
                    .any(|(other, other_payload)| other == canonical && other_payload != payload)
                    .then_some(*canonical)
            })
    }
}

/// Why a document and a bank cannot be assembled into a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssembleError {
    /// The ui section is not a structurally valid `Document`, so its asset
    /// entries — which say what the file must carry — cannot be read.
    Document(flatbuffers::InvalidFlatbuffer),
    /// The bank binds no payload to the canonical hash asset entry `index`
    /// names. Assembling anyway would write a file whose own asset table points
    /// at a payload it does not carry, which fails at load with
    /// [`crate::container::ContainerError::NoBlobForHash`] instead of here.
    Unbound { index: usize },
    /// The bank holds payloads no asset entry names. They would become cold
    /// bytes nothing in the file can reach, because a payload is found by the
    /// hash an entry carries and nothing else — a silent size regression rather
    /// than a broken file, which is why it is refused rather than trimmed.
    UnusedPayloads { count: usize },
    /// The bank binds this canonical hash to two payloads that are not the same
    /// bytes. One file cannot carry both under one identity, so assembling
    /// would silently drop one claim — which P4 forbids. Only a derived bank
    /// can express it: under the null binding a payload is bound to its own
    /// hash, so two bindings sharing a hash are the same bytes.
    ContradictoryBinding { canonical: [u8; HASH_LEN] },
    /// The section set the assembly produced is not writable. Reachable only
    /// through an empty payload or a section count past `u32`; the hot-before-
    /// cold order is this function's own construction.
    Write(WriteError),
}

impl fmt::Display for AssembleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(error) => {
                write!(f, "the ui section is not a valid document: {error}")
            }
            Self::Unbound { index } => write!(
                f,
                "the bank binds no payload to the content hash asset entry {index} names"
            ),
            Self::UnusedPayloads { count } => write!(
                f,
                "the bank holds {count} payload(s) no asset entry names; nothing in the \
                 assembled file could reach them"
            ),
            Self::ContradictoryBinding { canonical } => write!(
                f,
                "the bank binds canonical hash {} to two different payloads; one file \
                 cannot carry both under one identity",
                blake3::Hash::from_bytes(*canonical).to_hex()
            ),
            Self::Write(error) => write!(f, "{error}"),
        }
    }
}

impl Error for AssembleError {}

impl From<flatbuffers::InvalidFlatbuffer> for AssembleError {
    fn from(error: flatbuffers::InvalidFlatbuffer) -> Self {
        Self::Document(error)
    }
}

impl From<WriteError> for AssembleError {
    fn from(error: WriteError) -> Self {
        Self::Write(error)
    }
}

/// Assembles one document and one cold bank into `.dsb` file bytes.
///
/// The shipped shape (`docs/decisions/dsb-sectioned-container.md`): the hot
/// sections at the head, the chosen profile's payloads page-aligned in cold
/// sections at the tail, one mmap of the whole file, and untouched cold pages
/// that never fault.
///
/// The ui document, then the derivation manifest when the bank needs one, then
/// one blob section per asset entry, in entry order. The alignment, the
/// page-aligned hot/cold boundary, and the zero-filled gaps are
/// [`container::write`]'s; what this adds is the resolution from what the
/// document names to what the file carries, and the manifest that records it.
///
/// Blob order is entry order rather than, for example, hash order, because
/// entry order is the order the producer minted the entries in, and a
/// re-ordering here would move bytes for no reader's benefit — every reader
/// looks payloads up by hash.
///
/// Two assemblies of one document under different banks produce files whose ui
/// **section bytes** are identical, and whose differences lie entirely in the
/// envelope (the header's root hash and the section table), in the manifest
/// section, and in the cold payload bytes. The ui section's *offset* is not
/// part of that guarantee: a derived bank adds a manifest entry to the section
/// table, so the payloads start one 64-byte stride later than under RAW. What
/// the document promises is its content, not its address — an `AssetEntry`
/// names a hash and never an offset, so no reader depends on where the ui
/// section sits.
///
/// The output is byte-reproducible for a given document and bank (R7): every
/// step below is a pure function of its input.
pub fn assemble(ui_section: &[u8], bank: &ColdBank<'_>) -> Result<Vec<u8>, AssembleError> {
    // A property of the bank alone, so it is settled before the document is
    // even parsed: no document makes a self-contradicting bank assemblable.
    if let Some(canonical) = bank.contradiction() {
        return Err(AssembleError::ContradictoryBinding { canonical });
    }

    let document = root_as_document(ui_section)?;
    let entries = document.assets().unwrap_or_default();

    // Every entry resolved before any section is built. The manifest is itself
    // a section and sits ahead of the blobs, so it cannot be written until
    // every resident payload is known.
    let mut resolved = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let at = bank
            .position_of(entry.hash().bytes())
            .ok_or(AssembleError::Unbound { index })?;
        resolved.push(at);
    }

    // A binding is used when some entry names its hash.
    //
    // Counted over hashes rather than over the positions resolution picked,
    // which is not the same thing when two bindings share a canonical hash:
    // `position_of` always returns the first, so a position-based count would
    // report the second as unnamed when its hash is in fact named, and refuse a
    // document that is fine. `dashc` turns an error here into a panic through an
    // `expect`, so that miscount would have been a panic on input the previous
    // inline writer accepted — a named condition becoming a crash, which P4
    // forbids.
    let unused = bank
        .bindings
        .iter()
        .filter(|(canonical, _)| {
            !entries
                .iter()
                .any(|entry| entry.hash().bytes() == canonical.as_slice())
        })
        .count();
    if unused != 0 {
        return Err(AssembleError::UnusedPayloads { count: unused });
    }

    let manifest = manifest(bank, &resolved);

    let mut sections = Vec::with_capacity(2 + entries.len());
    sections.push(Section::structured(FLAVOR_UI, ui_section));
    if let Some(bytes) = &manifest {
        sections.push(Section::structured(FLAVOR_BINDINGS, bytes));
    }
    for &at in &resolved {
        sections.push(Section::blob(FLAVOR_ASSET, bank.bindings[at].1));
    }

    Ok(container::write(&sections)?)
}

/// The derivation manifest for the bindings `resolved` names, or `None` when
/// every one of them is the identity map.
///
/// One row per *distinct* canonical hash whose resident payload is not its own
/// preimage, in entry order. Identity bindings are left out because a reader
/// resolves them by the canonical hash directly — writing them would be bytes
/// that change no answer, and it would make a RAW file differ from the RAW
/// files already committed.
///
/// Row order is entry order and not, say, sorted hash order: it matches blob
/// order, and it is already a pure function of the input, which is all R7 asks.
fn manifest(bank: &ColdBank<'_>, resolved: &[usize]) -> Option<Vec<u8>> {
    let mut rows: Vec<([u8; HASH_LEN], [u8; HASH_LEN])> = Vec::new();
    for &at in resolved {
        let (canonical, payload) = bank.bindings[at];
        let resident: [u8; HASH_LEN] = blake3::hash(payload).into();
        // The identity map needs no row, and one canonical hash needs no second
        // row: two entries naming one asset resolve through the same binding.
        if resident == canonical || rows.iter().any(|(seen, _)| *seen == canonical) {
            continue;
        }
        rows.push((canonical, resident));
    }
    if rows.is_empty() {
        return None;
    }

    let mut builder = flatbuffers::FlatBufferBuilder::new();
    let bindings: Vec<_> = rows
        .iter()
        .map(|(canonical, resident)| {
            let canonical = builder.create_vector(canonical);
            let resident = builder.create_vector(resident);
            AssetBinding::create(
                &mut builder,
                &AssetBindingArgs {
                    canonical: Some(canonical),
                    resident: Some(resident),
                },
            )
        })
        .collect();
    let bindings = builder.create_vector(&bindings);
    let manifest = AssetBindings::create(
        &mut builder,
        &AssetBindingsArgs {
            bindings: Some(bindings),
        },
    );
    builder.finish(manifest, None);
    Some(builder.finished_data().to_vec())
}
