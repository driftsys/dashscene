# The document is mapped where it is packed, not copied out first

    status   accepted and built by story #1124 on 2026-08-23, and the whole
             path executed on a device that day. The Android facts below are
             measured rather than read off the platform documentation; the
             paragraph that states each says which run produced it.
    scope    crates/dashbuf (MappedFile::open_range),
             crates/dashscene-ffi (ds_runtime_load_document_mapped_range and
             the header that declares it),
             unity/com.driftsys.dashscene (DocumentRange, the loader over it,
             and StreamingAssetDocument, the Android resolver)
    related  docs/specification/01-goals-and-requirements.md R5, which this
             is what keeps on Android
             docs/design/dsb-container-format.md (the alignment policy the
             cost below is measured against)
             docs/design/c-abi.md (the surface this adds a symbol to)
             docs/design/unity-csharp-host.md (the managed half, as built)
             docs/decisions/assets-borrow-from-the-mapping.md (why a mapped
             load is bounded at all)

## Context

R5 is a hard requirement: **cold-start cost proportional to what is shown, not
to file size**, and the parenthesis in the requirement names the mechanism —
"mmap + section discipline". `ds_runtime_load_document_mapped` is the C ABI's
expression of it, and it takes a `const char *path`.

**A `.dsb` shipped in a Unity Android build has no path.** Unity puts everything
under `Assets/StreamingAssets/` into `assets/` in the APK, and
`Application.streamingAssetsPath` resolves to
`jar:file:///data/app/<pkg>/base.apk!/assets` — a URI into a zip container. The
mapped loader maps a real file, so it answers `DS_MAP` there and the frame-loop
sample disabled itself. With the sample's shipped default of
`documentPath = "scene.dsb"` that was what an Android user got on first run
(issue #1288).

Issue #851's open question 2 is the general form: **you cannot mmap through a
container.** Separate what mapping buys, because only one half was at risk —
zero-parse is flatbuffers and survives any backing memory; **demand paging is
what a container destroys**, and it is the half R5 names.

## What was measured before anything was designed

A throwaway Unity project on `6000.3.22f1`, one `.dsb` in `StreamingAssets`,
IL2CPP, ARM64, `minSdkVersion` 33, built in batchmode and run on an `arm64-v8a`
Android 14 automotive emulator. Four facts, each of which a design here would
otherwise have had to assume:

- **Unity stores a `StreamingAssets` file uncompressed, by default and with no
  gradle template of one's own.** Its shipped `mainTemplate.gradle` sets
  `noCompress = **BUILTIN_NOCOMPRESS** + unityStreamingAssets.tokenize(', ')`.
  In the built APK `assets/scene.dsb` was `Stored`; every other `assets/` entry,
  all of them Unity's own, was `Defl:N`. This matters because
  `AssetManager.openFd` refuses a compressed entry — it is the one build setting
  the design below depends on, and a stock build already satisfies it.
- **`openFd` reports a start offset and a length, and they are the entry's real
  position.** It answered `startOffset=24073616 length=4189`, and parsing the
  APK's local file header from the host gave a data offset of exactly 24073616
  for the same entry.
- **That offset is not page-aligned.** 24073616 is 1424 past a 4096-byte
  boundary. `zipalign` aligns an ordinary stored entry to 4 bytes and
  page-aligns shared objects only, so this is the normal case rather than an
  unlucky one.
- **The process can open the container by path and read the document there.**
  `Application.dataPath`, `ApplicationInfo.sourceDir` and the canonical path of
  `/proc/self/fd/<n>` for the descriptor `openFd` returned were all the same
  `base.apk`, and a `FileStream` on it seeked to the start offset and read the
  `.dsb` magic. **This is the fact that decided the ABI's shape**: a file
  descriptor does not have to cross the boundary.

## Decision

**D1 — the ABI gains a byte range, not a file descriptor.**

`ds_runtime_load_document_mapped_range(runtime, path, offset, length,
shown_root, faces, face_count)`.
The library opens the container itself and maps `length` bytes at `offset`.

The alternative considered first was fd + offset + length, which is what
`AAsset_openFileDescriptor` and `openRawResourceFd` hand a native caller. It was
rejected on two grounds, and the second is the one that settles it:

- **A descriptor makes the caller's ownership part of the contract.** Who closes
  it, and whether the mapping outlives it, becomes prose across a C boundary.
  With a path, the loader's existing rule — "the mapping is the runtime's, and
  the caller keeps no lifetime rule" — carries over unchanged.
- **`int fd` is not portable to the one platform in scope that is not POSIX.**
  `the-native-library-ships-inside-the-unity-package.md` D3 assigns a
  `dashscene_ffi.dll` to Windows editor and standalone, where the platform's
  file handle is a `HANDLE` and a C-runtime descriptor depends on which CRT the
  caller linked. A `const char *path` has no such split, and the measurement
  above says Android does not need one.

**D2 — a separate symbol, not a widened signature.**

`ds_runtime_load_document_mapped` is unchanged. By the versioning rule at the
top of `crates/dashscene-ffi/include/dashscene.h`, adding a symbol is free and
changing a signature is not, so `DS_ABI_VERSION` stays **2** and no host built
against an older header is affected. It adds no `DsStatus` variant either: every
failure it reports is one the whole-file loader already reports.

**D3 — a range the file cannot satisfy is refused, not mapped.**

Zero length, a range ending past the end of the file, and an offset that
overflows when the length is added are each `DS_MAP`, raised before anything is
mapped. `mmap` past the end of a file **succeeds** and answers `SIGBUS` when the
page is touched, which arrives with nothing naming the range that caused it.

There is no sentinel length meaning "to the end of the file", for the reason
`shown_root` has none meaning "every root": a caller that wants the whole file
has the other entry point, and a bound that can be switched off reads as a bound
when it is not one.

**D4 — the alignment loss is accepted and named, because it is a cost and not a
defect.**

**Two different alignments live in that format, and only one of them is
optional.** `docs/design/dsb-container-format.md` calls section alignment writer
policy — 64-byte sections, a page for the first blob, a page of its own for a
blob of 64 KiB or more — and a reader deliberately does not enforce any of it,
"because alignment is writer policy and a reader that enforced it would freeze a
heuristic the format leaves open". But the **hot/cold boundary is required**,
and it is required for a stated purpose: "so a load gate can verify everything
it needs to lay out and paint without faulting a single cold page". That is not
decorative. `dashbuf::open` calls `Container::verify_hot` on every load,
including this one.

**A container offset shifts that boundary, and so does an ordinary whole-file
mapping on most hosts.** The format's quantum is the constant
`dashbuf::container::PAGE_ALIGN`, which is **4096** — a property of the file,
not of whoever maps it. The machine this was built on has 16384-byte pages, and
Android 15 requires 16 KB page support on new devices, so on any host whose
pages are **larger** than 4096 a 4096-aligned boundary can already sit mid-page
at offset 0 and `verify_hot` already faults up to a page of cold bytes. A host
whose page size divides 4096 does not have that, which is why the claim names
the direction.

Write the boundary's own file offset as `B` — a multiple of `PAGE_ALIGN` that
depends on how large the document's structured sections are, and **not** 4096
itself. Mapping at a container offset moves the shift from `B mod host_page` to
`(offset + B) mod host_page`. It is the same quantity, differently arrived at,
and neither is zero in general.

So **correctness is unaffected** — nothing reads alignment, and `madvise` is not
called, so nothing depends on a blob occupying whole pages — and the cost is
bounded at **one host page at the hot/cold boundary plus one per large-blob
boundary**. What this decision gives up is the guarantee's exactness, on hosts
that already did not have it.

**D5 — the managed half splits at which gate can reach it, and nothing of it is
in the sample.**

`DocumentRange` and `DashsceneRuntime.LoadDocumentMapped(DocumentRange, uint)`
are in `Runtime/`'s engine-free half, where `unity/ffi-check` builds a container
with the document at a deliberately unaligned offset and loads it against the
real library on every pull request. `StreamingAssetDocument` — the JNI query
that asks the APK where the entry is — needs `UnityEngine`, and lives in
`Runtime/Engine/`, which `just unity-editor` compiles
(`r-e10-is-checked-in-two-halves.md` D2 and D3).

**It was in `Samples~/FrameLoop` when this story wrote it, and that was wrong by
the time the story merged.** `Runtime/Engine/` did not exist then: R-E10's check
could not compile a `UnityEngine` reference at all, so a sample nothing compiled
was the only place a JNI query could go. Story #1122 landed the two-halves
ruling in the same slice, which both created the right home and made the old
reasoning false — so the resolver moved before this branch merged, rather than
shipping a package whose Android path a customer had to copy out of a sample.

What is left in the sample is `Time.deltaTime`, a component lifecycle and where
the painter hangs. **The compile is a gate; the behaviour is not** — whether
`openFd` reports the offset an APK actually holds is answered by the device run
recorded below, and that run does not repeat itself when someone edits the file.

## Alternatives considered

Issue #1288 named three shapes and issue #1124 named a fourth. All four were
costed against the measurement above rather than against expectation.

- **Extract to `Application.persistentDataPath` on first run, then map that.**
  Needs no ABI change at all, which is its whole appeal. It costs a full copy of
  the file on first run — which is R5's cost, on the run where a customer is
  least tolerant of it — and a second copy of the document on disk for the life
  of the install, since the APK's own copy is uncompressed and cannot be
  reclaimed. It also needs an atomic write and a content key so an app update
  does not leave a stale copy, none of which is checkable by any gate here.
  **Rejected**: it is more managed machinery than D1 and it gives up the
  property the story exists to keep.
- **Play Asset Delivery with file storage**, which gives a real path and no
  copy. **Rejected on the target rather than on cost.** `AGENTS.md` defines the
  target as embedded display hardware — in-vehicle, industrial and medical
  panels, kiosks, avionics — which are sideloaded or preinstalled. Making the
  one supported Android path depend on a store the target does not use is the
  wrong dependency, whatever it costs.
- **`LoadDocument(byte[])` on Android only.** Gives up demand paging, which is
  the thing story #1124 exists to keep, and adds a copy of the whole file onto
  the managed heap on top. **Rejected**: it violates R5 by construction.
- **Replace the load seam with a range-reader**, so that mmap, an HTTP byte
  range and a container entry become implementations of one thing. Issue #1124
  calls this "the change that would dissolve the question", and it is right that
  it is the larger one and the one that does not have to be redone for the next
  host — `dashscene-web` already loads a `.dsb` by byte range through
  `dashbuf::prefix`. **Not taken here, and not rejected**: it is a refactor
  across `dashbuf`, both integration crates and the C ABI, and D1 does not
  foreclose it. `open_range` is where a seam would land.

## What decides it, and the measurement issue #1124 asked for

Issue #1124 says the choice turns on "whether documents are large enough for
demand paging to matter", and that this slice can measure it.

**The measurement, taken and recorded honestly: every `.dsb` in this repository
is smaller than one page pair.** The eleven committed goldens run from 692 to
4345 bytes, and the two `dashbuf` fixtures are 1644 and 4108. At that size
demand paging buys nothing — the whole document is one or two faults either way,
and every one of the four shapes above performs identically.

**That does not decide it, and treating it as the deciding measurement would be
measuring the fixtures rather than the product.** R5 is a hard requirement
stated over documents, not over the corpus that happens to exist while v0 is
being built, and it names mmap as its mechanism. What the measurement does
settle is narrower and worth writing down: **nothing here yet exercises the
property, so no number in this repository can be quoted as evidence that the
chosen path is faster than option 1.** The argument for D1 is that it is the
only shape that keeps R5 with no copy, no duplicate storage and no store
dependency — not that a measurement here separates it from the others. A
document large enough to separate them is a v1 measurement, and issue #1107's
device runs are where it belongs.

## The device run

The whole design executed on a device, on the shipped default. A Unity Android
build carrying this branch's package and its `Samples~/FrameLoop` component,
`documentPath = "scene.dsb"` unchanged, on the same `arm64-v8a` Android 14
automotive emulator.

**Taken twice, and the second is the one that counts.** The first run went
through a resolver in `Samples~/FrameLoop`; D5's move put it in
`Runtime/Engine/StreamingAssetDocument.cs`, so the first run became evidence
about a code path that no longer exists. Both produced the same numbers. What is
below is the second.

**The capture follows from before the launch rather than dumping the ring
afterwards**, and that is not fastidiousness: `adb logcat -c` is routinely
refused on Android 11 and later, and a bare `-d` replays whatever the ring still
holds — so a dump taken after a failed clear can return the _previous_ run's
markers, which for this verifier differ only in one random path segment. The run
asserts it saw exactly one `player-start`, and the container path below carries
an install hash that is this run's:

    resolved container=/data/app/~~2zq07HPDQMFpExnI_QE-kA==/com.driftsys.dspackprobe-...==/base.apk
    resolved offset=24073616 length=4189 wholeFile=False
    resolved container-is-apk=True
    resolved offset%4096=1424
    loaded rects=14 paint_entries=14 image_payload_bytes=4189 generation=3
    loaded payload-is-the-range=True
    no-copy persistentDataPath-dsb-count=0 []
    frameloop enabled=True after 30 frames

Five things line up there, and each is a different claim:

- **The resolver produced the right range** — the same 24073616 and 4189 the
  probe read out of the APK, this time from `StreamingAssetDocument.Resolve`,
  which is code the package ships rather than code a customer would copy.
- **A non-page-aligned offset loaded.** 1424 bytes past a page boundary, so D4's
  argument that this is a cost rather than a defect survives contact.
- **The document was mapped, not copied.** `image_payload` is the whole mapped
  region for a mapped load, and it is 4189 bytes — the entry's length exactly,
  neither the 35 MB APK nor an owned copy of anything.
- **Nothing was extracted.** No `.dsb` anywhere under
  `Application.persistentDataPath`, which is the property option 1 gives up.
- **The frame-loop sample stayed enabled**, which answers issue #1288 on the
  case that filed it: the shipped default, on first run. It disables itself on
  any failure, so "still enabled 30 frames later" is an observation rather than
  the absence of an error line.

**What this does not say.** It is an emulator and not target hardware, taken in
the emulator's default GPU mode. The document is 4189 bytes, so it measures
correctness and not the paging behaviour D4 discusses. And a split APK is still
untested.

## Consequences

- **A Unity host loads a `StreamingAssets` document on Android with no copy**,
  and the frame-loop sample's shipped default now works there rather than
  disabling itself.
- **The ranged loader is not Unity-specific and not Android-specific.** Any host
  whose document is packed inside a larger file reaches it — an iOS app bundle
  in v1, a game archive, `demo-android` if it ever loads a `.dsb`, which today
  it does not.
- **A custom `mainTemplate.gradle` that drops `unityStreamingAssets` from
  `noCompress` breaks the Android path.** `AssetManager.openFd` throws for a
  compressed entry, the sample's resolver lets that exception through, and the
  host reports it rather than falling back to a copy. A fallback was considered
  and not written: it would make the fast path silently optional.
- **A split APK is untested.** `AssetManager` serves an asset out of whichever
  APK holds it, and the resolver reads the container off `/proc/self/fd/<n>`
  rather than assuming `Application.dataPath` precisely so that case is right —
  but the measurement above is a single-APK install, so that is a design for the
  case rather than a reading of it.
- **The page-alignment loss is real and unmeasured.** D4 argues it is bounded at
  one page per blob boundary; no number here says what it costs on a document
  large enough to have many.
