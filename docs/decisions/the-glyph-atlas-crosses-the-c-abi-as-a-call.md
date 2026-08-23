# The glyph atlas crosses the C ABI as its own call, sheet included

    status   accepted (2026-08-23), built by story #1123 on the same day
    scope    crates/dashscene-ffi (DsAtlas, ds_runtime_atlas_count,
             ds_runtime_atlas, DsStatus::NoSuchAtlas), crates/dashpaint
             (Atlas::glyphs), unity/com.driftsys.dashscene (TextAtlas,
             TextAtlasSet, the packer's glyph half and the text material)
    related  docs/decisions/the-frame-crosses-under-a-lease.md (the frame this
             is beside, and D4's reasoning for why an Atlas is not a row)
             docs/decisions/glyph-runs-cross-boundary-b.md (the runs that
             name an atlas)
             docs/decisions/glyph-coverage-is-declared-at-build-time.md (why
             this is an upload and not a cache)
             docs/design/c-abi.md (the as-built ABI)
             docs/design/unity-csharp-host.md (the first consumer)

## Context

Story #859 gave a host that draws its own frames the committed tables:
`ds_runtime_acquire_frame` hands out nineteen arrays, two of which are the glyph
runs and their quads. A quad is a glyph id and a pen position, and nothing else.

The **sheet those quads sample did not cross**, and neither did the per-glyph
placement that turns a quad into geometry: `atlas_px`, the rectangle in the
sheet, and `plane_em`, the quad's bounds in ems, both live on
`dashpaint::AtlasGlyph` inside a `dashpaint::Atlas`. The header said so in its
`DsFrame` preamble — "the runs cross and the sheet they sample does not. Until
that lands you can lay text out and cannot shade it" — and
`the-frame-crosses-under-a-lease.md` D4 gave the reason: an `Atlas` is an
encoded payload, four scalars and a glyph list, which is not a row.

So a Unity host could measure and lay out a document's text and could draw none
of it, and `PackDiagnostic.GlyphRun` reported that on every frame that carried a
run.

## Decision

**D1 — the atlas crosses on its own call, not as members of `DsFrame`.**

`ds_runtime_atlas_count` answers how many the loaded document names, and
`ds_runtime_atlas(index)` describes one, keyed by the value a `GlyphRun::atlas`
carries. Three reasons, and the first is the one that decides it:

- **An atlas set belongs to the load, not to the commit.** It is installed by
  `ds_runtime_load_document_with_text` (or a mapped loader with faces) and is
  replaced only by another load. A `DsFrame` member would say it can change per
  tick, which it cannot, and a host would have to invent its own change
  detection to avoid re-uploading a texture sixty times a second. As a call, the
  shape of the API is the advice: read it when a frame reports
  `document_replaced`, and not otherwise.
- **An `Atlas` is not a row.** As frame members it would be a per-atlas entry
  row plus two flat arrays that row indexes — a new boundary-B row type and
  three new `DsSlice` members — for data that is not part of the frame.
- **Adding members to `DsFrame` changes its layout.** A host built against an
  older header allocates the smaller struct and the library writes past it, so
  that route moves `DS_ABI_VERSION` and rebuilds every host. Adding a symbol
  does not, which is the rule at the top of the header, and `DS_ABI_VERSION`
  stays at 2.

**D2 — the sheet crosses too, and that is not redundant with the bytes the host
supplied.**

A host hands the library each face's `atlas_png`, so it may look as though it
already holds every sheet and needs only the glyph table.

**It holds them and cannot tell which is which.** An `AtlasIndex` is the
typesetter's font slot. `dashscene_engine::TextResources::from_faces` builds
that order by grouping the faces **by family** — through
`FontFamily::name_matches`, which trims and compares ASCII-case-insensitively —
and then flattening family-major, so the atlas list is in the flattened order
and not in the caller's argument order. A caller that lists one family's faces
non-contiguously gets an atlas index that names a different face than the same
index in its own array, and pairing by array index uploads another face's sheet:
the glyph ids still resolve, the rectangles are still in range, and the text
draws **the wrong letters rather than failing**.

That hazard is not hypothetical and it is not new prose: the header's own
`DsFontFace` comment names it for the library's internal pairing — "a list in
any other order samples the wrong face RATHER THAN FAILING … including when you
list one family's faces non-contiguously" — and pairs the two there so a caller
cannot get it wrong. `DsAtlas::png` closes the same hole on the host's side.
`an_atlas_index_is_a_font_slot_and_not_the_hosts_face_index` in
`crates/dashscene-ffi` is the measurement: three faces listed Inter, Arabic,
Inter-Bold give atlas 1 = Inter's **bold** sheet, which is the host's face 2.

The cost is a pointer and a length. `dashpaint::Atlas` owns the encoded payload
already, so nothing is copied and nothing is decoded to satisfy this.

**D3 — a new `DsStatus` variant for an index past the set, and never a clamp.**

`DS_NO_SUCH_ATLAS = 20`, the atlas-side twin of `DS_NO_SUCH_ROOT`. Appended at
the tail and reachable only through a call that did not exist before it, so it
is additive in effect as well as in value and `DS_ABI_VERSION` does not move.
Never a clamp, for D2's reason: the nearest atlas is a different face's sheet.

**D4 — no lease, and the lifetime is the load's.**

`ds_runtime_acquire_frame`'s views are invalidated by a commit, which is what
the lease exists to prevent. Nothing here is: the runtime holds the set's `Arc`
from the load that installed it, and a commit clones the same `Arc` rather than
replacing the `Vec`. So neither call takes a lease, neither is refused by one —
a host may read an atlas while its workers are still reading a frame, which is
exactly when a painter uploading a sheet wants it — and the pointers stay valid
until the next load or `ds_runtime_free`.

`a_tick_does_not_replace_the_atlas_set` pins that on the **pointers**, not on
the values: two equal sheets compare equal after a reallocation that invalidated
every pointer a host had cached, which is the failure the claim rules out.

**D5 — `px_per_em` widens to `uint32_t` at the ABI.**

It is a `u16` on `dashpaint::Atlas`. A two-byte member among four-byte ones
costs padding the header would have to name and saves nothing, which is the rule
`sub-word-members-widen-rather-than-pad.md` already applied to `GlyphQuad` and
`AtlasGlyph`. The domain is unchanged.

## Consequences

- **A host reads the atlases once per load and uploads once per load.** That is
  what makes this seam an upload rather than a cache, and it rests on
  `glyph-coverage-is-declared-at-build-time.md`: a `glyph_id` with no
  `AtlasGlyph` draws nothing and there is no runtime atlas rebuild, so there is
  no residency to manage and no eviction to design.
- **`ds_runtime_load_document_with_text` had to gain a managed wrapper.** The C#
  package exposed only the loaders that pass no cascade, so nothing on that side
  could produce a document with glyph runs at all — the atlas seam would have
  had no reachable input. `DashsceneRuntime.LoadDocumentWithText` is that
  wrapper and `TextFontFace` is its descriptor.
- **The glyph table is copied on the way into C#.** `TextAtlasSet` holds managed
  arrays rather than the library's pointers: the pointers outlive a frame lease
  and do not outlive the next load, which is a lifetime a painter holding
  textures across commits cannot honour. An ASCII sheet places about a hundred
  glyphs and the copy happens once per load.
- **One text material per atlas.** A sheet is a texture and a texture is a
  per-material binding, so a document naming two faces mints two materials over
  one shader and the painter emits a draw command per contiguous run of
  instances that share one. The draw-command count already depended on the
  document — a batch splits every 256 visible instances — and this is the first
  thing that makes it depend on **which** nodes those are rather than on how
  many.
- **`DS_ABI_VERSION` stays at 2**, and a package carrying these declarations
  against a library from before them passes the R-E16 handshake and fails at the
  first call — which is the direction adding a symbol has always left open.
  `DashsceneSymbolMissingException` is what `ReadAtlases` turns that into, the
  same rethrow `LoadDocumentMapped(DocumentRange, uint)` already makes.

## Alternatives considered

**The glyph table in `DsFrame`, the sheet left to the host.** The shape the
driver prompt for this story proposed, and it is the one D2 refutes: it is the
half that a host cannot pair up on its own, and the failure is the wrong letters
rather than an error. It also inherits D1's three objections for the table.

**A postcard reader in C#.** The host already holds `atlas_metrics`, and
decoding it would recover the glyph table with no new export at all. Rejected:
postcard is a Rust serialization format and the blob's schema is internal and
unversioned, so this would put a second hand-written decoder of a private format
into the package — the opposite of what boundary B exists for, and a thing that
breaks silently the day the blob's shape moves.

**Decoding the sheet inside the library and handing over texels.** It would
remove the host's decode, and it costs a full RGBA copy of every sheet held for
the life of the document, on a target whose memory is the constraint. The host
has a decoder — `ImageConversion.LoadImage` — and uses it once per load.

**`DsStatus::Atlas` for an out-of-range index.** That variant means "an atlas is
unusable", which is a judgement about the bytes a host supplied at load. An
index past the set is a caller error at a different call, and reusing the
variant would make a host unable to tell a bad sheet from a bad index.

## What is not settled

- **The `px_range` formula has two copies.** `dashscene-gpu`'s `gpu_glyph_run`
  computes `distance_range_px * size / px_per_em` in Rust and
  `TextAtlas.PixelRange` computes it in C#, and nothing compares them — the same
  shape as the heap row widths before `unity/package-gate` held those together.
  Issue #828's portable conformance suite is where the comparison belongs.
- **Nothing has drawn a glyph.** The geometry, the run heap and the atlas lookup
  are executed by `unity/ffi-check` on any pull request whose diff is not
  documentation-only; the material, the texture and the draw commands are
  `Runtime/Engine/`, which only a Unity editor compiles and only a device runs.
- **A document that names more atlases than a host can hold textures for** has
  no bound and no diagnostic. A cascade is a handful of faces today, so nothing
  measures where that stops being true.
