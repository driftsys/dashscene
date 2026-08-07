# Feature set

    status     living — revised alongside docs/roadmap.md at each phase-end
    audience   product owners, designers, and anyone deciding whether a
               screen can be built on this today
    authority  docs/roadmap.md carries the sequencing;
               docs/specification/05-qualification.md carries the evidence
               behind a ticked box. Where this file and one of those
               disagree, they are right and this one is stale.

dashscene turns a screen designed in Figma — or written in code — into
pixels, on more than one kind of device, with every device agreeing to the
pixel about where each rectangle and each letter sits.

This file is the plain-language catalogue of what that means feature by
feature. It exists because the engineering records answer "how" and
"why" well and answer "can we ship this screen" badly: the roadmap is
organised by delivery slice, the design records by component. Neither is
a list a designer can scan for the effect they wanted to use.

Everything here is deliberately written without engineering vocabulary.
Where a technical term is unavoidable it is defined the first time it
appears.

## How to read this

Three states, and the third one matters — a feature that is 60 % built
and one nobody has started should not read the same way.

- **`- [x]`** — built and working today, with tests behind it.
- **`- [ ]` with a bold "part built" note** — some of it works. The line
  says which part does, and what is missing.
- **`- [ ]`** — not started. The line says when it is planned, or that it
  is deliberately refused.

**Deliberately refused is a real answer here, not a gap.** A design
construct this system will not draw is reported by name when the design
is imported, together with the workaround, rather than being dropped
quietly or approximated. That rule is one of the five principles the
whole system is built on, and it is why several lines below say "refused
by name" instead of "not supported yet".

---

## 1. Layout and structure

How elements are placed, sized, and arranged.

- [x] **Rows and columns** — stack elements horizontally or vertically,
      with the container sizing itself around them.
- [x] **Wrapping rows** — elements flow onto a new line when they run out
      of room.
- [x] **Grids** — elements placed on a grid of rows and columns,
      including elements that cover more than one cell.
- [x] **Free placement** — put an element at an exact position inside its
      container, without a stacking rule.
- [x] **Three sizing rules** — hug (shrink to fit the contents), fill
      (take whatever space is going), and fixed (an exact number).
- [x] **Minimum and maximum sizes** — on either axis.
- [x] **Spacing, padding and alignment** — space between elements
      (separately per axis), padding inside a container, and alignment
      along both axes.
- [x] **Baseline alignment** — a row of mixed-size text lines up on the
      line the letters sit on, not on the bottoms of their boxes.
- [x] **Deliberate overlap** — negative spacing, so elements overlap on
      purpose. Stacked avatars and overlapping cards work.
- [x] **Show and hide** — hiding an element removes it from the layout
      and the elements around it close up.
- [x] **Clipping** — a container crops what overflows it, round corners
      included.
- [x] **Overlays** — one file can hold several independent screens, each
      with its own coordinate space, drawn one over another.
- [x] **Components and instances** — a component defined once and used
      many times, including components pulled from a shared library file.
- [x] **Variants** — one component with named states, switched at
      runtime, with the layout change animating.
- [x] **Every layout number is resolved once** — the same arithmetic runs
      for every device, so two devices cannot disagree about where a box
      sits.

Not available:

- [ ] **Rotation** — refused by name. Nothing in the system draws an
      element at an angle, so a rotated design element is reported at
      import rather than drawn upright and wrong.
- [ ] **Distributing wrapped lines** — space-between across wrapped rows
      is refused by name; the alignment vocabulary does not carry it yet.
- [ ] **Absolutely-positioned children inside a stacking frame** —
      refused by name. Defaulting it would silently reflow the siblings.
- [ ] **Strokes that consume layout space, and reversed paint order** —
      refused by name, for the same reason.

## 2. Text and typography

- [x] **Latin text** — full stack: measurement, shaping, wrapping,
      drawing.
- [x] **Arabic text** — proper shaping, with letters taking their correct
      contextual form and joining as they should.
- [x] **Mixed-direction text** — right-to-left and left-to-right in one
      paragraph, ordered correctly.
- [x] **Arabic-Indic digits chosen from context** — the digit shape
      follows the surrounding text.
- [x] **Text drives layout** — a box grows to fit its text. A
      fixed-width box wraps the text and grows taller instead.
- [x] **Line wrapping** — at spaces and at explicit line breaks.
- [x] **Line height and spacing** — taken from the font's own metrics.
- [x] **Letter spacing.**
- [x] **Letter case** — upper, lower and title case, applied before the
      text is shaped, so it is correct for every script.
- [x] **Vertical alignment inside a text box.**
- [x] **Multiple font weights** — four Latin weights today. Asking for a
      weight the build does not carry is reported by name rather than
      quietly substituted.
- [x] **Font fallback** — several fonts per text style; a character
      missing from the first is drawn from the next. A character no font
      covers is reported by name, never silently blank.
- [x] **Text is identical on every device** — measured and shaped exactly
      once, then handed to each renderer already positioned. Backends
      cannot disagree about a letter's size or position, by construction.
- [x] **Sharp at any size** — glyphs are stored as distance fields rather
      than fixed-size bitmaps, so they stay crisp when scaled or animated.
- [x] **A legibility warning** — text small enough for that technique to
      smear is warned about at import, against a measured threshold, and
      a target that accepts the trade can record a waiver.

Not available:

- [ ] **Several styles inside one text block** — planned (v1). Today a
      text element carries one style. Bold-inside-a-sentence needs
      separate elements.
- [ ] **Full Unicode line breaking** — planned. Breaks happen at spaces
      and explicit newlines only: no hyphenation, no mid-word breaking,
      and no language-specific rules. A word wider than its box overflows
      rather than breaking.
- [ ] **Latin ligatures** — switched off deliberately, pending wider
      coverage of long ligature chains in the glyph build. Arabic
      ligatures are on.
- [ ] **Italic and oblique** — no vocabulary for it; reported at import.
- [ ] **Bold Arabic** — planned (v1). One Arabic weight ships today
      against Latin's four.
- [ ] **CJK, Indic and other scripts** — planned (v1) as a single piece
      of work, because the glyph-storage design cannot be settled without
      knowing which scripts it has to hold. Chinese, Japanese and Korean
      have never been ruled in or out.
- [ ] **Animated variable-font axes** — rejected. A variable font is
      supported as a fixed instance, chosen at build time.
- [ ] **Kashida justification** — deferred, with a warning at import.
- [ ] **Line height from the tallest font on a line** — the line box
      currently comes from the primary font. Individual letters do scale
      correctly per font.

## 3. Visual styling and effects

- [x] **Solid fills.**
- [x] **Four gradient types** — linear, radial, angular (which is what
      makes a gauge sweep possible), and diamond.
- [x] **Stacked fills** — several fills layered on one shape, each with
      its own opacity.
- [x] **Image fills** — with fill, fit, crop and tile modes, and a crop
      transform.
- [x] **Strokes** — aligned inside, centred, or outside the edge.
- [x] **Per-corner radii** — each corner independently.
- [x] **Drop shadows and inner shadows** — any number on one element,
      each with its own colour, offset, blur and spread.
- [x] **Backdrop blur** — frosted-glass panels that blur whatever is
      behind them, and keep doing so while they move.
- [x] **Masks** — one shape stencils the shapes after it.
- [x] **Group opacity** — a whole group faded as one. Where the group's
      children do not overlap this is free; where they do, the group is
      composited separately so the fade looks right.
- [x] **Element opacity** — independently of its group.
- [x] **Vector artwork** — vector shapes from the design file are
      converted at build time into a form that stays sharp at any size.
- [x] **Blur and shadow tuned to match Figma** — the blur constant is not
      a guess; it is measured against Figma's own render, and two
      comparison frames hold it there.

Not available:

- [ ] **Layer blur** — deferred, with a warning at import. Blurring an
      element itself, as opposed to what is behind it.
- [ ] **Advanced blend modes** — multiply, screen and the rest. Deferred,
      warned at import, and available only on high-end backends when it
      lands.
- [ ] **Corner smoothing (squircles)** — deferred, with a warning.
- [ ] **Luminance masks** — deferred, with a warning. Shape masks work.
- [ ] **Dashed strokes** — refused by name. Workaround: a baked dash
      pattern.
- [ ] **Different stroke widths per side** — warned today, and may become
      a refusal. Workaround: four edge rectangles.
- [ ] **Variable-width strokes** — rejected.
- [ ] **Noise, texture and progressive-blur effects** — rejected.
      Workaround: bake them into an image.
- [ ] **Boolean shape operations, lines, stars and polygons** — refused
      by name. The file format carries no freeform path geometry, and
      adding it is a distinct, larger piece of work. Vector artwork
      imported as a baked shape does work — see above.

## 4. Motion and interaction

- [x] **Animation is data, not code** — a designer or developer describes
      how something animates; nothing they write executes inside the
      frame loop. This is what makes the per-frame cost knowable in
      advance rather than discovered on the device.
- [x] **Tweens** — with linear, ease-in, ease-out and ease-in-out curves.
- [x] **Springs** — described by stiffness and damping, in the same shape
      Jetpack Compose uses, so a spring specified there transfers as data.
- [x] **Keyframes** — including overshoot past the target.
- [x] **Staggered transitions** — successive elements start a fixed delay
      apart.
- [x] **Interruptible mid-flight** — retargeting an animation that is
      already running picks up from where it actually is, and a spring
      keeps its velocity. No visible snap.
- [x] **Layout transitions** — when a variant switch changes the layout,
      elements animate from where they were to where they now are.
- [x] **Reproducible** — the same inputs produce the same frames on every
      machine, which is what lets animation be tested rather than
      eyeballed.
- [x] **Live values from the application** — a number or a string from
      the running app drives a property: position, size, spacing, or a
      fill colour channel.
- [x] **Smoothed values** — a driven value follows its target through a
      spring rather than jumping.
- [x] **Visibility driven by a value.**
- [x] **Values authored in Figma** — a designer declares a Figma
      Variable, and the running application writes it by name. The
      designer names the connection; nobody guesses it.
- [x] **Pointer and keyboard input** — in the demonstration host: the
      pointer scrubs a scene's value, arrow keys snap it, space triggers
      a variant switch.

Not available:

- [ ] **Gauges and radial motion** — a value driving a rotation about a
      pivot or an arc sweep. Designed, and a v1 candidate.
- [ ] **Looping animations, enter and exit animations, and standalone
      keyframe tracks** — later rows of the same vocabulary; the
      variant-transition row is what ships today.
- [ ] **Known limit — a frame that changes only a text string** currently
      clears the screen's text unless something else on that frame also
      changes the layout. There is a documented authoring rule to work
      around it, and a fix is tracked.

## 5. Runtime performance

- [x] **Layout solved once for every backend** — not once per renderer.
- [x] **Text shaped once and cached** — re-measuring unchanged text costs
      a lookup, not a re-shape.
- [x] **Incremental updates** — the cost of a change scales with the size
      of the change, not with the size of the screen.
- [x] **Changes that only affect appearance skip layout entirely.**
- [x] **A fast path for contained changes** — a change that provably
      cannot move anything else replays cached geometry and never runs
      the layout solver at all.
- [x] **The renderer is told what changed** — so it can upload only the
      parts of the frame that moved.
- [x] **That "what changed" list is proven correct** — not assumed. A
      second rendering mode models the upload and is checked
      pixel-for-pixel against the ordinary one, so a missed entry is a
      caught bug rather than a flicker somebody notices on a device six
      months later.
- [x] **No memory allocation while animating.**
- [x] **Lists are bounded pools rather than growing structures.**
- [x] **The showcase scenes have a measured frame cost** — recorded, per
      scene.

Part built and planned:

- [ ] **Faster startup on large files** — **part built.** See "The design
      file format" below; this is the current open slice.
- [ ] **Renderer-side packing of only the changed regions** — **part
      built.** Uploading only changed regions works; deciding which
      regions to rebuild does not, so the lean renderer still rebuilds
      the whole frame's draw list each time.
- [ ] **Performance tuned against real target hardware** — planned (v1).
      About twenty specific improvements are identified and deliberately
      held: none has a frame budget or a device measurement behind it,
      and fixing one now would produce a change whose only success
      criterion is that the tests still pass.
- [ ] **A memory budget for a device** — planned (v1). No number exists
      in the specification today, so nothing can currently fail for
      exceeding it.

## 6. The design file format

The compiled file a device actually loads.

- [x] **One format for both sources** — a screen from Figma and a screen
      written in code produce the same kind of file and render
      identically. This is proven rather than asserted, for the layout
      and solid-fill vocabulary both routes express: the same screen
      authored both ways produces a byte-identical image. Extending that
      proof to cover text and driven values is planned (v1).
- [x] **The file carries intent, never results** — no baked-in positions,
      no rasterised pixels, no glyph placements. That is what lets one
      file serve devices with different screens and different renderers.
- [x] **Reproducible output** — the same design always compiles to a
      byte-identical file. Everything downstream depends on this:
      caching, integrity checking, and signing when it arrives.
- [x] **Split into a small head and a bulk tail** — everything needed to
      lay out and draw sits at the front; images and other payloads sit
      behind a page boundary, so a device can verify what it needs
      without touching the rest.
- [x] **Integrity checked at load** — every section carries a
      cryptographic hash, and the file is rejected whole if the version
      or any hash does not match. The file is validated before any parser
      is allowed to run on it.
- [x] **Assets identified by content** — an image is named by what it is,
      not by where it sits in the file, so payloads can be reordered or
      swapped for a different device build without touching the design.
- [x] **Compatible in both directions** — a newer file opens in an older
      reader, and a construct the older reader does not know is reported
      by name rather than silently ignored.
- [x] **The format is frozen against accidental change** — a committed
      reference file fails the build if any field moves, so a schema
      break cannot happen quietly.
- [x] **Memory-mapped loading** — the file is mapped rather than read
      into memory.
- [x] **Image payloads read straight from the mapping** — not copied a
      second time on the way in.
- [x] **The same format works on the wire as on disk** — which is what
      makes streaming a later feature rather than a rewrite.

Part built and planned:

- [ ] **Startup cost proportional to what is shown** — **part built, and
      this is the current open slice.** The goal: opening a large file
      and showing one screen should cost about what that one screen
      costs, not what the whole file costs. The requirement, the
      benchmark and a measurement all exist, and the measurement
      currently reads 9.81x against a target of 1.00x. Mapping the file
      and removing the duplicate copies have landed. The remaining piece
      — fetch only the shown screen's images, and verify each as it is
      first touched — is specified and not yet built.
- [ ] **Placeholders for content that has not loaded yet** — planned
      (v1). The format reserves the fields. It is blocked on deciding
      what a not-yet-loaded image should show, because the design source
      supplies no answer and inventing a grey would put a result into a
      file that is only allowed to carry intent.
- [ ] **File signing** — the header reserves the fields for it and today
      refuses any file that puts anything in them. No signing tool, key
      handling, or verification policy exists yet. See section 12.
- [ ] **Big-endian devices** — correct by construction, untested, and
      deferred until such a device appears on the plan.

## 7. Images, fonts and asset preparation

Turning source images and fonts into what actually ships on a device.

- [x] **Three quality profiles** — RAW (untouched), HiFi, and LoFi.
- [x] **A profile is a measured quality band, not a fixed format** — the
      tool walks a ladder of encodings and picks the cheapest one that
      still stays inside the band, per asset.
- [x] **Every band ships with the change that breaks it** — a quality
      threshold nobody has ever seen fail is not a contract, so each one
      is stored together with the measured degradation that trips it.
- [x] **Textures the GPU reads directly** — compressed into a form the
      graphics hardware samples without unpacking, which is what keeps
      the memory cost down rather than only the file size.
- [x] **Everything runs in-process** — the encoder is built into the tool
      rather than shelling out to an external command, so a build has no
      hidden tool dependency.
- [x] **Text and icon artwork is never compressed** — deliberately. The
      legibility risk is too high for text and the saving too small for
      icons.
- [x] **One set of source assets, several device builds** — a derived
      build plus a manifest recording exactly what was substituted.
- [x] **The design half of the file never changes with the profile** —
      measured byte for byte, so a quality change cannot silently move a
      layout.
- [x] **Image formats identified from the content** — never trusted from
      a label. A file whose contents contradict its declared format, or
      whose real dimensions differ from the recorded ones, is an error.
- [x] **PNG, JPEG and static GIF sources.**
- [x] **Glyph coverage decided at build time** — per locale, including
      the contextual Arabic forms that only exist after shaping.
- [x] **Font builds are reproducible** — the same font produces the same
      bytes, checked across two different CPU architectures in
      automation.
- [x] **A review preview** — the reference renderer draws all three
      quality profiles so the trade can be looked at rather than argued
      about.

Not available:

- [ ] **An overall memory budget for a device** — planned (v1). Today a
      build can succeed and still not fit the target, and nothing detects
      it. This is a stated, accepted gap rather than an oversight.
- [ ] **Generating glyphs on the device at runtime** — deferred. Coverage
      is declared at build time.
- [ ] **A universal texture format for mixed device fleets** — named as
      the contingency if the fleet stops sharing one texture format. Not
      built.

## 8. Where designs come from

- [x] **Figma import** — through Figma's own API, with the design
      compiled by the same code that compiles everything else.
- [x] **Roughly 95 % of real product design files render** — the
      specification's own estimate of the supported vocabulary.
- [x] **Auto-layout, components, instances, variants, text, shapes,
      images and effects** all import.
- [x] **Components from a shared library file** resolve across files.
- [x] **Design tokens** — Figma Variables reach the runtime both as
      resolved values and by name, through a companion Figma plugin.
- [x] **Designer-authored intent** — a plugin lets a designer mark up
      which elements are screens, which are scaffolding that should not
      ship, and which properties the application drives.
- [x] **Nothing is ever dropped silently** — every unsupported construct
      is a named message naming the workaround. This is enforced as a
      design principle, not a habit.
- [x] **Warnings can be waived per target, with a recorded reason** — and
      a release build refuses a warning that has not been waived. A
      waiver covers one rule at one place, never a rule everywhere.
- [x] **Authoring in code** — a Rust interface fills the same model
      directly, with no file in between. Used for tests, stress cases,
      and application-driven screens.
- [x] **Both routes are proven to converge** — the same screen authored
      in Figma and in code produces an identical result, over the layout
      and solid-fill vocabulary both express. The wider proof, covering
      text and driven values, is planned (v1).
- [x] **A generated stress corpus** — deliberately awkward screens, used
      to keep the layout engine honest.
- [x] **Two real public Figma files import and render end to end.**

Not available:

- [ ] **A command-line "compile this Figma file"** — the importer runs as
      a scripted tool today. Verifying an already-compiled file has a
      command; compiling does not.
- [ ] **Authoring from inside a game engine (C#)** — planned (v1),
      alongside the Unity renderer.
- [ ] **Other design tools** — Penpot and similar were assessed and
      declined. Adding a design tool means writing a new front end for
      it, not a new file format.

## 9. Rendering backends

- [x] **One renderer contract** — every renderer consumes the same
      finished data. Swapping renderers is a re-check of the pictures,
      not a redesign.
- [x] **A renderer only colours** — it never measures text, wraps lines,
      or moves anything. This is why two renderers cannot drift apart on
      layout: they are never given the chance to disagree.
- [x] **Reference renderer (Skia)** — draws the whole feature set on the
      CPU, which makes its output exactly reproducible and therefore
      usable as the yardstick everything else is measured against. It
      stays permanently, for that reason.
- [x] **Lean GPU renderer** — instanced quads and analytic distance
      fields, built for bandwidth-constrained hardware. Draws the whole
      feature set, native and in a browser, from one codebase.
- [x] **The two renderers agree** — measured on real hardware. One set of
      tolerance bands serves both, which is the opposite of what was
      expected, and none of the reference pictures had to change.
- [x] **The renderer contract is language-neutral** — enforced by the
      build rather than promised, so a renderer written in another
      language can consume the same data.
- [x] **Backend chosen per screen, not per element.**

Part built and planned:

- [ ] **Unity renderer** — **part built.** The contract that keeps the
      data consumable from C# exists and the build enforces it. The
      renderer itself, its shader library, and the C# projection are
      planned (v1) and live in a separate repository that has not been
      created.

Not available:

- [ ] **A direct GLES renderer** — the named contingency if the current
      GPU layer's older-hardware path fails on a target device.
- [ ] **Browsers without WebGPU** — a browser without it is told so and
      draws nothing. A fallback is a redesign rather than a small piece
      of work, and is a v1 question that depends on which browsers the
      product must reach.
- [ ] **Switching entry-level devices to the lean renderer** — waits on a
      measurement on real entry-level hardware, and no such hardware is
      in the loop.
- [ ] **JPEG and GIF through the lean renderer** — refused on purpose:
      the asset tool converts them away before a product build ships, and
      linking three image decoders into a renderer works against both the
      size and the security goals.

## 10. Platform support

- [x] **macOS, Linux and Windows desktop** — a windowed host with an
      event loop, input, and the animated showcase running in it.
- [x] **Browsers with WebGPU** — the same showcase on a canvas, with the
      design file fetched in byte ranges so only the part needed is
      downloaded.

Part built and planned:

- [ ] **Embedding in a real application** — **part built, and this is the
      current open slice.** Everything below the renderer contract is a
      library. Everything above it is two demonstrations that are not
      published, so an integrator today starts from a demonstration and
      copies out of it. Deciding what an embedder actually gets is the
      open work.
- [ ] **Android** — nothing yet: no build target, no toolchain, no
      automation. Planned for the current open slice, over a proposed
      three-layer structure — surface handling, application state, and a
      screen-description layer — sharing one C interface.
- [ ] **iOS** — planned (v1). The same three-layer structure is written
      to apply to it.
- [ ] **A C interface for non-Rust hosts** — the foundations exist and
      are enforced by the build; nothing is built on them yet. This is
      the likely seam between the desktop/web half and the mobile half.
- [ ] **A guide for writing a new renderer**, with a worked example —
      planned for the current slice. It serves Unity and any future
      backend.

## 11. Quality tooling and workflow

- [x] **Reference pictures for every construct** — committed images the
      build compares against, so a change that alters a pixel has to be
      declared rather than discovered.
- [x] **Comparison against Figma's own render** — seven frames
      perceptually compared against real Figma captures, each inside a
      declared tolerance, in automation. This has already caught two real
      bugs on their first measurement.
- [x] **A second comparison set** — seven more self-authored frames
      covering vocabulary the Figma comparison does not reach.
- [x] **Every tolerance ships with the change that breaks it** — the same
      discipline as the asset quality bands, for the same reason.
- [x] **Three test tiers** — about five seconds between edits, about
      thirty-three before pushing, and about fifty-four for the run that
      re-derives the asset tables. Which tier ran is stated on every
      change.
- [x] **A single check asserting all seven qualification criteria on one
      commit** — with its membership pinned by name, so a renamed test
      cannot quietly leave the check.
- [x] **Cross-architecture reproducibility** — the font glyph build must
      produce identical bytes on two different CPU architectures.
- [x] **Formatting, linting, commit-message checks and a dependency
      audit** — all automated.
- [x] **The marketing still image is reproducible from its arguments** —
      it steps a fixed frame time and never reads a clock, so it is
      regenerated rather than screenshotted.
- [x] **Every diagnostic carries a rule name and a location** — so a
      report is actionable rather than a wall of text.

Not available:

- [ ] **The compiler as a shipped product** — planned (v1): a stable
      command-line interface, versioned diagnostics, a waiver workflow,
      lint rule packs, and reporting tooling for design review. Today it
      is an internal tool.
- [ ] **Strict profile enforcement on shipped documents** — planned (v1).
      The waiver machinery is built; the gate that requires it is not
      wired up.
- [ ] **Anything that catches "it looks wrong on a real automotive
      driver"** — the only check that could is a measurement on real
      hardware, and it is not automated.

## 12. Build, delivery and integrity

- [x] **Reproducible builds** — the same design always produces a
      byte-identical file. Everything below depends on this being true.
- [x] **Integrity verified at load** — version and per-section
      cryptographic hashes, checked before anything else in the file is
      trusted.
- [x] **The container is validated before any parser runs** — the part of
      the code that runs first, on untrusted bytes, deliberately depends
      on no parser and takes the writer's word for nothing.
- [x] **Reserved fields must be zero** — a file that puts meaning in a
      field this version does not understand is refused, rather than
      being read with that meaning silently ignored.
- [x] **Assets authenticated by content** — a substituted payload does
      not resolve, because assets are found by what they contain.
- [x] **A deliberately small trusted surface** — image decoders are the
      classic source of vulnerabilities in this kind of system. The asset
      tool converts formats away before a product build ships, and the
      lean renderer links one decoder rather than three.
- [x] **No network in the render path** — nothing is fetched while
      drawing.
- [x] **A broken contract stops rather than guesses** — the system is
      built to fail loudly on an impossible state instead of drawing
      something plausible and wrong.
- [x] **Pinned dependency versions, a committed lock file, and an
      automated dependency audit.**

Not available:

- [ ] **Signing** — the file format reserves the header fields for a
      signature and the byte range to cover, and today refuses any file
      that uses them. No signing tool, key management, or verification
      policy exists. This is the single largest gap in this section.
- [ ] **Over-the-air delivery and update.**
- [ ] **An admission policy for untrusted producers** — an open question
      that arrives together with streaming.
- [ ] **Remote and streamed screens** — v2. Sending a screen to a display
      that is not local to the renderer. The architecture is already
      shaped for it, and today's interfaces are deliberately constrained
      so that v2 does not become a breaking change.

---

## Two things worth stating plainly

Both are easy to misread from a checklist, so neither is left implied.

**Nothing here is released.** This is a private working repository. The
public package names were reserved before development started and hold no
code. Nothing in this document is available to anyone outside the
project.

**The automated checks exist and are not currently running.** Continuous
integration on this repository is blocked at the account level: every job
fails within seconds having executed no steps. Measured 2026-08-07, this
was still the case. It is a billing block rather than a code failure, but
the consequence is real — recent work was verified by running the same
checks locally, with the evidence recorded on each change, rather than in
automation. A ticked box in this document means the tests exist and pass;
for recent items it does not mean a machine other than a developer's ran
them.

## What this document is not

It is not a schedule. No line here carries a date, and the ordering
inside a section is by topic rather than by delivery order.
[`docs/roadmap.md`](roadmap.md) is where the sequencing lives — which
slice delivers what, which are closed, and what depends on what.

It is not the evidence, either. A ticked box here is a summary;
[`docs/specification/05-qualification.md`](specification/05-qualification.md)
is where a claim is proven, and the design records under
[`docs/design/`](design/) are where a feature's actual behaviour and its
edge cases are written down.
