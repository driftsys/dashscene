# Blob verification moves off `open` and onto the touch that makes a payload resident

    status   accepted (2026-08-06), before implementation — nothing here is
             built. Story #597 builds it; story #599 records the as-built
             result against this record rather than replacing it.
    scope    `dashbuf::open`'s contract, the residency step that replaces the
             verification it drops, and what the two hosts do differently as a
             result. Boundary B is untouched: `ImageTable`, `ImageEntry` and
             the `Painter` trait are exactly what story #596 left them.
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

`dashbuf::open` resolves every asset entry through
`Container::blob_by_hash`, which calls `verify_section`, which BLAKE3-hashes the
whole payload. Opening a document therefore reads every byte of every asset. On
a mapping that faults every page of the file in, which is the exact cost mapping
it was supposed to avoid, and it happens before anything is known to be needed.

The format was built to prevent this and says so. `container.rs`'s own module
doc: payload hashes are "checked on demand by `Container::verify_section` so
that a caller verifying only the hot sections never faults a cold page".
`Container::verify_hot` exists, verifies every structured section and "touches
no blob payload". `open` does not call it.

So the eager verification is not an oversight in the format. It is one call in
one function, and moving it is the whole of what stands between the slice and
its criterion.

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

**D5 — `madvise` per range, native only, and advisory in the strict sense.**
`memmap2::Mmap::advise_range(Advice::WillNeed, offset, len)` is issued for each
range about to be touched, and its failure is not an error: `advise` is `#[cfg(unix)]`,
it is a hint the kernel may ignore, and a payload that is hashed successfully is
resident whether or not the hint was taken. A failing hint must not fail a load.

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

**D7 — the counter records at the touch.** `LoadCost::record_hashed` moves from
`open_with_cost` to `Residency::touch`, so the number story #598 asserts on is
what was actually made resident rather than what was resolved. A payload
resolved and never touched costs nothing and is counted as nothing, which is the
claim R5 makes.

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
