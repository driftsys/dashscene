# A sub-word member on a boundary-B row widens rather than declaring padding

    status   accepted (2026-08-02)
    scope    dashpaint's GlyphQuad and AtlasGlyph; any future boundary-B row
             whose member is narrower than the members around it

## Context

Story #578 gave boundary B a C representation, and set the rules for anything
crossing that seam: `#[repr(C)]`, fixed-width integers, no `bool`, no payload
enums, no nested collections, and **explicit padding**.

Two types failed the last of those. `GlyphQuad` is `{u16, f32, f32}` and
`AtlasGlyph` is `{u16, [f32; 4], [f32; 4]}`; at alignment 4 both put the glyph
id at offset 0 and the next member at offset 4, so rustc inserts two bytes after
the id. That is FFI-_safe_ — a C compiler inserts the same — but it is not
FFI-_explicit_, and `crates/dashscene-unity` asserted the hole rather than
fixing it, with a comment saying story #578 would.

## Decision

**Widen the member to the word the struct is built from, rather than declaring
the padding beside it.** Both glyph ids are `u32`. No `_pad` member is added,
here or in any future boundary-B row with the same shape.

## Why

**A declared padding member is public, and it participates in equality.** These
types derive `PartialEq`, so a `pub _pad: u16` would make two quads that agree
in every meaningful field compare unequal for differing in a member that means
nothing. That is exactly the hazard
`optional-members-are-ranges-of-arity-one.md` D2 removed from the empty ranges,
one story earlier, and re-introducing it to satisfy a rule about padding would
trade a real defect for a cosmetic one.

**It costs nothing.** `{u16, f32, f32}` and `{u32, f32, f32}` are both 12 bytes;
`{u16, [f32; 4], [f32; 4]}` and `{u32, [f32; 4], [f32; 4]}` are both 36. Every
float rectangle keeps the offset it had, so no consumer's declaration changes
and no golden moves. The bytes that were padding become bytes the producer
writes.

**Removing a hole beats naming one.** The rule asks for padding to be explicit
so that a struct reads the same in both languages. A struct with no padding
satisfies that more directly than one whose C header has to declare a filler
field, and it removes the question of what the filler contains.

## What it gives up, and what replaces it

The `u16` made an out-of-domain id **unrepresentable**. OpenType glyph ids are
16-bit, and `dashscene-typeset` still carries them as `u16`; the five producer
sites widen with `u32::from`, which is exact. But the boundary-B type can now
hold a value no font can produce, and such a value would make `Atlas::glyph`
miss and the glyph paint nothing with no diagnostic — a silent drop (P4).

`Atlas::new` refuses it by name, beside the sorted-and-unique assertion it
already carries. A type-level guarantee became an asserted one, which is the
trade this decision makes and the reason it is written down rather than left in
a commit message.

## How the property is held

Not by the size. Both structs were already 12 and 36 bytes _with_ the hole in
them, and the member after the id sat at offset 4 either way — so
`the_surface_layout_is_what_it_was_when_it_was_pinned` stayed green through the
whole time the padding existed, and an offset-only check does too. Both were
measured, not assumed.

What has teeth is the id member's **own size**:
`neither_glyph_type_carries_padding` asserts `size_of_val(&value.glyph_id) == 4`
for each type, together with the offsets and with total = sum of member sizes.
Narrow either id together with whatever must change for the workspace to compile
and that assertion fails; narrow one on its own and the compiler refuses it
first.
