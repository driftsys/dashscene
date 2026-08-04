//! Loading a compiled `.dsb` over HTTP, by byte range.
//!
//! The flow `docs/decisions/container-parse-reads-a-prefix-through-a-host-reader.md`
//! describes, driven for real: fetch the fixed header, learn how long the
//! section table is, fetch the envelope, fetch the hot run as one contiguous
//! range, then fetch each payload the document names on its own.
//!
//! `dashbuf::open` is not used and cannot be: it bounds-checks every section
//! against the length of the slice it is handed, which here would mean pulling
//! the whole file into linear memory before the envelope could be read at all.
//! That is the difference this host exists to demonstrate.

use std::ops::Range;

use dashbuf::prefix::{self, Envelope, MIN_PREFIX, PrefixError};
use dashlang::LiveScene;
use dashscene_core::Arena;
use dashscene_engine::TaffySolver;

use crate::{HostError, fetch};

/// Where the loader reads bytes from.
enum Ranges {
    /// The server honours ranges, so each read is a request.
    Remote { url: String, total: u64 },
    /// The server ignored the first range and sent the whole file, so every
    /// further read is already in hand.
    ///
    /// The flow above is unchanged — the envelope still decides what is read
    /// and in what order — and nothing more goes over the network. Keeping one
    /// code path rather than falling back to `dashbuf::open` means the prefix
    /// reader is what runs either way, which is what makes a plain static
    /// server a usable way to look at this host.
    Resident(Vec<u8>),
}

impl Ranges {
    /// The file's total length, which the envelope reader is bounded by.
    fn total(&self) -> u64 {
        match self {
            Self::Remote { total, .. } => *total,
            Self::Resident(bytes) => bytes.len() as u64,
        }
    }

    async fn get(&self, range: Range<u64>) -> Result<Vec<u8>, HostError> {
        if range.is_empty() {
            return Ok(Vec::new());
        }
        match self {
            // `end - 1`: an HTTP byte range names its last byte, where a Rust
            // range names one past it. The empty case is returned above, so the
            // subtraction cannot underflow here.
            Self::Remote { url, .. } => {
                Ok(fetch::range(url, range.start, range.end - 1).await?.bytes)
            }
            Self::Resident(bytes) => bytes
                .get(range.start as usize..range.end as usize)
                .map(<[u8]>::to_vec)
                .ok_or(HostError::ShortFile),
        }
    }
}

/// Fetches the document at `url`, loads it into `arena`, and attaches a scene.
///
/// The same three steps the native host runs (`demo/src/document.rs`) — open,
/// gate, load — with the opening half replaced by the prefix flow. The gate is
/// not skipped because the file arrived over a network: it is the one place
/// that reports a referentially broken document by name.
pub(crate) async fn load(url: &str, arena: &mut Arena) -> Result<LiveScene, HostError> {
    let first = fetch::range(url, 0, MIN_PREFIX as u64 - 1).await?;
    if !first.ranged {
        crate::host::log(&format!(
            "{url}: the server ignored the range and sent all {} bytes; the \
             envelope still drives the read, but nothing further is fetched",
            first.bytes.len()
        ));
    }
    let ranges = if first.ranged {
        Ranges::Remote {
            url: url.to_owned(),
            total: first.total,
        }
    } else {
        Ranges::Resident(first.bytes.clone())
    };

    // At most two answers by `Envelope::read`'s contract — the header, then the
    // table whose length the header states. Bounded anyway rather than looped
    // on trust: this is a network loop, and a reader that kept asking for a
    // length it already had would spin against a server forever.
    let mut prefix_bytes = first.bytes;
    let mut envelope = None;
    for _ in 0..2 {
        match Envelope::read(&prefix_bytes, ranges.total()) {
            Ok(read) => {
                envelope = Some(read);
                break;
            }
            Err(PrefixError::NeedMore { need }) => {
                prefix_bytes = ranges.get(0..need as u64).await?;
            }
            Err(PrefixError::Malformed(error)) => return Err(HostError::Envelope(error)),
        }
    }
    let Some(envelope) = envelope else {
        return Err(HostError::EnvelopeUnreachable);
    };

    // One contiguous range: the envelope plus every structured section, which
    // is the document and its derivation manifest and nothing else.
    let hot = ranges.get(0..envelope.hot_len()).await?;
    let plan = prefix::plan(&envelope, &hot).map_err(HostError::Open)?;

    // Then the payloads, one range each. `plan.wanted()` is empty for a
    // document carrying no assets, and eight of the ten committed goldens are
    // that, so this loop doing nothing is the ordinary case rather than a sign
    // something went wrong.
    let mut fetched = Vec::with_capacity(plan.wanted().len());
    for want in plan.wanted() {
        fetched.push(ranges.get(want.range.clone()).await?);
    }
    let borrowed: Vec<&[u8]> = fetched.iter().map(Vec::as_slice).collect();
    let payloads = plan.bind(&borrowed).map_err(HostError::Bind)?;

    crate::host::log(&format!(
        "{url}: {} bytes, {} sections, {} hot, {} payload(s)",
        ranges.total(),
        envelope.sections().len(),
        envelope.hot_len(),
        payloads.len()
    ));

    let document = plan.document();
    let report = dashscene_validator::validate_document(&document);
    if report.has_errors() {
        return Err(HostError::Gate(format!("{report:?}")));
    }
    dashscene_core::load_document(&document, &payloads, arena);
    Ok(dashlang::attach_live(arena, Box::new(TaffySolver::new())))
}
