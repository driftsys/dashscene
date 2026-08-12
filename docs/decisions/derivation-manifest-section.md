# The canonical-to-resident mapping is its own section

Status: accepted, v0.12 story #434.

Traces: `docs/decisions/asset-model-content-addressed-blobs.md` (what a
canonical hash means), `docs/decisions/dsb-sectioned-container.md` and
`docs/design/dsb-container-format.md` (where the bytes go),
`docs/decisions/asset-quality-profile-naming.md` (what RAW is).

## The problem

An asset has one canonical payload and one canonical hash. A quality profile
binds that hash to the bytes a file actually carries. Under RAW — the null
binding — those are the same bytes, so a reader finds a payload by looking for
the blob section whose own content hash equals the entry's hash. That is what
`Container::blob_by_hash` does.

Under any production profile they are not the same bytes, by definition:
`AssetEntry.hash` is the BLAKE3 of the original PNG, and the file carries a KTX2
file the packer derived from it. The two hashes differ, so the lookup finds
nothing. A bank of derived payloads assembled correctly and produced a file no
reader could load.

`SectionEntry` is a fixed 64-byte stride with 12 reserved bytes and carries no
second hash, so there was nowhere for the mapping to already be.

## The decision

The mapping lives in a **derivation manifest**: a `dashbuf.AssetBindings`
flatbuffer in its own structured section, flavor `FLAVOR_BINDINGS`, holding one
`canonical`/`resident` hash pair per binding that is not the identity.

Four properties follow, and each of them is why this was chosen over the
alternatives below.

**A RAW file has no manifest section, so no committed byte moved.** A row is
written exactly where `blake3(resident)` differs from the canonical hash. RAW is
the identity map, so it produces no rows, and a manifest with no rows is not
written at all. All 70 committed binary artifacts in the repository are
byte-identical across this change, verified by `git hash-object` before and
after.

**`assemble` needs no new parameter.** The mapping is a function of the bank
alone — the bank holds canonical hashes and payloads, and hashing the payloads
answers which bindings are derived. So the manifest is an _output_ of assembly,
not a second input to it, and `assemble(ui_section, bank)` keeps its signature.
There is also no separate "is this RAW" flag to get wrong: a bank built through
`ColdBank::derived` over canonical payloads is the same bank as `ColdBank::raw`,
and assembles to the same bytes.

**An `AssetEntry` still names a hash and never a section index.** Both sides of
every manifest row are hashes too. Nothing in the document, and nothing in the
manifest, learns where a payload sits, which is what keeps hot sections
assembly-invariant (`asset-model-content-addressed-blobs.md`).

**`container` stays parser-free.** That module exists to validate a file before
any parser is trusted, so it cannot depend on one. It returns the manifest
_bytes_, verified against the section table; reading rows out of them is
`dashbuf::open`'s step, one layer up. `blob_by_hash` is unchanged and still
resolves by content hash — the manifest is a step before it, not a change to it.

### What it costs

The ui section moves. A derived file has one more section-table entry, so its
payloads start one 64-byte stride later than under RAW. The v0.11 doc comment
claiming "even the ui section's offset is the same" across banks was true only
because the asset count fixed the section count, and it is now false. The
guarantee that survives is the one the format actually promises: the ui
section's **bytes** are identical, and no reader depends on its address, because
nothing names an offset.

Assembly hashes each resident payload once more than before, to decide whether a
row is needed. `container::write` hashes the same bytes again. That is one extra
BLAKE3 pass over the cold region on the write path only, against the image
decode and encode the same step already pays for.

## Alternatives rejected

**Append the binding rows to `Document`.** The cheapest change, and the worst.
It makes the hot section vary with the profile, which destroys the very
invariant this slice was sequenced to measure, and it puts a packer _result_
into a document that carries only intent (P1). Two files of one design under two
profiles would no longer share a document.

**Widen `SectionEntry` to carry a second hash.** `SECTION_STRIDE` is pinned by a
compile-time assertion and checked by every reader, so this is an envelope
version bump: every existing `.dsb` becomes unreadable and every committed
golden moves — to carry a field that is meaningless under RAW, which is the only
profile shipped today. It also puts a profile concept into the envelope, which
is deliberately profile-blind.

**Use the 12 reserved bytes in `SectionEntry`.** A 64-bit truncation of a
256-bit content address weakens the identity for no gain, and writing meaning
into a reserved field is exactly what `ContainerError::ReservedNotZero` exists
to refuse. A canonical hash is a property of a _binding_, not of a section: the
same section could be the resident payload for two canonical identities.

**Redefine `SectionEntry.hash` as the canonical hash.** It is what
`verify_section` compares against, so the file would lose its own integrity
check on cold payloads — which the R5 load gate depends on. Identity and
integrity are two jobs and one 32-byte field cannot do both.

**Put the manifest in a cold blob section at the tail.** A reader needs it to
resolve any asset at all, so it would fault a cold page at load, which is the
one thing the page-aligned hot/cold boundary exists to prevent. It also would
not have avoided the ui-section shift: the table grows either way.

## What a reader must refuse

The envelope exists precisely to not trust the writer, so `open` refuses rather
than tolerates:

- more than one manifest section (`NotOneBindingsSection`) — two answers to one
  question;
- a row whose canonical or resident hash is not 32 bytes
  (`OpenError::BindingHashLength`) — it can never match anything, so accepting
  it is a claim silently doing nothing (P4);
- a row repeating a canonical hash an earlier row bound
  (`OpenError::BindingRepeated`) — resolving the first would be a silent choice
  between two claims.

A row naming a canonical hash no entry uses is **not** refused, and that is
deliberate: resolution is keyed by the entry's hash, so a row nothing asks for
answers nothing and cannot misdirect anything.

The manifest's own bytes are covered by its section's content hash, and the
section table by the header's root hash, so redirecting a canonical hash at
different bytes is not something that can be done quietly.

On the write side, `assemble` refuses a bank that binds one canonical hash to
two different payloads (`AssembleError::ContradictoryBinding`). One file cannot
carry both under one identity, so writing it would drop a claim silently. It is
unreachable from `ColdBank::raw` by construction — there a payload is bound to
its own hash, so two bindings sharing a hash are two identical payloads — which
is why it is checked on the bank rather than on the file, and why `dashc`'s
`expect` remains sound.
