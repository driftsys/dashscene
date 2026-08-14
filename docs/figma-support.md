# What imports from Figma

    status     living — revised at each phase-end, alongside docs/features.md
    audience   designers working in Figma, and whoever answers "will this
               import?"
    method     derived from the importer's code, not from a specification.
               See "How this list is derived".
    related    docs/features.md — the same ground by system concern rather
               than by Figma panel

Design in Figma, and this compiles it into something a device can draw. Not
every Figma feature survives that trip. This page says which do, which are
turned away with a message, and — the part worth reading — which do nothing at
all and tell you nothing.

## The five outcomes

Figma features do not divide into "works" and "does not work". They divide into
five, and the difference between them is what you lose.

|                   | What happens                                              | Told?        |
| ----------------- | --------------------------------------------------------- | ------------ |
| **Imports**       | Drawn as designed.                                        | —            |
| **Warned**        | The layer imports. That one property is left out.         | Yes          |
| **Layer dropped** | That layer does not import. The rest of the file does.    | Yes, by name |
| **Import fails**  | Nothing imports. The compile stops.                       | Yes, by name |
| **Silent**        | The property is not read at all. Everything else imports. | **No**       |

Three of these are easy to confuse:

- **Warned** costs you a squircle and leaves the button.
- **Layer dropped** takes the button. A shadow on a text layer does not import
  the text without its shadow — the text is gone.
- **Import fails** takes the file. One layer set to Multiply and nothing comes
  across at all.

**And read the silent list.** A refusal is a conversation. A silent property is
a design that looks right in Figma, imports without complaint, and comes out
wrong on the device.

## How this list is derived

Unlike a hand-written feature list, this one comes out of the code:

- **Layer-dropped** entries are the importer's own refusal messages. It refuses
  a layer by name in 56 places, several of which fill in the offending value, so
  the wording below is close to what you will actually see.
- **Warned** and **import fails** are a separate, short, fixed list of
  constructs the importer recognises and hands to a checker, which decides which
  of the two it is.
- **Silent** properties are the ones the importer's Figma data model does not
  name. This is structural rather than a list of oversights: the parser does not
  reject properties it was not taught, so anything unread is dropped without a
  word.
- **Imports** is what is left, checked against the code that draws it.

Checked against `crates/dashc/src/figma/mod.rs` (what lowers, and what is
refused as a whole layer), `crates/dashc/src/figma/triage.rs` (which constructs
are recognised and handed to the checker), `crates/dashc/src/figma/rest.rs`
(what is read at all), `crates/dashc/src/figma/bindings.rs` (what a Figma
Variable can drive), `crates/dashscene-validator/src/triage.rs` (warn, or stop
the import), and `importers/figma/src/` (the import tool).

**This page can go stale, and nothing fails when it does.** It is re-checked at
each phase-end. If a decision rides on a line here, read the code named above,
or ask.

---

## Stops the whole import

Three constructs. One of these anywhere in what you are importing and nothing
comes across.

- **Blend modes other than Normal** — multiply, screen, overlay, and the rest.
  Planned for high-end backends; today they stop the import.
- **Noise and texture effects.**
- **Progressive blur.**

Workaround for all three: bake the result into an image, or design without it.

The checker also knows about variable-width strokes, animated boolean operations
and animated variable-font axes, but the Figma importer never reports them: a
variable-width stroke is caught earlier as a non-basic stroke and drops that
layer, and nothing reads the animation data the other two would need. Do not
read those as supported — read them as reaching you a different way, or not at
all.

## Layer types

Eight kinds import. Any other kind drops that layer, reporting its type.

| Imports               | Layer dropped                                            |
| --------------------- | -------------------------------------------------------- |
| Frame, Group, Section | Line, Star, Polygon, Arrow                               |
| Rectangle, Ellipse    | Boolean operations (union, subtract, intersect, exclude) |
| Text                  | Slice, and anything else                                 |
| Component instance    |                                                          |
| Vector                |                                                          |

**Ellipses have conditions.** A full circle imports. A ring (inner radius), a
partial arc, an ellipse whose width and height differ, and an ellipse that is
not fixed-size each drop the layer.

**Vectors are baked at build time** into a form that stays sharp at any size. A
vector with no path geometry drops.

**Workaround:** flatten the shape to a vector, or export it as an image.

## Auto layout

| Imports                              | Layer dropped                                    |
| ------------------------------------ | ------------------------------------------------ |
| Horizontal, vertical, wrap, grid     | Any other auto-layout mode                       |
| Gap, padding, separate gaps per axis | Distributing wrapped lines across the cross axis |
| Hug, fill, fixed sizing              | A fill-sized child on an axis its parent hugs    |
| Min and max width and height         | Absolute position inside an auto-layout frame    |
| Alignment on both axes               | Alignment values outside the supported set       |
| Baseline alignment                   | "Strokes included in layout"                     |
| Grid row and column spans            | Reversed layer order                             |
| Negative gap, for deliberate overlap | Grid tracks sized `auto` or `min-content`        |

**Baseline alignment has two holes, and neither is reported:** a nested frame
inside a baseline row aligns by its box bottom rather than by its text, and a
wrapping row is not baseline-corrected at all.

**A fill child on a hug axis** is refused rather than guessed — Figma and CSS
resolve that cycle differently, and the result would be a picture Figma never
showed you. Set the parent to fixed, or the child to hug.

## Size and position

| Imports             | Layer dropped | Silent                               |
| ------------------- | ------------- | ------------------------------------ |
| X, Y, width, height | Rotation      | **Constraints** — pin, centre, scale |
| Min and max sizes   |               |                                      |

**Constraints are read nowhere, and this is the largest silent gap here.** A
frame whose children are pinned to its edges imports as though they were placed
at fixed offsets; nothing moves when the frame resizes. Use auto layout, which
does import, for anything that must respond to size.

**Rotation drops the layer.** Nothing in the system draws a rotated element.
Bake the rotation into a vector or an image.

## Fill

| Imports                                                                     | Layer dropped                                              |
| --------------------------------------------------------------------------- | ---------------------------------------------------------- |
| Solid                                                                       | More than one fill on an ellipse, a vector or a text layer |
| Linear, radial, angular and diamond gradients                               | A fill with no colour                                      |
| Image fills — fill, fit, crop, tile                                         |                                                            |
| Several stacked fills on frames, rectangles, instances, sections and groups |                                                            |
| Per-fill opacity and visibility                                             |                                                            |

**Stacked fills depend on the layer type.** On a frame or rectangle they work.
On a circle, a text layer or a vector, a second fill drops the layer.
Workaround: put the extra fill on a rectangle behind it.

**Images:** PNG, JPEG and static GIF. An animated GIF is refused by name.

## Stroke

| Imports                 | Layer dropped             | Silent                     |
| ----------------------- | ------------------------- | -------------------------- |
| One solid stroke        | More than one stroke      | **Per-side widths**        |
| Inside, centre, outside | Dashed strokes            | **Cap, join, miter limit** |
| One width               | Any non-basic stroke type |                            |
|                         | A stroke with no colour   |                            |
|                         | Stroke on text (outline)  |                            |

**Per-side stroke widths are silent, and nothing can warn about them** — the
property is not read, so there is nothing to diagnose. A layer with a 4-point
top border and nothing elsewhere imports with one uniform width taken from
whichever value the file reports. Draw four rectangles instead.

**Dashes:** bake the pattern into a vector.

## Corner radius

| Imports                                 | Warned                           |
| --------------------------------------- | -------------------------------- |
| One radius, or four independent corners | **Corner smoothing** (squircles) |

Corner smoothing is the clearest example of the warned category: you get a
message, the corner is drawn as an ordinary rounded corner, and the layer
imports.

## Effects

| Imports         | Layer dropped                      | Warned     | Stops the import |
| --------------- | ---------------------------------- | ---------- | ---------------- |
| Drop shadow     | A blur on a text layer             | Layer blur | Noise, texture   |
| Inner shadow    | A shadow on a text or vector layer |            | Progressive blur |
| Background blur | A shadow with no colour            |            |                  |

**Shadows do not work on text or vector layers, and the whole layer goes** — not
just the shadow. This surprises people. Put the text on a shadowed frame
instead.

**Background blur works** and keeps working while the panel moves.

## Text

| Imports                                   | Layer dropped                                  |
| ----------------------------------------- | ---------------------------------------------- |
| Font family, size, weight                 | Italic and oblique                             |
| Four Latin weights                        | Text case (upper, lower, title)                |
| Latin and Arabic, including right-to-left | Decoration (underline, strikethrough)          |
| Line height, letter spacing               | Truncation and ellipsis                        |
| Horizontal and vertical alignment         | Hyperlinks                                     |
| Auto-width, auto-height, fixed            | Mixed styles in one text layer                 |
|                                           | OpenType feature settings                      |
|                                           | A text layer with no characters, style or fill |

**Text case drops the layer, it does not apply.** A layer set to All Caps in
Figma does not import. Type the words in the case you want.

**Mixed styles in one text layer** — one bold word in a sentence — drop the
layer. Split it into separate text layers.

**Below 14 pixels per em**, text is warned about: the way glyphs are stored
smears below that size. It still imports.

**Arabic ships in one weight**, against Latin's four.

## Masks

| Imports                                         | Layer dropped      |
| ----------------------------------------------- | ------------------ |
| Shape masks — one layer stencils those after it | Alpha (soft) masks |
|                                                 | Luminance masks    |

A soft or luminance mask has no equivalent that can be drawn, so the layer goes
rather than being approximated with a hard edge.

## Components, instances and variants

| Imports                                      | Import fails                                                   |
| -------------------------------------------- | -------------------------------------------------------------- |
| Instances                                    | A library component containing an image                        |
| Variants, switched at runtime                | A library component instancing one from a second library       |
| Components from a shared library             | A library component whose own reference the library data lacks |
| The layout change animating between variants |                                                                |

All three failures stop the **whole import**, not just that layer. If a shared
library is in play and the import stops, look here first.

**Import instances, not the components themselves.** A main component, and a
component set, is treated as a definition rather than as content: it resolves,
so instances of it work, but it draws nothing and — unlike every other case on
this page — **says nothing either**. Point the import at a frame or an instance.

The same skip has a second effect worth knowing: problems inside a main
component never surface. A dashed stroke or an unsupported construct in the
definition is not reported, because nothing in it is being drawn. You will hear
about it through the instance that uses it, not through the component.

## Variables and tokens

Figma Variables import, both as values and by name, so an application can write
them at runtime. A designer declares the connection; nobody guesses it. A number
can drive position, size, spacing, opacity or one channel of a solid fill; a
true/false value can drive visibility.

**A variable bound to a fill only works on a solid fill.** Bind one to a layer
carrying a gradient or an image and you get a warning, the layer imports with
the fill exactly as you drew it, and the binding is dropped — so the colour is
right on the first frame and never changes afterwards. Text is worth watching
here: a text layer's fill lives in its text style, so a fill binding on text
takes the same path.

## Read nowhere

Everything here imports without complaint and does nothing. Consolidated,
because this is the list worth scanning before you commit to an approach.

| Property                                              | What you lose                                                                                                           |
| ----------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| **Constraints**                                       | Nothing moves when its parent resizes. Use auto layout.                                                                 |
| **Per-side stroke widths**                            | One uniform width instead. Use four rectangles.                                                                         |
| **Stroke cap, join, miter limit**                     | Defaults instead of what you set.                                                                                       |
| **Layout grids and columns**                          | The guide does not come across. Usually harmless, since it is a design aid, but a visible grid overlay will not render. |
| **Prototyping** — links, transitions, hotspots        | Nothing interactive. Motion is described in code, not in Figma.                                                         |
| **Export settings**                                   | Ignored; the asset pipeline decides formats.                                                                            |
| **Paragraph spacing, paragraph indent, list spacing** | Paragraphs run together; lists lose their indentation.                                                                  |
| **Shared style references**                           | The resolved values still come across, so this is usually invisible in the result.                                      |
| **`layoutAlign` / `layoutGrow`**                      | Figma's older auto-layout child sizing. Current files use the newer sizing properties, which do import.                 |

The reason this list exists at all: the importer's data model does not reject
properties it was not taught to read, so an unread Figma property is dropped
rather than reported. That is what separates this group from everything above.

## Where the specification disagrees with the code

`docs/specification/04-figma-vocabulary-profile.md` is the engineering profile
for this vocabulary. It is **not** the source for this page: this page is
derived from the lowering, and that profile is derived from the design intent.

The five disagreements this section listed were corrected in the profile on
2026-08-14 (issue #802) — text case, luminance masks, dashed strokes, per-side
stroke widths and stroke-on-text alignment — along with a sixth the list had
missed, single-stop gradients, which the profile described as lowering to a
solid fill with an info diagnostic and which in fact lowers verbatim as a
one-stop gradient with no diagnostic at all. The profile also gained the
disposition it had no vocabulary for: **NOT READ (silent)**, the class per-side
stroke widths belongs to.

That closes the six rows. It does **not** mean the two documents now agree
everywhere: they are derived from opposite ends, and the pass that produced this
section found one error they shared rather than disagreed on — both placed
prototyping in the silent class, where the importer in fact reads `interactions`
and reports what it cannot lower. The profile is corrected; this page's "Read
nowhere" table still lists it and is the next thing to check.

Repeat the derivation whenever either document moves. Agreement between two
documents derived from opposite ends is evidence about one of them at most.

---

## What this page is not

It is not a promise about a date. Refused constructs have workarounds today;
whether any becomes supported is `docs/roadmap.md`'s question.

It is not exhaustive about Figma. It covers what the importer names, plus the
properties a designer is most likely to reach for. A Figma property absent from
every column here is most likely in the silent group — the parser ignores what
it was not taught — and worth asking about before you rely on it.
