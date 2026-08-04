//! Reading a `.dsb` envelope out of a leading byte range, without holding the
//! file.
//!
//! Specified by
//! `docs/decisions/container-parse-reads-a-prefix-through-a-host-reader.md`.
//!
//! # Why this exists beside `Container::parse`
//!
//! [`crate::container::Container::parse`] bounds-checks every section's
//! declared extent against the length of the slice it is given. It never reads
//! those bytes; it only checks their extents. That one check costs nothing
//! under `mmap`, where the mapping is full-length and the pages are never
//! touched — and everything in wasm, where there is no mapping and the same
//! check forces the whole file into linear memory before the envelope can be
//! read at all.
//!
//! So the strict reader stays strict, and a host that holds only a prefix uses
//! this one instead. The two ask different questions: `parse` answers "is this
//! whole file consistent", and [`Envelope::read`] answers "can I see enough to
//! know what to fetch next".
//!
//! They do **not** apply different rules. Every check is one implementation
//! that both call, bounds included; the only difference is that the strict
//! reader takes the file's length from the slice it holds, and this one is
//! told the length by its host. A truncated file is still named at the gate.
//!
//! # The flow
//!
//! ```text
//! fetch [0, MIN_PREFIX)          -> Err(NeedMore { need })   // the table's extent
//! fetch [0, need)                -> Ok(envelope)
//! fetch [0, envelope.hot_len())  -> the document and every structured section
//! fetch each blob's own range    -> by hash, on demand
//! ```
//!
//! Two round trips before the envelope is known, and the section layout then
//! makes the hot region one contiguous range rather than a scatter. The file's
//! total length comes back with the first range response, which is why
//! [`Envelope::read`] can ask for it without costing a request.

use std::error::Error;
use std::fmt;
use std::ops::Range;

use crate::container::{
    ContainerError, FLAVOR_BINDINGS, FLAVOR_UI, HASH_LEN, HEADER_SIZE, Header, SECTION_STRIDE,
    SectionEntry, SectionKind, check_table,
};

/// The smallest prefix worth fetching: the fixed header, which is what states
/// how long the section table is.
///
/// A host fetching exactly this reaches the envelope in two round trips.
pub const MIN_PREFIX: usize = HEADER_SIZE;

/// Why [`Envelope::read`] did not return an envelope.
///
/// The two variants are different kinds of answer, and a host must not treat
/// them alike: one says fetch more, the other says stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefixError {
    /// Not malformed — short. Fetch at least `need` bytes from offset zero and
    /// call again. `need` never shrinks as the prefix grows, so a host can
    /// treat it as a target rather than a step.
    NeedMore { need: usize },
    /// The envelope is malformed, by the same rule and under the same name
    /// [`crate::container::Container::parse`] would give it.
    Malformed(ContainerError),
}

impl fmt::Display for PrefixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NeedMore { need } => {
                write!(
                    f,
                    "the envelope needs {need} bytes from the start of the file"
                )
            }
            Self::Malformed(error) => write!(f, "{error}"),
        }
    }
}

impl Error for PrefixError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NeedMore { .. } => None,
            Self::Malformed(error) => Some(error),
        }
    }
}

impl From<ContainerError> for PrefixError {
    fn from(error: ContainerError) -> Self {
        Self::Malformed(error)
    }
}

/// A `.dsb` envelope — the header and the section table — read from a prefix
/// of the file and owning its own copy of the table.
///
/// Owned rather than borrowing, because a host that fetches into a buffer will
/// reuse that buffer for the payloads that come next. The table is a few tens
/// of bytes per section; the payloads are the large thing, and this holds none
/// of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    sections: Vec<SectionEntry>,
    hot_len: u64,
}

impl Envelope {
    /// Reads the envelope from a prefix of a `.dsb` file `file_len` bytes long.
    ///
    /// Runs every check [`crate::container::Container::parse`] runs, including
    /// that each section lies inside the file. The difference is only where the
    /// file's length comes from: that reader holds the file, this one is told.
    /// A host always knows it without holding the bytes — `Content-Range` gives
    /// it on the first range request, and a local `File` states it outright.
    ///
    /// Requiring it rather than skipping the bound is what keeps a truncated
    /// file a fault named at the gate. Without it, a section count of
    /// `u32::MAX` reads as a well-formed request to fetch 256 GiB on a 64-bit
    /// host, and as an overflow on a 32-bit one — the answer would depend on
    /// the target, and wasm is the 32-bit one.
    ///
    /// Returns [`PrefixError::NeedMore`] when the prefix is shorter than the
    /// envelope, twice at most: once for the header, then once for the table
    /// whose length the header states.
    pub fn read(prefix: &[u8], file_len: u64) -> Result<Self, PrefixError> {
        // A file shorter than the header is not a short *prefix*, it is not a
        // `.dsb` — and the two are different answers: one says fetch more, and
        // fetching more of this file would return nothing. Named with the
        // strict reader's own diagnostic, because for these bytes the two
        // readers must agree.
        if file_len < HEADER_SIZE as u64 {
            return Err(PrefixError::Malformed(ContainerError::TooSmall {
                len: file_len as usize,
            }));
        }
        if prefix.len() < HEADER_SIZE {
            return Err(PrefixError::NeedMore { need: MIN_PREFIX });
        }
        let header = Header::decode(prefix[0..HEADER_SIZE].try_into().expect("64 bytes"));
        header.check()?;

        let table_end = header.table_end()?;
        if table_end as u64 > file_len {
            return Err(PrefixError::Malformed(ContainerError::TableOutOfRange));
        }
        if prefix.len() < table_end {
            return Err(PrefixError::NeedMore { need: table_end });
        }

        // The table's own integrity comes before anything is read out of it —
        // the same load order `Container::parse` follows, and the reason the
        // root hash covers the table rather than the file.
        if header.root_hash != *blake3::hash(&prefix[HEADER_SIZE..table_end]).as_bytes() {
            return Err(PrefixError::Malformed(ContainerError::RootHashMismatch));
        }

        let sections: Vec<SectionEntry> = (0..header.section_count as usize)
            .map(|index| {
                let at = HEADER_SIZE + index * SECTION_STRIDE;
                SectionEntry::decode(&prefix[at..at + SECTION_STRIDE])
            })
            .collect();

        // The same walk `Container::parse` runs, bounded by the length the host
        // stated rather than by the bytes in hand.
        let hot_len = check_table(sections.len(), |index| sections[index], table_end, file_len)?;

        Ok(Self { sections, hot_len })
    }

    /// The section table, in file order.
    pub fn sections(&self) -> &[SectionEntry] {
        &self.sections
    }

    /// How many bytes from the start of the file make up the **hot run**: the
    /// envelope and every structured section, which is every part of a `.dsb`
    /// that is not an asset payload.
    ///
    /// One contiguous range, because the format requires structured sections to
    /// precede blobs and to be ascending and non-overlapping. A file carrying
    /// no structured section at all has a hot run of the envelope alone.
    pub fn hot_len(&self) -> u64 {
        self.hot_len
    }

    /// The byte range of the blob section whose content hash is `hash`, for a
    /// host to fetch on its own.
    ///
    /// Searches blob sections only, as
    /// [`crate::container::Container::blob_by_hash`] does: a search over every
    /// section would resolve an asset's hash to a structured section.
    ///
    /// **Unlike that reader, this one cannot verify.** It holds the payload and
    /// hashes it before handing it over; this holds no bytes at all, so
    /// checking what comes back against the table is the host's own step —
    /// [`Plan::bind`] is where that happens for a loaded document.
    pub fn blob_by_hash(&self, hash: &[u8]) -> Result<Range<u64>, ContainerError> {
        let entry = self.sections[self.blob_index_by_hash(hash)?];
        Ok(entry.offset..entry.offset + entry.length)
    }

    /// The index of the blob section whose content hash is `hash`.
    fn blob_index_by_hash(&self, hash: &[u8]) -> Result<usize, ContainerError> {
        self.sections
            .iter()
            .position(|entry| entry.kind == SectionKind::Blob as u16 && entry.hash == hash)
            .ok_or(ContainerError::NoBlobForHash)
    }

    /// The index of the one structured section carrying `flavor`, or `Err` with
    /// how many there are when that is not one.
    ///
    /// The same shape as `Container::only_structured`, over an owned table.
    fn only_structured(&self, flavor: u16) -> Result<usize, usize> {
        let mut found = 0;
        let mut at = 0;
        for (index, entry) in self.sections.iter().enumerate() {
            if entry.kind == SectionKind::Structured as u16 && entry.flavor == flavor {
                found += 1;
                at = index;
            }
        }
        if found == 1 { Ok(at) } else { Err(found) }
    }

    /// Section `index`'s bytes out of a hot run, verified against the table.
    ///
    /// Fails rather than panics when the run is short of the section: a host
    /// that fetched too little is a case that happens, and reading whatever sits
    /// at that offset instead is how a truncated fetch becomes a wrong picture.
    fn section_in<'h>(&self, hot: &'h [u8], index: usize) -> Result<&'h [u8], ContainerError> {
        let entry = self.sections[index];
        let end = entry.offset + entry.length;
        if end > hot.len() as u64 {
            return Err(ContainerError::SectionOutOfRange { index });
        }
        let bytes = &hot[entry.offset as usize..end as usize];
        if *blake3::hash(bytes).as_bytes() == entry.hash {
            Ok(bytes)
        } else {
            Err(ContainerError::SectionHashMismatch { index })
        }
    }
}

/// Why fetched payloads could not be bound to a document.
///
/// Its own type rather than a pair of [`crate::OpenError`] variants, because
/// these are the only two ways [`Plan::bind`] can fail and neither is a way
/// [`crate::open`] can. An error a function cannot return does not belong in its
/// error type — the compiler said so, by way of an exhaustive match in `dashc`
/// that a `PayloadCount` variant on `OpenError` broke for no reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindError {
    /// A different number of payloads than the plan asked for. They bind by
    /// position, so a miscount would pair them with the wrong entries rather
    /// than fail — which is why it is refused instead of taken as far as it
    /// goes.
    Count { wanted: usize, given: usize },
    /// A payload does not hash to what the section table records for it, so it
    /// is not the payload the file names.
    Payload { section: usize },
}

impl fmt::Display for BindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Count { wanted, given } => write!(
                f,
                "the plan asked for {wanted} payloads and was given {given}"
            ),
            Self::Payload { section } => write!(
                f,
                "the payload fetched for section {section} does not match its recorded \
                 content hash"
            ),
        }
    }
}

impl Error for BindError {}

/// A payload the document names and the host has not fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    /// The blob section it lives in, as an index into [`Envelope::sections`].
    pub section: usize,
    /// Where it lies in the file — what a host turns into a range request.
    pub range: Range<u64>,
    /// What it must hash to. [`Plan::bind`] checks it; a host caching payloads
    /// across loads can key on it, since it is the content's own name.
    pub hash: [u8; HASH_LEN],
}

/// A document read out of a hot run, together with the payloads still to fetch.
///
/// The prefix-side counterpart of [`crate::open`], split in half around the
/// fetch that `open` never has to make. `open` resolves an asset entry to bytes
/// it already holds; a prefix host has to go and get them, and the split is
/// what lets it do that without either restating the binding rules or blocking
/// inside this crate.
///
/// The wanted list is one entry per asset entry, in entry order — not deduped.
/// Two entries naming one payload are two ranges here, as they are two lookups
/// in `open`. A host that minds fetching a range twice can cache by
/// [`Wanted::hash`].
#[derive(Debug)]
pub struct Plan<'a> {
    document: crate::Document<'a>,
    wanted: Vec<Wanted>,
}

impl<'a> Plan<'a> {
    /// The document, verified and structurally checked.
    pub fn document(&self) -> crate::Document<'a> {
        self.document
    }

    /// The payloads to fetch, one per asset entry and in entry order.
    ///
    /// Every range lies outside the hot run, because a structured section is by
    /// definition part of it.
    pub fn wanted(&self) -> &[Wanted] {
        &self.wanted
    }

    /// Checks fetched payloads against the table and hands them back in the
    /// shape [`dashscene_core::load_document`] takes — one per asset entry, in
    /// entry order.
    ///
    /// `fetched` must be [`Plan::wanted`]'s ranges, in the same order.
    /// Verification is not optional here and is not a separate call a host can
    /// forget: `open` hashes a payload before returning it, and a prefix load
    /// keeps that promise at this step or nowhere.
    ///
    /// [`dashscene_core::load_document`]: https://docs.rs/dashscene-core
    pub fn bind<'b>(&self, fetched: &[&'b [u8]]) -> Result<Vec<&'b [u8]>, BindError> {
        if fetched.len() != self.wanted.len() {
            return Err(BindError::Count {
                wanted: self.wanted.len(),
                given: fetched.len(),
            });
        }
        // The hash alone, with no length check beside it: a payload of a
        // different length hashes differently, so a length comparison here
        // could never fail on its own. A mutation pass removed one and nothing
        // could be written that failed.
        for (want, bytes) in self.wanted.iter().zip(fetched) {
            if *blake3::hash(bytes).as_bytes() != want.hash {
                return Err(BindError::Payload {
                    section: want.section,
                });
            }
        }
        Ok(fetched.to_vec())
    }
}

/// Reads the document out of a hot run and works out which payloads it needs.
///
/// `hot` is the file's first [`Envelope::hot_len`] bytes: the envelope and every
/// structured section, which is where both the ui document and the derivation
/// manifest live. Everything this step needs is therefore in one fetched range,
/// and everything it cannot do — the payloads — comes back as [`Plan::wanted`].
///
/// Runs the same checks [`crate::open`] runs over a whole file: the ui section's
/// content hash, the flatbuffers verifier over it, the manifest's own hash and
/// rows, and the resolution of each asset's canonical hash to a resident blob.
pub fn plan<'a>(envelope: &Envelope, hot: &'a [u8]) -> Result<Plan<'a>, crate::OpenError> {
    let ui = envelope
        .only_structured(FLAVOR_UI)
        .map_err(|found| ContainerError::NotOneUiSection { found })?;
    let document = crate::root_as_document(envelope.section_in(hot, ui)?)?;

    let manifest = match envelope.only_structured(FLAVOR_BINDINGS) {
        Ok(at) => Some(envelope.section_in(hot, at)?),
        Err(0) => None,
        Err(found) => return Err(ContainerError::NotOneBindingsSection { found }.into()),
    };
    let rows = crate::binding_rows(manifest)?;

    let mut wanted = Vec::new();
    for entry in document.assets().unwrap_or_default().iter() {
        let resident = crate::resident_of(&rows, entry.hash().bytes());
        let section = envelope.blob_index_by_hash(resident)?;
        let blob = envelope.sections[section];
        wanted.push(Wanted {
            section,
            range: blob.offset..blob.offset + blob.length,
            hash: blob.hash,
        });
    }

    Ok(Plan { document, wanted })
}
