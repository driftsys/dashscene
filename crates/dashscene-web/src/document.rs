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

use dashbuf::prefetch::ShownRoot;
use dashbuf::prefix::{self, Envelope, MIN_PREFIX, PrefixError};
use dashbuf::residency::BlobResidency;
use dashlang::LiveScene;
use dashscene_core::{Arena, Region};

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

/// Fetches the document at `url`, loads it into `arena`, and attaches a scene,
/// bounding what it fetches by the root `shown_root` names.
///
/// The same three steps the native host runs — open, gate, load — with the
/// opening half replaced by the prefix flow. The gate is not skipped because the
/// file arrived over a network: it is the one place that reports a referentially
/// broken document by name.
///
/// `shown_root` is a parameter here for the same reason it is one on
/// `dashscene_desktop::Document::load`: the embedder is what knows which
/// artboard it is showing, and this function runs again on every rebuild
/// (story #837).
///
/// **The bound is unconditional since story #838.** Naming a root other than
/// the first changes which payloads this fetches, for every document shape:
/// the load names that root on the arena below, so the traversal, the solve and
/// the paint follow it and a row nothing paints is a row nothing can ask for.
/// Until then the fetch widened to every root that drew, whatever was named,
/// and `shown`'s module documentation carries the whole of that history and the
/// one thing it left behind — a root this load did not read cannot be shown
/// afterwards.
///
/// # Errors
///
/// Every error **this function returns** is raised before the document is
/// replayed into `arena`, so a failed load leaves `arena` exactly as it was and
/// an embedder may reuse it. That is not true of the panics below.
///
/// # Panics
///
/// This can panic rather than return, and the conditions belong to the crates
/// under it rather than to this signature. The one an embedder meets is
/// `Txn::use_mapped_pool`, which refuses an arena whose image table already holds
/// rows, whatever put them there; passing a fresh [`Arena`] avoids it.
///
/// **On this target a panic aborts the wasm instance**, so unlike the native
/// host there is no caught state to reason about.
pub async fn load_document(
    url: &str,
    shown_root: ShownRoot,
    text: Option<crate::TextResources>,
    arena: &mut Arena,
) -> Result<LiveScene, WebError> {
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
        return Err(WebError::Gate(report));
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
        dashbuf::prefetch::resolve(&document, shown_root).ok_or_else(|| WebError::NoSuchRoot {
            url: url.to_owned(),
            ordinal: shown_root.ordinal(),
            roots: dashbuf::prefetch::root_count(&document),
        })?;
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
    // The root that bounded it is reported beside the count. This line carried
    // a `shown::Bound` until story #838, because "the shown root draws them
    // all" and "another root draws one, so all of them are read" produced the
    // same number and were not the same fact. There is one bound now, so the
    // fact worth reporting is **which root** it was: the same payload count
    // means something different for root 0 than for root 7, and the ordinal is
    // the only thing here that says which.
    crate::host::log(&format!(
        "{url}: {} bytes, {} sections, {} hot, {} of {} payload(s) read for root {}, {} B, {} request(s)",
        ranges.total(),
        envelope.sections().len(),
        envelope.hot_len(),
        layout.fetch.len(),
        plan.wanted().len(),
        shown_root.ordinal(),
        layout.bytes,
        ranges.requests(),
    ));

    // How many roots the arena held *before* this load, so the document ordinal
    // can be turned into the arena node it actually named. The load appends to
    // whatever the arena already holds, so the two ordinals agree only when it
    // held nothing (issue #943).
    let roots_before = arena.roots().len();
    // A `Vec<u8>` is a `Region`: `dashpaint` blanket-implements it for every
    // `AsRef<[u8]> + Send + Sync`. That is the whole of what this host was
    // missing to use the loader the native one uses — see `shown` for the
    // comment this replaced, which said a browser had no region.
    let region: Arc<dyn Region> = Arc::new(region);
    dashscene_core::load_document_mapped(&document, region, &layout.payloads, arena);
    // The runtime's half of the bound `shown::layout` took above. The load
    // replays every root — a document is every artboard it carries — and this
    // confines what is solved, committed and painted to the one being shown
    // (story #838, issue #822). A commit of its own rather than a parameter on
    // the loader: the load has already committed, and one extra commit per load
    // is cheaper than a signature change on three public loaders.
    //
    // Named by node: `Txn::show_root` takes the arena's own vocabulary, and this
    // is the one place holding both the document and the arena it was appended
    // to. Passing the ordinal straight through would confine the traversal to
    // the *first* document's root while the fetch above read this one's — the
    // wrong artboard, solved and painted, with nothing to report it (issue
    // #943).
    //
    // **A named panic rather than a typed error**, and the desktop loader's copy
    // carries the argument in full: this same function already panics by name
    // through `Txn::use_mapped_pool`, and `NoSuchRoot::roots` is documented as
    // what the *document* carries — a number that cannot describe this arm,
    // since `prefetch::resolve` above already proved the document carries more
    // roots than the ordinal. A broken `dashscene-core` invariant is a
    // diagnostic that names it (P4), not an embedder error.
    let shown = *arena
        .roots()
        .get(roots_before + shown_root.ordinal() as usize)
        .unwrap_or_else(|| {
            // Inside the closure: nothing here runs on the ordinary path, and
            // `saturating_sub` so a shrunken root list cannot replace this
            // diagnostic with a bare subtraction overflow.
            let appended = arena.roots().len().saturating_sub(roots_before);
            panic!(
                "{url} declares {} root(s) and this load appended {appended} to the arena, so \
                 ordinal {} names no node: `load_document_mapped` appends one arena root per \
                 document root, and `dashbuf::prefetch::resolve` above already proved this \
                 document has that root",
                dashbuf::prefetch::root_count(&document),
                shown_root.ordinal(),
            )
        });
    let mut txn = arena.open();
    txn.show_root(Some(shown));
    txn.commit();
    Ok(dashlang::attach_live(
        arena,
        dashscene_engine::TaffySolver::boxed(text),
    ))
}
