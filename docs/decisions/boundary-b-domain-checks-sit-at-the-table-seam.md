# A boundary-B row's domain check sits at the table push, not on the type

    status   accepted (2026-08-15)
    scope    dashpaint's #[repr(C)] boundary-B rows and the tables that hold
             them; issues #985 and #986

## Context

Two operands reach a painter through boundary B carrying a domain nothing
enforced.

`GlyphQuad::glyph_id` is one half of the widening
`sub-word-members-widen-rather-than-pad.md` records. That record traded a
type-level guarantee — a `u16` made an out-of-domain glyph id unrepresentable —
for a checked one, and issue #966 put the check in `Atlas::new`. It covers
`AtlasGlyph` only. The drop the record describes is on the other side: a row
`Atlas::new` accepted is _found_ by `Atlas::glyph`'s binary search and paints,
while a **quad** naming an id no atlas has a row for is what both painters
`continue` past with no diagnostic (P4).

`VectorField::distance_range` is the coverage-mask copy of the operand that
issue #964 guarded on `Atlas`. Every painter derives a range from it, and each
way out of the domain paints a plausible wrong picture rather than nothing: zero
gives uniform half coverage, a negative value inverts it, and a NaN or an
infinity reaches the implementation-defined WGSL `clamp`.

Both issues left the same question open, and #986 stated it: neither type has a
constructor, so there is no seam a check sits at today.

## Decision

**The check goes at the table's push method — the one seam a row passes through
to reach a painter — as a documented panic, in the shape the refusal already
there uses.**

- `GlyphRunTable::push_run` refuses a quad whose `glyph_id` exceeds `u16::MAX`.
- `PaintTable::push_with` refuses a `parts.shape` whose `distance_range` is not
  finite and greater than zero.

Neither type gains a constructor and no field changes visibility.

## Why not a checked constructor

The two types reach this answer by different routes, and only one of them is a
hard obstacle.

**For `GlyphQuad` a constructor is unavailable.** A checked constructor holds
only if the fields are private, and `glyph_id` cannot be:
`neither_glyph_type_carries_padding` — the test
`sub-word-members-widen-rather-than-pad.md` names as what has teeth — reads
`offset_of!(GlyphQuad, glyph_id)` and `size_of_val(&quad.glyph_id)` from
`dashscene-unity`, another crate. Private fields make both a compile error, so
the constructor would be bought by deleting the assertion that holds the
widening this check exists to complete.

**For `VectorField` it is available, and the reason against it is cost.**
Nothing pins that type's field offsets — `dashscene-unity` measures only its
size and alignment, which private fields do not change — so private fields plus
accessors would work. They would mean rewriting every literal and every read of
the type outside `dashpaint`: both painters read the fields directly on the draw
path, the loader builds one per shape, and the arena folds `distance_range` into
a pool key. That is a real trade rather than an impossibility, and it is
recorded as one.

Re-derive that spread with `grep -rlw VectorField --include=*.rs` rather than
trusting a count here: an earlier draft of this record gave one, and it was
wrong twice — once counting the packages that merely _name_ the type, once
including a crate whose only mentions are comments.

**What is not a reason.** An earlier draft of this record argued that both rows
cross the C ABI by value through `dashscene-unity`'s `abi_surface!`, so a
foreign host writes the bytes and no constructor is on that path. That is false
and was removed. `abi_surface!` generates `fn(value: T) -> T { value }` — an
identity function whose job is to prove a hand-written C header agrees on field
order — and it reaches no table. `dashscene-ffi` exposes no function taking
either row: there is no data plane for these types today. The argument is noted
here because it was written into three documents before the review caught it,
and because it is exactly the defect this pair of issues exists to correct.

**Keeping the fields public beside a constructor is the shape already
rejected.** PR #983's review found that a `pub px_per_em` let `Atlas::new`'s
check be bypassed by assignment, and the fix was to make it private.
`#[non_exhaustive]` would refuse the struct literal outside the crate while
leaving the fields assignable, so it reproduces that defect rather than avoiding
it.

## Why not a `Result`

Issue #985 proposed one. It cannot be handled where it would be raised:
`Arena::commit` returns `u64`, and it is the only production caller of both
methods. Every in-workspace caller would `.expect()` the error, which is the
same panic one frame further from its cause, at the cost of changing all 77 call
sites across the six packages that hold them (`dashpaint`, `dashscene-core`,
`dashscene-gpu`, `dashscene-skia`, `dashscene-validator`, `goldens/tooling`) —
most of them tests taking an `.unwrap()` that says nothing.

Both methods already refuse a range they did not assign as a documented panic,
and so do `quads`, `atlas`, `resolve`, `push` and `intern_fill`. A second
refusal in the same method, in a different shape, is worse than one in the same
shape. This is a statement about the refusing methods, not about the whole
surface: `PaintTable::get` returns an `Option` beside `resolve`'s panic, so a
fallible shape is not foreign to these types.

**What this gives up:** an embedder building a table from its own font gets a
panic rather than an error it can report. The refusal is named rather than
silent, which is what P4 requires, but it is not recoverable in-process —
`dashscene-ffi` turns the unwind into `DsStatus::Panic`, which is a status and
not a diagnostic, and `demo-android` drives `dashscene-android` directly and
never enters that guard at all. If a later story makes `commit` fallible, these
two are the natural first `Result`s.

## Written as `if … { panic!() }`, not `assert!`

`Atlas::new`'s documentation gives the reason it is a `Result`: no test tier in
this repository runs `--release`, so a `should_panic` test cannot tell an
`assert!` from a `debug_assert!` and stays green if the guard is weakened to
one. That argument survives the choice of a panic. An `if` around a `panic!` has
no debug-only spelling to be weakened to, so the guard a test exercises is the
guard compiled into a release build.

The glyph-id test is a bitwise OR over the ids rather than a search, because
`push_run` runs once per run per commit on the frame path: an id above
`u16::MAX` is exactly an id with a bit above bit 15 set, so OR-ing every id and
testing once is the same predicate with no early exit and no per-quad counter.
Measured, the search form cost about 0.2 ns per quad and the OR form was inside
the noise of no check at all. The search still runs on the failing path, to name
the quad.

## What these two checks do not cover

Stated because the defect this pair corrects was prose claiming a cover the code
did not have.

- **The predicate is "an id no font can express", not "an id this atlas has no
  row for".** The painters' `continue` stays reachable and stays correct: an
  empty outline such as a space has no atlas row by design, and a charset gap is
  a build-time coverage question the closure owns. The great majority of the id
  space is untouched by this check, by design.
- **`distance_range` has no lower bound above zero.** A subnormal is finite and
  greater than zero, so it is accepted, and it drives `px_range` to zero and
  paints the same uniform half coverage the zero case is refused for. The domain
  is a domain, not a useful range; bounding it needs a number no measurement in
  this repository supplies, which is the reason `Atlas::new` gives for having no
  upper bound on its own copy of this operand.
- **`VectorField::atlas_rect`'s extent is deliberately not refused.**
  `dashscene-gpu`'s `field_draws` treats a zero width or height as a field that
  draws nothing, so it is a legal state. That `dashscene-skia` divides by it
  instead of taking the same skip is a painter divergence, issue #1000, which
  also covers a non-finite `plane_bounds` — `field_draws` rejects that and
  skia's own degenerate guard does not.
- **`push_with` is not atomic on its other panics** (issue #1012). Only this
  check runs before the five arrays are extended; `push_entry`'s panics do not,
  and the production caller interns the entry's fills into the same table before
  calling in.
- **A `.dsb` reaches `push_with` through a validator that does not look at
  `distance_range`** (issue #1002), so for a document the panic is currently the
  only check rather than the second one. `Atlas::new` records the shape the pair
  should have: `AtlasMetrics::from_bytes` refuses a bad blob at the parse
  boundary, and the constructor check comes after it. Closing #1002 is more than
  adding a rule: `validate_scene`, the gate whose stated job is to turn these
  panics into named diagnostics, has no production caller at all.
- **`Atlas::width` and `Atlas::height` are still `pub` and unchecked, and one
  painter divides by both** (issue #1001).

`GlyphRun::size` was filed as a sixth gap (#999) and is not one. It is the third
operand of `px_range = distance_range_px * size / px_per_em`, and no seam in
`dashpaint` refuses it — but `dashscene-validator`'s
`text.style-size-out-of-range` refuses exactly that predicate on the
`TextStyle.size` that `dashscene-engine` copies into the field. The residual is
the producer that stages a text style directly and never runs the gate, which is
the same residual every public field on these rows has, and is the reason
`GlyphRun::opacity` was examined and rejected. #999 is closed as not a finding;
`distance_range` differs from both because it has no rule on any path, which is
what made #986 and #1002 real.
