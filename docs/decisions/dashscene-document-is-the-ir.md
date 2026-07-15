# The dashscene document is the IR; `.dsb` is its file extension

    status   accepted (2026-07-14) — supersedes
             docs/archive/2026-07-14-scope-decisions.md §20 ("The
             IR is named DSB; SCD is retired")
    scope    crates/dashc; every prose reference to the IR
    binds    crates/dashc's type names

## Decision

The intermediate representation is **the dashscene document**. `.dsb` is the
extension of the flatbuffer that serializes it, and nothing more.

In Rust, the in-memory document is `Document` (in `dashc`), and its nodes are
`Node`. The generated flatbuffer types of the same name are aliased `FbDocument`
and `FbNode` where both are in scope.

## Why §20 is overturned

§20 retired the working name `SCD` — correctly — and then named the IR after the
file format, `DSB`. Its argument was that "two names for one thing is a cost this
removes".

The IR and its serialization are not one thing. `.dsb` is one way to carry the
document; the arena in `dashscene-core` is another, and a producer can populate
the arena without a `.dsb` ever existing. Naming the IR after one of its
encodings makes the other encoding read as secondary, and it makes P5 —
"DSB is a schema-first IR with its own spec and validator" — assert that a file
format has a validator. What is validated is the document.

The drift was already visible within a day of §20 landing. Three documents
described the same name and no two agreed:

| Source                                                           | The IR is called | `DSB` expands to    |
| ---------------------------------------------------------------- | ---------------- | ------------------- |
| `docs/archive/2026-07-14-scope-decisions.md` §20, `crates/dashc` | DSB              | —                   |
| `docs/archive/2026-07-14-design-1-seed.md` naming note           | DSB              | "dash scene binary" |
| `docs/technotes/glossary.md`                                     | dashscene        | "dashscene buffer"  |

## What this binds

- `crates/dashc` — the IR type is `Document`, not `Dsb`.
- Every prose reference to the IR — "the dashscene document", or just "the
  document".
- `.dsb` — unchanged.
- `SCD`, `scdc`, `.scb` — already retired by PR #152. Nothing to do.
