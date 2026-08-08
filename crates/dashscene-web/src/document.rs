//! Loading a compiled `.dsb` over HTTP, by byte range.
//!
//! The flow `docs/decisions/container-parse-reads-a-prefix-through-a-host-reader.md`
//! describes, driven for real: fetch the fixed header, learn how long the
//! section table is, fetch the envelope, fetch the hot run as one contiguous
//! range, then fetch the payloads on their own.
//!
//! **Not every payload the document names** — that was the read until story
//! #792, and replacing it is what that story is. The set comes from `shown`,
//! which bounds it by the root being drawn when doing so is safe and says so
//! when it is not.
//!
//! `dashbuf::open` is not used and cannot be: it bounds-checks every section
//! against the length of the slice it is handed, which here would mean pulling
//! the whole file into linear memory before the envelope could be read at all.
//! That is the difference this host exists to demonstrate.

use std::cell::Cell;
use std::ops::Range;
use std::sync::Arc;

use dashbuf::prefix::{self, Envelope, MIN_PREFIX, PrefixError};
use dashbuf::residency::BlobResidency;
use dashlang::LiveScene;
use dashscene_core::{Arena, Region};
use dashscene_engine::TaffySolver;

use crate::{WebError, fetch, shown};

/// Where the loader reads bytes from.
struct Ranges {
    source: Source,
    /// How many range requests have gone over the network.
    ///
    /// Counted here rather than by the caller, because only this type knows
    /// which reads are requests: a [`Source::Resident`] read slices bytes
    /// already in hand and costs nothing. A counter kept outside reported one
    /// per payload either way, which is the number the log then printed —
    /// found in review, and exactly the term the log exists to make visible.
    requests: Cell<usize>,
}

/// Where the loader reads bytes from.
enum Source {
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
    /// A source, with the request that opened the load already counted: every
    /// load begins by fetching the fixed prefix, whichever way the server
    /// answered.
    fn new(source: Source) -> Self {
        Self {
            source,
            requests: Cell::new(1),
        }
    }

    /// The file's total length, which the envelope reader is bounded by.
    fn total(&self) -> u64 {
        match &self.source {
            Source::Remote { total, .. } => *total,
            Source::Resident(bytes) => bytes.len() as u64,
        }
    }

    /// How many requests have gone over the network so far.
    fn requests(&self) -> usize {
        self.requests.get()
    }

    async fn get(&self, range: Range<u64>) -> Result<Vec<u8>, WebError> {
        if range.is_empty() {
            return Ok(Vec::new());
        }
        match &self.source {
            // `end - 1`: an HTTP byte range names its last byte, where a Rust
            // range names one past it. The empty case is returned above, so the
            // subtraction cannot underflow here.
            Source::Remote { url, .. } => {
                self.requests.set(self.requests.get() + 1);
                Ok(fetch::range(url, range.start, range.end - 1).await?.bytes)
            }
            // No request: the whole file is already in hand, so this is a slice
            // and the counter does not move.
            Source::Resident(bytes) => bytes
                .get(range.start as usize..range.end as usize)
                .map(<[u8]>::to_vec)
                .ok_or(WebError::ShortFile),
        }
    }
}

/// Fetches the document at `url`, loads it into `arena`, and attaches a scene.
///
/// The same three steps the native host runs (`demo/src/document.rs`) — open,
/// gate, load — with the opening half replaced by the prefix flow. The gate is
/// not skipped because the file arrived over a network: it is the one place
/// that reports a referentially broken document by name.
pub async fn load_document(url: &str, arena: &mut Arena) -> Result<LiveScene, WebError> {
    let first = fetch::range(url, 0, MIN_PREFIX as u64 - 1).await?;
    if !first.ranged {
        crate::host::log(&format!(
            "{url}: the server ignored the range and sent all {} bytes; the \
             envelope still drives the read, but nothing further is fetched",
            first.bytes.len()
        ));
    }
    let ranges = Ranges::new(if first.ranged {
        Source::Remote {
            url: url.to_owned(),
            total: first.total,
        }
    } else {
        Source::Resident(first.bytes.clone())
    });

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
            Err(PrefixError::Malformed(error)) => return Err(WebError::Envelope(error)),
        }
    }
    let Some(envelope) = envelope else {
        return Err(WebError::EnvelopeUnreachable);
    };

    // One contiguous range: the envelope plus every structured section, which
    // is the document and its derivation manifest and nothing else.
    let hot = ranges.get(0..envelope.hot_len()).await?;
    let plan = prefix::plan(&envelope, &hot).map_err(WebError::Open)?;

    let document = plan.document();

    // The gate runs **before** anything is prefetched, where it used to run
    // after every payload had been fetched. Two reasons, and the first is a
    // requirement rather than a preference: `prefetch::assets_of_root` computes
    // subtree membership in one forward pass, which is sound only for a
    // document whose every node follows its parent — and the gate is what
    // refuses one that does not. The second is that a referentially broken
    // document is now refused without a single payload request.
    let report = dashscene_validator::validate_document(&document);
    if report.has_errors() {
        return Err(WebError::Gate(format!("{report:?}")));
    }

    // Bound as **canonical**, and refused when that would be a lie — the same
    // guard the native host carries, for the same reason (issue #640).
    // `prefix::plan` resolves each entry's hash through the derivation manifest,
    // so a file carrying one hands back the rung a profile selected rather than
    // the payload the document names. Binding that as canonical tags a KTX2 as
    // whatever the entry claims.
    //
    // It matters more here than it reads. This host binds ranges now rather
    // than bytes, and the owning loader used to catch a mismatched format by
    // parsing the payload's header — a step the mapped loader deliberately does
    // not take. It is also what keeps `ImageTable::push_mapped`'s baked-length
    // assertion out of reach of the empty ranges `shown::layout` writes for the
    // frames this load does not read.
    let entries = document.assets().unwrap_or_default();
    for (want, entry) in plan.wanted().iter().zip(entries.iter()) {
        if want.hash != entry.hash().bytes() {
            return Err(WebError::Derived(url.to_owned()));
        }
    }

    // What to read, and where each payload will sit once it has been read —
    // decided from the hot document alone, before a byte of any payload is
    // requested. That is what bounds this load by the root being shown rather
    // than by the file's size (R5).
    let root =
        dashbuf::prefetch::first_root(&document).ok_or_else(|| WebError::NoRoot(url.to_owned()))?;
    let layout = shown::layout(&document, plan.wanted(), root);

    // The payloads, one range each, appended into the region in the order the
    // layout laid them out. `Layout::fetch` is empty for a document carrying no
    // assets, and eight of the ten committed goldens are that, so this loop
    // doing nothing is the ordinary case rather than a sign something went
    // wrong.
    //
    // Each range is proven as it arrives, by the same call the native host
    // makes over its mapping (story #597). `Plan::bind` is no longer used: it
    // returned owned slices for the owning loader and checked that the number
    // of payloads matched the document's asset count, and the mapped loader
    // makes that check itself against a table `shown::layout` builds with one
    // row per entry.
    let residency = BlobResidency::new();
    let mut region: Vec<u8> = Vec::with_capacity(layout.bytes as usize);
    for want in layout.requests(plan.wanted()) {
        let bytes = ranges.get(want.range.clone()).await?;
        let proven = residency.touch(want, &bytes).map_err(WebError::Payload)?;
        region.extend_from_slice(proven);
    }
    // The layout said how many bytes this would be before any of them arrived,
    // so a short or over-long response is a fact this host can state rather
    // than a range that quietly points at the wrong payload.
    if region.len() as u64 != layout.bytes {
        return Err(WebError::ShortPayloads {
            asked: layout.bytes,
            got: region.len() as u64,
        });
    }

    // **The round trips, reported beside the bytes.** On a network the request
    // count is a cost the byte counter cannot see, and this is the first host
    // where that is true: the native one maps a file and makes none. The count
    // comes from `Ranges`, which is the only thing that knows a read went over
    // the network — a server that ignored the first range sent the whole file,
    // and every read after that is a slice.
    //
    // `bound` is reported rather than left to be inferred from the payload
    // count, because "the shown root draws them all" and "another root draws
    // one, so all of them are read" produce the same number and are not the
    // same fact.
    crate::host::log(&format!(
        "{url}: {} bytes, {} sections, {} hot, {} of {} payload(s) read ({:?}), {} B, {} request(s)",
        ranges.total(),
        envelope.sections().len(),
        envelope.hot_len(),
        layout.fetch.len(),
        plan.wanted().len(),
        layout.bound,
        layout.bytes,
        ranges.requests(),
    ));

    // A `Vec<u8>` is a `Region`: `dashpaint` blanket-implements it for every
    // `AsRef<[u8]> + Send + Sync`. That is the whole of what this host was
    // missing to use the loader the native one uses — see `shown` for the
    // comment this replaced, which said a browser had no region.
    let region: Arc<dyn Region> = Arc::new(region);
    dashscene_core::load_document_mapped(&document, region, &layout.payloads, arena);
    Ok(dashlang::attach_live(arena, Box::new(TaffySolver::new())))
}
