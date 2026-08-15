# The C ABI's mapped, shown-root-bounded document load — design

    status  written 2026-08-15 against `main` at b357128, and gardened on the
            same branch into `docs/design/host-integration.md`,
            `docs/features.md` and D8 of
            `docs/decisions/the-shown-root-is-named-by-ordinal.md`, with the
            raw original moved to `docs/archive/` in that commit.
            **This line first claimed no decision record changed.** That was
            wrong, and the review caught it: D2 of
            `host-integration-in-three-layers.md` was checked and does record
            only that one C ABI exists — but the conclusion was generalised to
            all of `docs/decisions/` without looking, and D8 of the record
            above said in as many words that the C ABI takes no root and named
            this very issue as what would unblock it.
    issue   #925. Related, and deliberately not taken here: #945.
    epic    #833 (v0.19)

## The problem

`ds_runtime_load_document` takes a whole `.dsb` as `(ptr, len)` and hands every
payload to `dashscene_core::load_document`, the owning loader. Every payload is
copied whether or not anything draws it, and no root is named. R5 — "the shown
root bounds the load" — therefore has no expression at all on the C ABI, while
`dashscene-desktop` expresses it with a mapping and `dashscene-web` with fetched
byte ranges.

The gap is documented as somebody else's: `ds_runtime_load_document`'s own doc
comment says a mapped path "belongs with the platform host that has the file
(story #841)". Story #841 closed without doing it, and `dashscene-android` says
"Where the document comes from … is the embedder's. This crate has no opinion
and no scene registry." So the deferral points at a story that closed and a
crate that disclaims it.

Android's own path makes the cost concrete rather than theoretical.
`Java_dev_driftsys_dashscene_DashsceneNative_nativeSurfaceCreated` takes a
`JByteArray` and calls `env.convert_byte_array`, so the file is read whole into
the JVM heap and copied again into a `Vec` — two whole-file costs — after which
the owning loader copies every payload a third time into the arena's own pool,
for a document of which one artboard is shown.

## What is already built, and is simply not reached

Nothing in this design is new machinery. `dashscene_core::load_document_mapped`
takes a region and a range per asset and copies no payload byte;
`dashbuf::map::MappedFile` maps a path; `dashbuf::prefetch` resolves a
`ShownRoot` ordinal to a root and lists the assets that root's subtree draws;
`dashbuf::residency::BlobResidency` proves one payload at a time.
`dashscene_desktop::document::Document::load` is a complete worked recipe over
all of it, and `dashscene-ffi` already depends on every crate involved — its
`Cargo.toml` describes its dependency set as "the same set `dashscene-desktop`
takes, minus `winit`".

What is missing is an entry point.

## Decisions

**D1 — the mapped entry point takes a filesystem path.** `const char *path`,
NUL-terminated UTF-8, mapped with `MappedFile::open`. This mirrors the desktop
loader exactly, adds no code to `dashbuf`, and is portable to the iOS and Unity
hosts that follow.

It does not reach an asset compressed inside an APK; an Android host extracts to
app storage first, or waits for a descriptor-taking variant. That deferral is
cheap by this ABI's own versioning rule — a new symbol does not move
`DS_ABI_VERSION` — so the fd form can be added the day a host needs it, without
disturbing this one.

**D2 — one new symbol, not a mapped/text pair.** The existing
`ds_runtime_load_document` and `ds_runtime_load_document_with_text` are a pair
for a historical reason: story #947 added the second after the first had
shipped. A symbol written now can carry the nullable-faces rule from the start,
so the mapped path does not double into four symbols.

```c
DsStatus ds_runtime_load_document_mapped(DsRuntime *runtime,
                                         const char *path,
                                         uint32_t shown_root,
                                         const DsFontFace *faces,
                                         size_t face_count);
```

`faces == NULL` or `face_count == 0` loads without text, exactly as
`ds_runtime_load_document_with_text` documents.

**D3 — `shown_root` is required, not optional.** Mapped implies bounded. There
is no sentinel for "every root", because a caller who wants every root already
has the two owning symbols and should pay the owning cost knowingly. This also
keeps the ABI from acquiring a bound that is not one, which the module docs
already argue against for the rejected shape of a `ShownRoot` parameter on the
owning call.

**D4 — the mapping's lifetime is the arena's, and the C caller has no rule to
keep.** `load_document_mapped` hands the arena its own `Arc<dyn Region>`, and
`load_into` installs a fresh `Arena` per load, so the previous mapping unmaps
when the previous arena drops. `DsRuntime` gains no field. This is the property
that made the path form preferable to a caller-supplied `(ptr, len)` region,
where "keep this mapping alive until the document is replaced" would have been a
contract enforced only by prose across an FFI boundary.

**D5 — the shared tail moves into `dashscene-core`.** The load recipe is already
written twice, in `dashscene-desktop` and `dashscene-web`, down to a `show_root`
panic message that is near word-for-word identical in both. A third copy in
`dashscene-ffi` would put the `roots_before` correction from issue #943 — itself
a bug fix — in three places, so the next correction has three sites to find.

Two functions move to `crates/dashscene-core/src/load.rs`:

- `first_derived_payload(doc, wanted) -> Option<u32>` — the index of the first
  asset entry whose bound payload hash disagrees with the entry, meaning the
  file carries derivations this crate cannot name a rung for. Returning the
  index rather than an error lets each host name its own source: a path, a URL,
  or a path again.
- `show_appended_root(doc, shown_root, roots_before, source, arena)` — the
  `roots_before + ordinal` correction, the `show_root` commit, and the
  diagnostic that fires when `dashscene-core`'s own one-arena-root-per-document
  -root promise is not met. `source` is what the diagnostic names.

The residency walk deliberately stays per-host: how payload bytes arrive is the
one step that genuinely differs (a slice of a mapping, against a buffer packed
from fetched ranges), and forcing those together would be a false unification.

**D6 — four status variants, appended at the tail.** `DsStatus` is explicitly
numbered and `Atlas` is 10, so nothing renumbers and `DS_ABI_VERSION` stays 1.

    Map = 11        the path is missing, unreadable, empty, or not UTF-8
    NoSuchRoot = 12 the ordinal names no root in this document
    Derived = 13    the payloads are derivations, refused rather than bound
                    as canonical
    Payload = 14    an asset the shown root draws failed its residency proof

`NoSuchRoot`'s message carries the ordinal asked for and the count the document
does carry, which is what the desktop error type reports and what a host needs
to tell an out-of-range ask from an empty document.

`Derived` refuses rather than binds. The owning path finds a mismatched format
by parsing the payload's header; this path reads no header by design, so nothing
downstream would catch a KTX2 tagged as a `Png` — the mistake issue #640 exists
to prevent. This crate ships no profile and cannot name a rung, so refusing the
file is the honest answer.

## What proves the bound

Not that the call returned `Ok`. The mapped loader takes no `LoadCost` on
purpose — it reads no payload byte, so a counter there could only ever report
zero — which means the bound has to be proven behaviourally.

**One fixture does it in both directions: a two-root `.dsb` whose roots draw
disjoint assets, with one root's payload corrupted by a single byte.** This is
the fixture shape `dashscene-desktop`'s own tests already build, so it is not
new ground. The disjointness is load-bearing rather than incidental: a shared
asset would be touched under either bound and the first assertion below would
prove nothing.

- Loading bounded to the **healthy** root succeeds. It can only succeed if the
  corrupt root's payload was never touched, because `BlobResidency::touch` would
  have failed on it. That is a positive proof that the load is bounded.
- Loading bounded to the **corrupt** root returns `Payload`. That is the proof
  that the residency check is reached at all — without it, the first assertion
  would also pass if nothing were ever verified.

Neither assertion can pass if the other's premise is wrong, which is what makes
the pair falsifiable rather than merely green.

Beside it: a null `path` is `NullArgument`; a path that does not exist is `Map`;
an ordinal past the last root is `NoSuchRoot` and its message names the count;
and the loaded arena's shown root is the one asked for. The committed C header
is compiled from C by `just check`'s `c-abi` gate, which is what holds the
header and the Rust half in agreement.

## Documentation this makes false, and therefore changes

Both are load-bearing, and leaving either would be prose asserting what the code
does not do:

- The module docs' closing argument — "It is not built here because that is a
  signature change on a shipped symbol … the shape that costs nothing is the
  shape that also bounds the load, and doing them together is why neither is
  here yet". The shape it describes is what this builds, so the paragraph
  becomes a description of what exists.
- `ds_runtime_load_document`'s "A mapped load belongs with the platform host
  that has the file (story #841)", in the Rust doc comment and in the header.
  This is the stale pointer issue #925 was filed about.

## Alternatives considered

**A descriptor-taking entry point (`fd`, offset, length).** Rejected for now,
not on merit: it is the right shape for an uncompressed asset inside an APK, via
`AAsset_openFileDescriptor`. It needs a new `MappedFile` constructor,
page-alignment handling for the offset, and a unix-only `cfg` — real new native
code whose only exerciser is the platform still waiting on hardware (#885).
Under this ABI's versioning rule it stays free to add later, so building it
before a host asks would be building ahead of the plan.

**A caller-mapped `(ptr, len)` region the runtime borrows.** Rejected. It needs
no new `dashbuf` code and works for any source a host can map, but it turns the
mapping's lifetime into a C contract enforced by prose, and requires an `unsafe`
`Send + Sync` `Region` over a caller's pointer. D4 is the direct answer to it.

**A `ShownRoot` parameter on the existing owning symbol.** Rejected, and the
module docs rejected it first: it would be accepted, change nothing measurable
because the owning loader copies every payload regardless, and read as a bound
that is not one. It would also bump `DS_ABI_VERSION` for no gain.

**Mirroring the recipe in `dashscene-ffi` without extracting.** Rejected under
D5.

**Taking #945 in this change.** Rejected, on evidence that corrects the driver
prompt — see below.

## Why #945 is not taken here

The driver prompt states that #945's stale-upload defect "stays latent only
because the ABI names no shown root, so the moment #925 lands it becomes real on
Android". Checked against the code, it does not.

`CommittedScene::renumbered` is `arena.shown_root != previous_shown_root` and
nothing else. `dashscene-ffi` calls `show_root` nowhere — the only matches for
that vocabulary in the crate are `ShownRoot` in its module documentation, in the
paragraphs arguing for this change — and `ds_runtime_tick` only calls
`scene.tick`. So after this change the shown root is named once, inside the
load, and there is no symbol by which a host can change it afterwards — which
means `renumbered` can fire only on the load's own commit, and `load_into`
already calls `surface.document_replaced()` immediately after the load, in that
order.

The defect would go live if the ABI gained a root-switching symbol. This design
does not add one. #945 is therefore what it always was — a rule written twice
and worth stating once — and it is judged on that merit in its own change rather
than carried in here as a bug fix it is not.

## Out of scope

- The Android JNI counterpart, and `demo-android` switching to it. Filed as debt
  against a milestone. The one platform that would execute it is waiting on
  hardware (#885, #842), so every claim made about it here would be a
  compile-time claim.
- A descriptor-taking variant.
- Changing the shown root after load.
- #945.

## Verification

    just test        while editing
    just build       the regression tier, before pushing — quote its Summary
    just check       adds the c-abi gate, which compiles the committed header
                     from C and checks the two halves agree
    just android     dashscene-ffi is Android's ABI, so its cross-compile is
                     part of this change's gate where an NDK is present

`crates/dashscene-core/src/load.rs` is touched, so whether the `packer` path
filter in `.github/workflows/ci.yml` selects it decides if `just calibrate` is
owed before merge. That is checked against the filter itself rather than
assumed.

## Trace

Satisfies R5 on the C ABI path, partially: this bounds the **load**. The
per-frame bound arrived with story #838 and is not re-litigated here.

Refs #925. Refs #945. Refs #947. Refs #841. Refs #838. Refs #943. Refs #640.
