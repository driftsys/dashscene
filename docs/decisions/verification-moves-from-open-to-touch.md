# Blob verification moves off the two readers and onto the touch that makes a payload resident

    status   accepted (2026-08-06); **AS-BUILT 2026-08-07 (v0.16, story #597,
             PR #778)** — the shape shipped, and **three of its clauses did
             not survive contact**. The as-built section at the end records
             each, with what the code does instead and why. D1's return type,
             D3's ownership of the region, and D3's second-touch fast path are
             the three.
             **Corrected the same day**: the first version named only
             `dashbuf::open` and missed that `prefix::Plan::bind` is the
             second eager verification, and the one that runs over a mapping
             since story #596. D7 and D9 are the correction; the shape D1-D6
             chose is unchanged.
    scope    the contracts of both readers — `dashbuf::open` and
             `dashbuf::prefix::Plan::bind` — the residency step that replaces
             the verification they drop, and what the two hosts and story
             #598's benchmark do differently as a result. Boundary B is
             untouched: `ImageTable`, `ImageEntry` and the `Painter` trait are
             exactly what story #596 left them.
    builds on `docs/decisions/dsb-sectioned-container.md` ("Blob sections are
             untouched until the loader thread prefetches them (touch + hash +
             mark ready)."), `docs/decisions/assets-borrow-from-the-mapping.md`
             (the mapped pool this hands ranges to),
             `docs/decisions/startup-scaling-is-measured-by-a-counter.md`
             (the number this is aimed at)

## Context

Stories #595 and #596 landed the two halves the epic named, and the criterion
did not move. It is still **9.81x** — measured, not assumed — because the cost
that dominates cold start was never a copy.

**There are two eager verifications, in two functions, on two paths**, and the
first draft of this record named only one of them.

`dashbuf::open` resolves every asset entry through `Container::blob_by_hash`,
which calls `verify_section`, which BLAKE3-hashes the whole payload. So opening
a document reads every byte of every payload an entry names — the hot sections
as well, since `ui_document` and the manifest are hashed on the way. Under a
mapping that faults in every page holding any of it. Not _every_ page of the
file: a blob no entry names is never resolved and never hashed, and the
alignment padding between sections is not read. In the documents this repository
carries, every blob is named by an entry and blobs are nearly all of the bytes,
so the distinction changes the wording rather than the conclusion — but "the
whole file" is a claim the code does not make.

`dashbuf::prefix::Plan::bind` hashes every fetched payload against the section
table, one at a time. **At the time this was written — after story #596 and
before D1 was built — that was the one that ran over a mapping.** The native
mapped host read the envelope with `prefix`, planned, and called `bind` for its
check; `open` was left holding the embedded golden, a `&'static [u8]` with no
pages to fault. D1 is what reversed that: `open` returns ranges now, which was
the only reason the host had moved off it, so the native host reads through
`open` again and `prefix` is the browser's reader. The as-built section records
it. So the function this record was
written about is, on the only path where a mapping exists, no longer the
function doing the damage.

Both have to move, and they move to the same place.

The format was built to prevent this and says so. `container.rs`'s own module
doc: payload hashes are "checked on demand by `Container::verify_section` so
that a caller verifying only the hot sections never faults a cold page".
`Container::verify_hot` exists, verifies every structured section and "touches
no blob payload". `open` does not call it.

So the eager verification is not an oversight in the format. It is one call in
each of two readers, and moving both is what stands between the slice and its
criterion.

## The rule it has to keep

Story #597's first stated property is not negotiable and is not weakened here:
**a painter must never receive bytes that have not been hashed.** The trust
chain is transitive — the root hash covers the section table, the table carries
each blob's content hash — and handing out an unverified pointer breaks it.

The question this record answers is _when_ a payload is hashed, not _whether_.

## Decision

**D1 — `open` verifies the hot half and returns ranges, not slices.** Its
signature becomes `Result<(Document<'_>, Vec<Range<u64>>), OpenError>`: the
envelope, the section table's own hash, every structured section
(`Container::verify_hot`), the flatbuffers verifier over the ui section, the
manifest and its rows, and each asset entry resolved to the **range** its
payload occupies. No blob payload is read.

The return type is the guarantee. A caller cannot hand a range to a painter, so
"unverified bytes reached a painter" stops being a rule someone has to remember
and becomes a thing that does not compile. That is why this was preferred over
a second entry point differing by a flag: the prefix record already refused
"one function answering both questions", and a flag would have left the unsafe
answer the same type as the safe one.

**D2 — the eager reader stays, renamed for what it does.**
`open_verified(file)` is today's `open`, unchanged: every payload hashed, slices
returned. It is the right call for a tool that is checking a file rather than
drawing it — `dashc`'s CLI is exactly that — and for a test that wants the whole
file proven before it asserts anything.

It is not a hole. It verifies before it returns, so its slices are as
trustworthy as they are today; what it is not is proportional, and its name now
says so. The renaming is most of this story's diff: **54 call sites over 26
files**, nearly all of them tests that want the eager behaviour and should keep
it. A twenty-seventh file, `crates/dashscene-core/src/load.rs`, names
`dashbuf::open` only inside the worked example in its module doc, and that
example has to change too — it states the read contract, and after this there
are two of them.

**D3 — a `Residency` owns touch + hash + mark ready, one blob at a time.** It
holds the region and the hashes the section table declares, and its only way to
produce a readable payload is `touch(range)`, which hashes the bytes, records
the range as ready, and returns them. A second touch of a ready range returns
immediately and does not hash again — readiness is per blob, which is what the
format's own packing rule is stated over: "two small blobs sharing a page is
harmless because verification and readiness are per-blob, and a shared page
faulting early is free prefetch."

It lives in `dashbuf`, beside `container`, `prefix` and `map`, because the
hashes it checks are the section table's and the ranges it takes are the
envelope's. Nothing above `dashbuf` should have to know how a blob is proven.

**D4 — the load path prefetches the shown root's assets, and nothing else.**
Cold start is bounded by making resident exactly what the shown root needs: the
asset indices reachable from the root's subtree through its nodes' paint
entries. Everything else in the document stays cold, and the criterion's
equality is the measurement of that.

The set is computed from the document, which is hot and already read. No
payload is touched to decide which payloads to touch.

**D5 — `madvise` is dropped from this slice, and the reason is that nothing
here can see it.** The owner ruled on it (2026-08-07) after the shape was
questioned. Story #597's body lists it and epic #594's scope lists it; this is
where that is given up, so it is a recorded change rather than a silent one.

Three things were wrong with building it now, and the third settles it.

- **The ordering first drafted here was the useless one.** "Issued for each
  range about to be touched" means hinting and then immediately blocking on the
  pages just asked for. `MADV_WILLNEED` earns its keep from asynchrony — the
  shape that helps is to hint the whole prefetch set, then hash in order, so
  read-ahead for the later ranges overlaps the hashing of the first.
- **It is Unix-only.** `advise` and `advise_range` are both `#[cfg(unix)]` in
  `memmap2`, with no Windows counterpart exposed.
- **This slice cannot measure it, and the benchmark specifically cannot.**
  `startup-scaling-is-measured-by-a-counter.md` D1 makes cost a count of bytes
  rather than an elapsed time, and D6 asserts on no wall clock, so a hint that
  changes only timing is invisible to the criterion by construction. Worse, the
  benchmark **writes its own documents**, so they are in the page cache the
  moment they exist: every fault is minor, there is no disk read for a hint to
  overlap, and `WILLNEED` against a cached file is a no-op. That is the same
  fact that made `mincore(2)` unusable as the instrument, and the reasoning was
  not carried across.

Nothing is lost by waiting. The format's investment stands and is untouched:
`docs/design/dsb-container-format.md` still page-aligns a blob of
`LARGE_BLOB_THRESHOLD` (64 KiB) or more "so it can be prefetched and evicted
with a single `madvise` range", and that padding is paid whether or not anyone
calls it. What is missing is a cold-cache measurement, which is a hardware and
harness question rather than a loading-path one — the same reason two v1 epics
already wait on target hardware for absolute numbers (#476, #462). Filed
against v1.

When it is built, two properties this record already fixed still apply: the
hints go in as a batch before the touches, not one before each, and a failing
hint must not fail a load — it is advice the kernel may ignore, and a payload
that hashes successfully is resident whether or not the hint was taken.

**D6 — no loader thread in this slice.** Epic #594 says, where it argues that
placeholder activation stays in v1, that "prefetching the shown root's assets
before first paint makes cold-start track the shown root, which is the whole
criterion" — a statement about what R5 needs rather than one of the four scope
bullets, and the load-bearing one here. The demo builds its scene
before the frame loop starts, so the faults are already off the frame thread by
construction.

`Residency` is `Send + Sync` regardless, because the arena holds
`Arc<ImageTable>` across threads already
(`assets-borrow-from-the-mapping.md` D4) and because a thread is the next step
rather than a different design. **What is deliberately not built here is
streaming**: drawing a frame in which a payload is not yet ready needs the
placeholder field that has no producer, and that stays in v1 for the reason
`asset-model-content-addressed-blobs.md` records.

**D7 — `Plan::bind` gives up its hashing to the same `Residency`.** It is the
prefix route's eager verification and, since story #596, the one on the mapped
path. `bind`'s job becomes what its name says — binding fetched ranges to entry
order — and the hash moves to the touch, so there is one place a payload is
proven rather than two that could disagree.

The browser host feels this as a shape change rather than a loss. `demo-web`
fetches a range and calls `bind`; afterwards it fetches a range and touches it,
which is the same check at the same moment, minus the rule
`container-parse-reads-a-prefix-through-a-host-reader.md` currently leaves to
every host to remember.

**D8 — the counter records at the touch.** `LoadCost::record_hashed` moves out
of `open_with_cost` and out of `bind`, into `Residency::touch`, so the number
story #598 asserts on is what was actually made resident rather than what was
resolved. A payload resolved and never touched costs nothing and is counted as
nothing, which is the claim R5 makes.

**D9 — story #598's benchmark has to move onto the mapped path.** It measures
`open_with_cost` plus `load_document_bound_with_cost` today, which is the
**owned** path: it writes its two documents to memory, not to files, and never
maps anything. Left alone it would keep reporting the owned path's number and
could not see any of this. The re-run therefore writes each generated document
to a temporary file, maps it, and loads it the way the native host does — which
is also what makes the criterion a measurement of what a host really does rather
than of a path only the benchmark takes.

## Consequences

- **The criterion can reach 1.00x.** Showing the same root out of a one-frame
  document and out of a sixty-five-frame one touches the same payloads, so it
  hashes the same number of bytes. Nothing else in this slice could have made
  that true: #595 removed a read that was not the expensive one, #596 removed
  two copies, and this removes the faults.
- **`open`'s doc comment stops being half-false.** It has claimed since v0.11
  that "a memory mapping of it works unchanged" — true of the copies and false
  of the faults. After this it is true of both.
- **Guardrail G-19 can be met.** `docs/technotes/engineering-guardrails.md`
  currently records it as failing, with story #597 named as the scheduled work;
  this is that work, and story #599 is where the entry is settled against a
  measurement.
- **The web path gains the same shape for free.** `demo-web` already fetches a
  payload's range and already has to check it against the table
  (`container-parse-reads-a-prefix-through-a-host-reader.md`: "checking a
  fetched payload against the table is the host's own step"). A `Residency` over
  fetched bytes is the same touch + hash + mark ready with a different source,
  and it removes the one rule that record leaves to every host to remember.
- **54 call sites move to `open_verified`.** Mechanical, and each one is a
  question worth asking once: a test that wants the whole file proven keeps the
  eager reader, and a test about _loading_ should be moved to the lazy one.

## Alternatives considered

**A flag on `open`, or a second entry point returning the same slices.** The
smallest diff by a wide margin: no call site changes and no rename. Refused
because nothing in the type system would separate the verified answer from the
unverified one — a caller passing the lazy reader's payloads to a painter would
compile, and the rule "a painter must never receive bytes that have not been
hashed" would be back to being a thing people remember. The prefix record
refused the same shape for the same reason, one function answering two
questions.

**Verify inside `ImageTable::resolve`, so a painter cannot read an unverified
payload.** The strongest guarantee available, and it puts the check exactly
where the bytes are read. Refused because it changes boundary B: `resolve` is
what every painter calls, `assets-borrow-from-the-mapping.md` D3 keeps the
`Painter` trait and every boundary-B type unchanged, and hashing inside a paint
call would put a BLAKE3 pass on the frame thread — the opposite of what this
story is for.

**Keep verifying everything, and make it cheap with a faster hash.** BLAKE3 over
2 MB is already fast, so the number this slice is aimed at is the page faults
rather than the hashing arithmetic. A faster hash does not stop a payload being
read, and R5's claim is about what cold start _touches_.

**Verify lazily at first paint rather than at load.** Would make cold start
smaller still, since a document whose first frame draws no image would hash
nothing. Refused because it moves the fault onto the frame thread, which is
story #597's second stated property, and because
`startup-scaling-is-measured-by-a-counter.md` D3 puts the criterion's boundary
at "a committed arena with the shown root's assets resident" — a benchmark that
ended before the assets were resident would measure zero and prove nothing.

## As built (story #597, 2026-08-07, PR #778)

The shape shipped: `open` reads no payload, a `Residency` proves one, the
prefetch is the shown root's assets, `Plan::bind` keeps only its count check,
and the counter records at the touch. The criterion reached **1.00x** once
story #598's re-run measured the mapped path — 197 387 B out of a one-frame
document and out of a sixty-five-frame one, on macos aarch64.

Three clauses did not survive contact with the code, and each is recorded here
rather than quietly diverged from.

### D1 returns `Wanted`, not `Range<u64>`

D1 specified `Result<(Document<'_>, Vec<Range<u64>>), OpenError>`. It returns
`Vec<Wanted>` — the `{section, range, hash}` triple `prefix::Plan::wanted`
already produced, promoted to the crate root and now returned by both readers.

A bare range cannot carry the two things its consumers need. `Residency` has to
know what a range must **hash to**, and a bare range does not say; deriving it
would mean re-parsing the container the reader just parsed. And the guard in `demo` from issue
(#640) — which refuses a file binding a derived payload, because this host has
no profile to name a rung with — compares the resident hash against the entry's,
and had nothing to compare.

D1's guarantee is unchanged and slightly stronger: a `Wanted` is not bytes, so
a caller still cannot hand one to a painter. What changed is that the two
readers now answer in **one type**, which is what D7 wanted for the proving
step and did not ask for in the reading step.

### D3 takes the bytes rather than holding the region

D3 described a `Residency` that "holds the region and the hashes the section
table declares", with `touch(range)` slicing the region itself. It holds
neither: `touch(want, bytes)` is handed the bytes the caller read.

D7 is why. The browser host has no region to hold — a payload there is its own
HTTP range response, in its own buffer — and D7 says that host "fetches a range
and touches it". A region-holding residency would have needed a second entry
point for the host that has no region, and two entry points for one check is
the shape D7 exists to remove. The proof is the hash either way: bytes that do
not hash to what the section table records are refused whatever slice they came
out of.

### D3's second touch hashes again, and the fast path was a hole

D3 said "a second touch of a ready range returns immediately and does not hash
again". That is sound **only** for a residency that holds the region, because
only such a residency can return the bytes it proved. This one is handed them,
so returning early meant returning bytes nothing had checked — and a `Wanted`
list is one per asset entry and is not deduplicated, so two entries naming one
payload really do touch one blob twice. In the browser host those are two
separate range responses, which need not carry the same bytes.

Found by the review, not by a test. Every touch hashes now. It costs a second
BLAKE3 pass over a payload some document names twice, and no second page fault
— the caller has already read the bytes by the time it calls. Readiness stays
per blob, which is the part of D3 the format's packing rule actually rests on.

### What the counter still cannot see

`load_document_mapped` takes no `LoadCost`, because it reads no payload byte.
So a regression that made the **replay** copy a payload would not move the
number the criterion asserts on. That property is held by an address rather
than a count, in `crates/dashscene-core/tests/mapped_load.rs`: the bytes a
painter resolves out of the arena must be pointers into the mapping, at the
offset the file declares. A copy has equal bytes, so only the address can tell.
Two instruments, two claims, and neither covers the other.

### One thing the record got right that was worth the argument

D2 kept the eager reader as `open_verified` rather than putting a flag on
`open`. Fifty-four call sites moved, nearly all of them tests, and the rename
is most of the story's diff — but the type system now separates the verified
answer from the unverified one, and `dashc`'s CLI, which checks files rather
than drawing them, reads as what it is.
