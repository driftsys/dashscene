# `.dsb` is the file extension; one flatbuffer schema serves file and wire

    status   accepted
    date     2026-07-11
    scope    crates/dashbuf, the document's file and wire framing

## Context

`docs/archive/2026-07-14-design-1-seed.md`'s working extension, `.scb`,
needed a name that did not collide with `dashbuf`, the crate that owns
the schema. Separately, `docs/design/architecture.md` and
`docs/roadmap.md` already specify that one flatbuffer schema serves
both the on-disk file role and the wire/remote-streaming role; this
decision re-affirms that against the naming question rather than
changing it.

## Choice

The file extension is **`.dsb`** ("dash scene buffer" — matches the
`dashbuf` crate name).

**One flatbuffer schema serves both roles.** The same tables (node tree,
layout, paint, variant, text) describe intent whether loading a whole
document or applying one staged commit — a remote update is
structurally a small document, not a different data model. What differs
is framing/transport only: the file role uses the mmap section-packing
discipline (hot sections at the head, cold sections page-aligned at the
tail, per-section hashes for the load gate — see
`docs/decisions/dsb-sectioned-container.md`); the wire role skips that
and uses plain length-prefixed flatbuffer message framing. FlatBuffers'
`file_identifier` mechanism (a 4-byte magic tag near the buffer root) is
shared across both roles, so a blob self-identifies as a dashbuf buffer
regardless of whether it arrived via mmap or a socket.

## Why

- `.dsd` ("dash scene document") was considered and rejected: it
  collides with a live, actively used format — Direct Stream Digital /
  SACD audio — plus AutoCAD Drawing Set Description and DAZ Studio morph
  files. `.dsb`'s collisions (Dell DataSafe Backup, an embroidery
  format, a DVD-slideshow project format, a DAZ Studio script format)
  are all dormant or narrow.
- One schema for both roles avoids maintaining two data models for the
  same intent; the file/wire split is a framing concern, not a schema
  concern.

## Consequences

- The self-identifying `file_identifier` also underpins the admission-
  policy question for untrusted remote producers
  (`docs/technotes/open-questions.md`, Q-5).
- `.dsb` is the file extension only. The intermediate representation
  itself is the dashscene document — see
  `docs/decisions/dashscene-document-is-the-ir.md`.
