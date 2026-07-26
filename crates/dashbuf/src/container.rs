//! The `.dsb` sectioned container — the envelope that carries flatbuffer
//! sections and raw payload blobs in one file.
//!
//! Specified by `docs/decisions/dsb-sectioned-container.md` and laid out byte
//! for byte in `docs/design/dsb-container-format.md`. Read the format doc
//! before changing anything here: the layout is frozen and evolves by version
//! bump, not by adding fields.
//!
//! # Why this is not a flatbuffer
//!
//! The envelope is validated *before* any parser is trusted, so checking it
//! must be plain bounds, magic, and version comparisons at fixed offsets. A
//! flatbuffer — even a struct, which cannot be a `root_type` — would pull
//! root-table framing and the verifier into the one component that exists to
//! stand outside them.
//!
//! # Three rules this module enforces rather than documents
//!
//! - **Explicit little-endian.** Every multi-byte field is read and written
//!   through `from_le_bytes`/`to_le_bytes`. The layout structs below are never
//!   reinterpreted from raw memory. Every target this repo builds for
//!   (`x86_64`, `aarch64`, `wasm32`) is little-endian, so a native cast would
//!   pass every test and still be wrong — no test can enforce this, which is
//!   why it is structural.
//! - **No implicit padding, and the struct is the layout.** Each struct's size
//!   and every field offset are pinned by a compile-time assertion against the
//!   number in the format doc, and `encode`/`decode` derive their byte ranges
//!   from `offset_of!` rather than repeating those numbers. A reordering that
//!   introduces a compiler-inserted gap therefore fails to build, and a
//!   reordering that does not still cannot silently move a field in the file
//!   while both directions agree with each other.
//! - **Deterministic bytes.** [`write`] is a pure function of its input: fixed
//!   field order, zero-filled alignment gaps, and content hashes that depend on
//!   content alone. R7 ("same input, byte-identical document") applies to the
//!   envelope exactly as it applies to the flatbuffer inside it.
//!
//! # Reading
//!
//! [`Container::parse`] borrows; it never copies a payload and never allocates
//! one. An `mmap` of the file is therefore a drop-in, which is what the R5
//! loading model ("one mmap of the whole file, once") needs. Parsing validates
//! the header, the section table, and the table's own hash; payload hashes are
//! checked on demand by [`Container::verify_section`] so that a caller
//! verifying only the hot sections never faults a cold page.

use std::error::Error;
use std::fmt;
use std::mem::{offset_of, size_of};

/// The file signature.
///
/// Built like PNG's rather than as a bare `"DSB1"`: the high bit in byte 0
/// catches a transport that strips to seven bits, and the `\r\n` / `\n` pair
/// catches one that translates line endings. It still reads as `DSB` in a hex
/// dump, which is the property the container decision asked for.
pub const MAGIC: [u8; 8] = [0x89, b'D', b'S', b'B', 0x0D, 0x0A, 0x1A, 0x0A];

/// The envelope format version. Bumped as a whole; there is no field-id rule.
pub const FORMAT_VERSION: u16 = 1;

/// Bytes in the header.
pub const HEADER_SIZE: usize = 64;

/// Bytes per section-table entry.
///
/// Recorded in the header alongside [`HEADER_SIZE`] so the table is
/// self-describing: an external tool — the signing tool the container decision
/// names — walks the table and computes the signed range
/// `header_size + section_count * section_stride` without hardcoding either
/// number. A reader of this version does not use the recorded value to skip a
/// grown entry, because the envelope evolves by version bump and a version it
/// does not implement is refused whole; what the recorded value buys a reader
/// is that a stride mismatch is a named error rather than a misparse.
pub const SECTION_STRIDE: usize = 64;

/// Bytes in a content hash (BLAKE3-256).
pub const HASH_LEN: usize = 32;

/// The universal small alignment quantum. Every section starts on it, so a
/// pointer into the mapping satisfies any consumer's natural alignment.
pub const SECTION_ALIGN: usize = 64;

/// The page quantum used for the hot/cold boundary and for large blobs.
pub const PAGE_ALIGN: usize = 4096;

/// At or above this size a blob is page-aligned, so it can be prefetched and
/// evicted on its own. Below it, blobs pack densely: verification and
/// readiness are per blob, so two small blobs sharing a page is harmless.
pub const LARGE_BLOB_THRESHOLD: usize = 64 * 1024;

/// What byte-language a section is written in.
///
/// The file holds exactly three: this envelope, ordinary flatbuffers
/// ([`SectionKind::Structured`]), and raw well-known payload formats
/// ([`SectionKind::Blob`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    /// A complete flatbuffer with its own root type. Verified with the stock
    /// flatbuffers verifier by the caller, after its hash check.
    Structured = 1,
    /// Raw payload bytes with no dashscene framing — a PNG, a JPEG, a KTX2.
    /// Verified by hash only.
    Blob = 2,
}

impl SectionKind {
    fn from_u16(value: u16) -> Option<Self> {
        match value {
            1 => Some(Self::Structured),
            2 => Some(Self::Blob),
            _ => None,
        }
    }
}

/// Flavor of a [`SectionKind::Structured`] section: which root type it holds.
///
/// Flavor is an **enumerated role**, compared for equality, not a bitfield —
/// the container decision's "flavor flags" wording is narrowed here, because a
/// section has exactly one role and [`Container::find`] compares the whole
/// field. Two roles in one section would need a second entry, not a second bit.
pub const FLAVOR_UI: u16 = 1;

/// Flavor of a [`SectionKind::Blob`] section: an asset payload referenced from
/// the document's asset table.
pub const FLAVOR_ASSET: u16 = 1;

// ---------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------

/// The fixed 64-byte header at offset 0.
///
/// `#[repr(C)]` pins the layout and the assertions below pin every offset to
/// the number in the format doc. The struct is never a transmute target —
/// [`Header::decode`] and [`Header::encode`] are the only ways bytes become
/// fields and back — but it *is* the layout: both directions take their byte
/// ranges from `offset_of!` on this struct, so the assertions are load-bearing
/// rather than decorative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Header {
    pub magic: [u8; 8],
    pub format_version: u16,
    pub header_size: u16,
    pub section_stride: u16,
    pub reserved_0: u16,
    pub section_count: u32,
    pub flags: u32,
    /// BLAKE3 over the section-table bytes — that is, over
    /// `bytes[header_size .. header_size + section_count * section_stride]`.
    /// It does not cover the header itself, which is what the deferred
    /// signature is for.
    pub root_hash: [u8; HASH_LEN],
    /// Reserved signature reference. Written zero, and required to be zero, in
    /// version 1: a writer that filled it without bumping the version produced
    /// a file this reader must not interpret.
    pub signature_offset: u32,
    /// Reserved signature reference. Written zero, and required to be zero, in
    /// version 1, as [`Header::signature_offset`].
    pub signature_length: u32,
}

const _: () = assert!(size_of::<Header>() == HEADER_SIZE);
const _: () = assert!(offset_of!(Header, magic) == 0);
const _: () = assert!(offset_of!(Header, format_version) == 8);
const _: () = assert!(offset_of!(Header, header_size) == 10);
const _: () = assert!(offset_of!(Header, section_stride) == 12);
const _: () = assert!(offset_of!(Header, reserved_0) == 14);
const _: () = assert!(offset_of!(Header, section_count) == 16);
const _: () = assert!(offset_of!(Header, flags) == 20);
const _: () = assert!(offset_of!(Header, root_hash) == 24);
const _: () = assert!(offset_of!(Header, signature_offset) == 56);
const _: () = assert!(offset_of!(Header, signature_length) == 60);

/// One fixed-stride 64-byte section-table entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SectionEntry {
    /// [`SectionKind`] as its numeric value.
    pub kind: u16,
    /// Role flags within the kind — [`FLAVOR_UI`], [`FLAVOR_ASSET`].
    pub flavor: u16,
    pub reserved_0: u32,
    /// Byte offset of the payload from the start of the file.
    pub offset: u64,
    /// Payload length in bytes.
    pub length: u64,
    /// BLAKE3 over the payload.
    pub hash: [u8; HASH_LEN],
    pub reserved_1: [u8; 8],
}

const _: () = assert!(size_of::<SectionEntry>() == SECTION_STRIDE);
const _: () = assert!(offset_of!(SectionEntry, kind) == 0);
const _: () = assert!(offset_of!(SectionEntry, flavor) == 2);
const _: () = assert!(offset_of!(SectionEntry, reserved_0) == 4);
const _: () = assert!(offset_of!(SectionEntry, offset) == 8);
const _: () = assert!(offset_of!(SectionEntry, length) == 16);
const _: () = assert!(offset_of!(SectionEntry, hash) == 24);
const _: () = assert!(offset_of!(SectionEntry, reserved_1) == 56);

/// Byte offsets, taken from the structs above so the compile-time assertions
/// bind the encoders as well as the layout. Writing a literal here instead
/// would let a field move in the file with both directions still agreeing.
mod at {
    use super::{Header, SectionEntry, offset_of};

    pub(super) const MAGIC: usize = offset_of!(Header, magic);
    pub(super) const FORMAT_VERSION: usize = offset_of!(Header, format_version);
    pub(super) const HEADER_SIZE_FIELD: usize = offset_of!(Header, header_size);
    pub(super) const SECTION_STRIDE_FIELD: usize = offset_of!(Header, section_stride);
    pub(super) const HEADER_RESERVED_0: usize = offset_of!(Header, reserved_0);
    pub(super) const SECTION_COUNT: usize = offset_of!(Header, section_count);
    pub(super) const FLAGS: usize = offset_of!(Header, flags);
    pub(super) const ROOT_HASH: usize = offset_of!(Header, root_hash);
    pub(super) const SIGNATURE_OFFSET: usize = offset_of!(Header, signature_offset);
    pub(super) const SIGNATURE_LENGTH: usize = offset_of!(Header, signature_length);

    pub(super) const KIND: usize = offset_of!(SectionEntry, kind);
    pub(super) const FLAVOR: usize = offset_of!(SectionEntry, flavor);
    pub(super) const ENTRY_RESERVED_0: usize = offset_of!(SectionEntry, reserved_0);
    pub(super) const OFFSET: usize = offset_of!(SectionEntry, offset);
    pub(super) const LENGTH: usize = offset_of!(SectionEntry, length);
    pub(super) const HASH: usize = offset_of!(SectionEntry, hash);
    pub(super) const ENTRY_RESERVED_1: usize = offset_of!(SectionEntry, reserved_1);
}

/// Writes a little-endian scalar at `at`.
macro_rules! put {
    ($out:expr, $at:expr, $value:expr) => {{
        let value = $value.to_le_bytes();
        $out[$at..$at + value.len()].copy_from_slice(&value);
    }};
}

/// Reads a little-endian scalar of type `$ty` at `at`.
macro_rules! get {
    ($bytes:expr, $at:expr, $ty:ty) => {
        <$ty>::from_le_bytes(
            $bytes[$at..$at + size_of::<$ty>()]
                .try_into()
                .expect("a fixed-width slice"),
        )
    };
}

impl Header {
    fn encode(&self) -> [u8; HEADER_SIZE] {
        let mut out = [0u8; HEADER_SIZE];
        out[at::MAGIC..at::MAGIC + self.magic.len()].copy_from_slice(&self.magic);
        put!(out, at::FORMAT_VERSION, self.format_version);
        put!(out, at::HEADER_SIZE_FIELD, self.header_size);
        put!(out, at::SECTION_STRIDE_FIELD, self.section_stride);
        put!(out, at::HEADER_RESERVED_0, self.reserved_0);
        put!(out, at::SECTION_COUNT, self.section_count);
        put!(out, at::FLAGS, self.flags);
        out[at::ROOT_HASH..at::ROOT_HASH + HASH_LEN].copy_from_slice(&self.root_hash);
        put!(out, at::SIGNATURE_OFFSET, self.signature_offset);
        put!(out, at::SIGNATURE_LENGTH, self.signature_length);
        out
    }

    fn decode(bytes: &[u8; HEADER_SIZE]) -> Self {
        Self {
            magic: bytes[at::MAGIC..at::MAGIC + 8].try_into().expect("8 bytes"),
            format_version: get!(bytes, at::FORMAT_VERSION, u16),
            header_size: get!(bytes, at::HEADER_SIZE_FIELD, u16),
            section_stride: get!(bytes, at::SECTION_STRIDE_FIELD, u16),
            reserved_0: get!(bytes, at::HEADER_RESERVED_0, u16),
            section_count: get!(bytes, at::SECTION_COUNT, u32),
            flags: get!(bytes, at::FLAGS, u32),
            root_hash: bytes[at::ROOT_HASH..at::ROOT_HASH + HASH_LEN]
                .try_into()
                .expect("32 bytes"),
            signature_offset: get!(bytes, at::SIGNATURE_OFFSET, u32),
            signature_length: get!(bytes, at::SIGNATURE_LENGTH, u32),
        }
    }
}

impl SectionEntry {
    fn encode(&self) -> [u8; SECTION_STRIDE] {
        let mut out = [0u8; SECTION_STRIDE];
        put!(out, at::KIND, self.kind);
        put!(out, at::FLAVOR, self.flavor);
        put!(out, at::ENTRY_RESERVED_0, self.reserved_0);
        put!(out, at::OFFSET, self.offset);
        put!(out, at::LENGTH, self.length);
        out[at::HASH..at::HASH + HASH_LEN].copy_from_slice(&self.hash);
        out[at::ENTRY_RESERVED_1..at::ENTRY_RESERVED_1 + self.reserved_1.len()]
            .copy_from_slice(&self.reserved_1);
        out
    }

    fn decode(bytes: &[u8]) -> Self {
        debug_assert_eq!(bytes.len(), SECTION_STRIDE);
        Self {
            kind: get!(bytes, at::KIND, u16),
            flavor: get!(bytes, at::FLAVOR, u16),
            reserved_0: get!(bytes, at::ENTRY_RESERVED_0, u32),
            offset: get!(bytes, at::OFFSET, u64),
            length: get!(bytes, at::LENGTH, u64),
            hash: bytes[at::HASH..at::HASH + HASH_LEN]
                .try_into()
                .expect("32 bytes"),
            reserved_1: bytes[at::ENTRY_RESERVED_1..at::ENTRY_RESERVED_1 + 8]
                .try_into()
                .expect("8 bytes"),
        }
    }
}

// ---------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------

/// One section handed to [`write`].
#[derive(Debug, Clone, Copy)]
pub struct Section<'a> {
    pub kind: SectionKind,
    pub flavor: u16,
    pub payload: &'a [u8],
}

impl<'a> Section<'a> {
    /// A structured section: a complete flatbuffer.
    pub fn structured(flavor: u16, payload: &'a [u8]) -> Self {
        Self {
            kind: SectionKind::Structured,
            flavor,
            payload,
        }
    }

    /// A blob section: raw payload bytes with no dashscene framing.
    pub fn blob(flavor: u16, payload: &'a [u8]) -> Self {
        Self {
            kind: SectionKind::Blob,
            flavor,
            payload,
        }
    }
}

/// Why a set of sections cannot be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// A structured section followed a blob. The hot region is the envelope
    /// plus every structured section, and it has to be contiguous at the head
    /// for the page-aligned hot/cold boundary to mean anything.
    StructuredAfterBlob { index: usize },
    /// More sections than the header's `u32` count can express, or than a
    /// section table can address on this target.
    TooManySections { count: usize },
    /// A section with no payload. A structured section with no bytes is not a
    /// flatbuffer and a blob with no bytes is not an asset, so an empty section
    /// has no meaning — and it would still claim its alignment, which for the
    /// first blob is a whole page of padding for nothing.
    EmptyPayload { index: usize },
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StructuredAfterBlob { index } => write!(
                f,
                "section {index} is structured but follows a blob: every structured \
                 section belongs in the hot region at the head of the file"
            ),
            Self::TooManySections { count } => {
                write!(f, "{count} sections exceeds the u32 section count")
            }
            Self::EmptyPayload { index } => {
                write!(f, "section {index} has an empty payload")
            }
        }
    }
}

impl Error for WriteError {}

/// Rounds `value` up to the next multiple of `align` (a power of two).
fn align_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}

/// Assembles `sections` into container bytes.
///
/// The layout, and the reasons for it (`docs/design/dsb-container-format.md`):
///
/// - header, then the section table, then payloads in table order;
/// - every payload starts on a [`SECTION_ALIGN`] boundary;
/// - the boundary between the last hot byte and the first cold byte is
///   [`PAGE_ALIGN`]-aligned — the one place alignment is format-relevant,
///   because it is what lets a load gate verify hot bytes without faulting
///   cold pages. With no blobs there is no boundary, and no padding is
///   written for one;
/// - a blob of [`LARGE_BLOB_THRESHOLD`] or more starts on a page boundary;
/// - every alignment gap is zero-filled, and the file ends at the last
///   payload byte.
///
/// Packing above the alignment floors is writer policy, not format law: the
/// format records only offsets, so a better heuristic needs no version bump.
pub fn write(sections: &[Section<'_>]) -> Result<Vec<u8>, WriteError> {
    let too_many = WriteError::TooManySections {
        count: sections.len(),
    };
    if u32::try_from(sections.len()).is_err() {
        return Err(too_many);
    }
    // Not implied by the u32 bound: on wasm32 `usize` is 32 bits, so every
    // possible `sections.len()` fits a u32 and only this check stands between a
    // large table and a wrapping multiply.
    let table_bytes = sections
        .len()
        .checked_mul(SECTION_STRIDE)
        .and_then(|bytes| bytes.checked_add(HEADER_SIZE))
        .ok_or(too_many)?
        - HEADER_SIZE;

    let mut seen_blob = false;
    for (index, section) in sections.iter().enumerate() {
        if section.payload.is_empty() {
            return Err(WriteError::EmptyPayload { index });
        }
        match section.kind {
            SectionKind::Blob => seen_blob = true,
            SectionKind::Structured if seen_blob => {
                return Err(WriteError::StructuredAfterBlob { index });
            }
            SectionKind::Structured => {}
        }
    }

    let mut cursor = HEADER_SIZE + table_bytes;
    let mut entries = Vec::with_capacity(sections.len());
    let mut first_blob = true;

    for section in sections {
        let align = match section.kind {
            SectionKind::Structured => SECTION_ALIGN,
            SectionKind::Blob => {
                // The hot/cold boundary, then the per-blob floor.
                let boundary = std::mem::take(&mut first_blob);
                if boundary || section.payload.len() >= LARGE_BLOB_THRESHOLD {
                    PAGE_ALIGN
                } else {
                    SECTION_ALIGN
                }
            }
        };
        let offset = align_up(cursor, align);
        entries.push(SectionEntry {
            kind: section.kind as u16,
            flavor: section.flavor,
            reserved_0: 0,
            offset: offset as u64,
            length: section.payload.len() as u64,
            hash: blake3::hash(section.payload).into(),
            reserved_1: [0; 8],
        });
        cursor = offset + section.payload.len();
    }

    let mut table = Vec::with_capacity(table_bytes);
    for entry in &entries {
        table.extend_from_slice(&entry.encode());
    }

    let header = Header {
        magic: MAGIC,
        format_version: FORMAT_VERSION,
        header_size: HEADER_SIZE as u16,
        section_stride: SECTION_STRIDE as u16,
        reserved_0: 0,
        section_count: sections.len() as u32,
        flags: 0,
        root_hash: blake3::hash(&table).into(),
        signature_offset: 0,
        signature_length: 0,
    };

    let mut out = vec![0u8; cursor];
    out[0..HEADER_SIZE].copy_from_slice(&header.encode());
    out[HEADER_SIZE..HEADER_SIZE + table_bytes].copy_from_slice(&table);
    for (entry, section) in entries.iter().zip(sections) {
        let at = entry.offset as usize;
        out[at..at + section.payload.len()].copy_from_slice(section.payload);
    }
    Ok(out)
}

// ---------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------

/// Why a byte string is not a valid container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerError {
    /// Shorter than a header.
    TooSmall { len: usize },
    /// The signature does not match [`MAGIC`]. A pre-envelope `.dsb` — a bare
    /// flatbuffer — lands here, by design: there is no transitional reader.
    BadMagic,
    /// A format version this build does not implement. The envelope evolves by
    /// version bump, so an unknown version is refused whole.
    UnsupportedVersion { found: u16 },
    /// The header or entry size does not match this version's fixed layout.
    BadLayout { field: &'static str, found: u16 },
    /// A reserved field is non-zero, or a flags field carries a bit this
    /// version does not define. Both mean the same thing: a writer put
    /// information here without bumping the version, and this reader must not
    /// guess at it.
    ReservedNotZero { field: &'static str },
    /// A structured section follows a blob. The hot region is the envelope plus
    /// the structured sections, and it has to be a contiguous prefix — a load
    /// gate derives its verify-and-prefetch ranges from that. The writer refuses
    /// to produce this order; the reader refuses to trust it, because the
    /// envelope exists precisely to not trust the writer.
    StructuredAfterBlob { index: usize },
    /// The section table does not fit in the file.
    TableOutOfRange,
    /// A section's byte range runs past the end of the file.
    SectionOutOfRange { index: usize },
    /// A section's byte range reaches into the header or the section table.
    SectionOverlapsTable { index: usize },
    /// Sections are not in ascending, non-overlapping file order. The table
    /// describes an ordered partition of the file — hot sections first — and
    /// that is what makes "verify the hot region" a contiguous byte range.
    SectionsOutOfOrder { index: usize },
    /// A section kind this version does not define.
    UnknownSectionKind { index: usize, kind: u16 },
    /// The section table does not hash to the header's root hash.
    RootHashMismatch,
    /// A section's payload does not hash to its recorded content hash.
    SectionHashMismatch { index: usize },
}

impl fmt::Display for ContainerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooSmall { len } => write!(
                f,
                "{len} bytes is shorter than the {HEADER_SIZE}-byte container header"
            ),
            Self::BadMagic => write!(
                f,
                "not a .dsb container: the file does not start with the container signature"
            ),
            Self::UnsupportedVersion { found } => write!(
                f,
                "container format version {found} is not supported (this build reads {FORMAT_VERSION})"
            ),
            Self::BadLayout { field, found } => write!(
                f,
                "container {field} is {found}, which is not this version's fixed layout"
            ),
            Self::ReservedNotZero { field } => {
                write!(f, "reserved container field {field} is not zero")
            }
            Self::StructuredAfterBlob { index } => write!(
                f,
                "section {index} is structured but follows a blob: the hot region \
                 must be a contiguous prefix of the file"
            ),
            Self::TableOutOfRange => write!(f, "the section table runs past the end of the file"),
            Self::SectionOutOfRange { index } => {
                write!(f, "section {index} runs past the end of the file")
            }
            Self::SectionOverlapsTable { index } => write!(
                f,
                "section {index} reaches into the header or the section table"
            ),
            Self::SectionsOutOfOrder { index } => write!(
                f,
                "section {index} starts before the end of the section before it"
            ),
            Self::UnknownSectionKind { index, kind } => {
                write!(f, "section {index} has unknown kind {kind}")
            }
            Self::RootHashMismatch => {
                write!(f, "the section table does not match the header's root hash")
            }
            Self::SectionHashMismatch { index } => {
                write!(
                    f,
                    "section {index} does not match its recorded content hash"
                )
            }
        }
    }
}

impl Error for ContainerError {}

/// A parsed container, borrowing the file bytes.
///
/// Holds no copy of any payload: [`Container::section_bytes`] returns a slice
/// into the same buffer that was parsed, so a memory mapping of the file needs
/// no second code path.
#[derive(Debug, Clone, Copy)]
pub struct Container<'a> {
    bytes: &'a [u8],
    count: usize,
}

impl<'a> Container<'a> {
    /// Validates the envelope and the section table.
    ///
    /// What this checks, in order: the header's magic, version, and fixed
    /// layout; that the section table fits; that the table hashes to the
    /// header's root hash; and only then, entry by entry, that every section
    /// kind is known and every byte range is inside the file, clear of the
    /// table, and in ascending order. The table's integrity is established
    /// before anything is read out of it.
    ///
    /// What it does **not** do is hash any payload. That is deliberate: a
    /// caller verifying only the hot sections must not be made to touch cold
    /// pages. Use [`Container::verify_section`] or [`Container::verify_hot`].
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ContainerError> {
        if bytes.len() < HEADER_SIZE {
            return Err(ContainerError::TooSmall { len: bytes.len() });
        }
        let header = Header::decode(bytes[0..HEADER_SIZE].try_into().expect("64 bytes"));

        if header.magic != MAGIC {
            return Err(ContainerError::BadMagic);
        }
        if header.format_version != FORMAT_VERSION {
            return Err(ContainerError::UnsupportedVersion {
                found: header.format_version,
            });
        }
        if header.header_size as usize != HEADER_SIZE {
            return Err(ContainerError::BadLayout {
                field: "header_size",
                found: header.header_size,
            });
        }
        if header.section_stride as usize != SECTION_STRIDE {
            return Err(ContainerError::BadLayout {
                field: "section_stride",
                found: header.section_stride,
            });
        }
        // Every header field this version does not define must be zero. These
        // four sit outside `root_hash`'s range, so nothing else would notice
        // them being set; refusing them here is what keeps a later writer from
        // slipping meaning past a version-1 reader.
        for (value, field) in [
            (u32::from(header.reserved_0), "header.reserved_0"),
            (header.flags, "header.flags"),
            (header.signature_offset, "header.signature_offset"),
            (header.signature_length, "header.signature_length"),
        ] {
            if value != 0 {
                return Err(ContainerError::ReservedNotZero { field });
            }
        }

        let count = header.section_count as usize;
        let table_end = HEADER_SIZE
            .checked_add(
                count
                    .checked_mul(SECTION_STRIDE)
                    .ok_or(ContainerError::TableOutOfRange)?,
            )
            .ok_or(ContainerError::TableOutOfRange)?;
        if table_end > bytes.len() {
            return Err(ContainerError::TableOutOfRange);
        }

        // The table's own integrity comes before anything is read out of it:
        // the decision's load order is "validate magic/version/bounds, hash the
        // section table against the root hash, then verify sections".
        let table = &bytes[HEADER_SIZE..table_end];
        if header.root_hash != *blake3::hash(table).as_bytes() {
            return Err(ContainerError::RootHashMismatch);
        }

        let container = Self { bytes, count };

        let mut previous_end = table_end as u64;
        let mut seen_blob = false;
        for index in 0..count {
            let entry = container.section(index);
            if entry.reserved_0 != 0 {
                return Err(ContainerError::ReservedNotZero {
                    field: "section.reserved_0",
                });
            }
            if entry.reserved_1 != [0; 8] {
                return Err(ContainerError::ReservedNotZero {
                    field: "section.reserved_1",
                });
            }
            match SectionKind::from_u16(entry.kind) {
                None => {
                    return Err(ContainerError::UnknownSectionKind {
                        index,
                        kind: entry.kind,
                    });
                }
                Some(SectionKind::Blob) => seen_blob = true,
                Some(SectionKind::Structured) if seen_blob => {
                    return Err(ContainerError::StructuredAfterBlob { index });
                }
                Some(SectionKind::Structured) => {}
            }
            let end = entry
                .offset
                .checked_add(entry.length)
                .ok_or(ContainerError::SectionOutOfRange { index })?;
            if end > bytes.len() as u64 {
                return Err(ContainerError::SectionOutOfRange { index });
            }
            if entry.offset < table_end as u64 {
                return Err(ContainerError::SectionOverlapsTable { index });
            }
            if entry.offset < previous_end {
                return Err(ContainerError::SectionsOutOfOrder { index });
            }
            previous_end = end;
        }

        Ok(container)
    }

    /// The header, decoded.
    pub fn header(&self) -> Header {
        Header::decode(self.bytes[0..HEADER_SIZE].try_into().expect("64 bytes"))
    }

    /// How many sections the table holds.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the container holds no sections at all.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Entry `index`, decoded on demand.
    ///
    /// # Panics
    ///
    /// On an index at or past [`Container::len`] — the table's length is known
    /// from the moment [`Container::parse`] returns, so an out-of-range index
    /// is a caller bug, not a malformed file.
    pub fn section(&self, index: usize) -> SectionEntry {
        assert!(
            index < self.count,
            "section {index} out of range ({} sections)",
            self.count
        );
        let at = HEADER_SIZE + index * SECTION_STRIDE;
        SectionEntry::decode(&self.bytes[at..at + SECTION_STRIDE])
    }

    /// Every entry, in table order.
    pub fn sections(&self) -> impl Iterator<Item = SectionEntry> + '_ {
        (0..self.count).map(|index| self.section(index))
    }

    /// Section `index`'s payload, borrowed from the parsed buffer.
    ///
    /// The bytes are **not** hash-verified by this call. Verify first if the
    /// payload is about to be trusted.
    ///
    /// # Panics
    ///
    /// On an index at or past [`Container::len`], as [`Container::section`].
    pub fn section_bytes(&self, index: usize) -> &'a [u8] {
        let entry = self.section(index);
        let at = entry.offset as usize;
        &self.bytes[at..at + entry.length as usize]
    }

    /// The index of the first section of `kind` carrying `flavor`, if any.
    pub fn find(&self, kind: SectionKind, flavor: u16) -> Option<usize> {
        (0..self.count).find(|&index| {
            let entry = self.section(index);
            entry.kind == kind as u16 && entry.flavor == flavor
        })
    }

    /// Hashes section `index`'s payload and compares it with the table.
    ///
    /// # Panics
    ///
    /// On an index at or past [`Container::len`], as [`Container::section`].
    pub fn verify_section(&self, index: usize) -> Result<(), ContainerError> {
        let entry = self.section(index);
        if *blake3::hash(self.section_bytes(index)).as_bytes() == entry.hash {
            Ok(())
        } else {
            Err(ContainerError::SectionHashMismatch { index })
        }
    }

    /// Verifies every [`SectionKind::Structured`] section — the hot region —
    /// and touches no blob payload.
    pub fn verify_hot(&self) -> Result<(), ContainerError> {
        for index in 0..self.count {
            if self.section(index).kind == SectionKind::Structured as u16 {
                self.verify_section(index)?;
            }
        }
        Ok(())
    }
}
