# Feature set

    status     living — revised at each phase-end, alongside docs/roadmap.md
    audience   product owners, designers, and anyone deciding whether a
               screen can be built on this today
    method     every claim below was checked against the code, not against
               another document. See "How this file is kept honest".
    authority  docs/roadmap.md carries the sequencing;
               docs/specification/05-qualification.md carries the evidence.
               Where this file and one of those disagree, they are right
               and this one is stale.
    related    docs/figma-support.md — the same ground by Figma panel, for a
               designer asking "will this import?"

dashscene turns a screen designed in Figma — or written in code — into
pixels, on more than one kind of device, with every device agreeing to the
pixel about where each rectangle and each letter sits.

This file answers one question the engineering records answer badly: can
this screen be built, and if not, what happens instead. The roadmap is
organised by delivery slice and the design records by component; neither
is a list a designer can scan for the effect they wanted to use.

## How to read this

- **`- [x]`** — built and working today.
- **`- [ ]` with a bold "part built" note** — some of it works. The line
  says which part, and what is missing.
- **`- [ ]`** — not built. The line says whether it is planned, or refused
  on purpose.

**Refused on purpose is a real answer, not a gap.** A construct this
system will not draw is reported by name when the design is imported,
with the workaround, rather than dropped quietly or approximated. That is
one of the five principles the system is built on, and it is why several
lines say "refused" rather than "not yet".

**Where a feature has a limit, the limit is on the same line.** A tick
with a caveat attached is the normal case here, not an exception.

## How this file is kept honest

This file is deliberately short. An earlier draft listed 186 features, and
three review rounds found factual errors in it at a rate that was not
falling — most of them because the claims had been written from this
repository's own design and specification records, which had themselves
drifted from the code in four places. A catalogue that is 95 % accurate is
worse than a short one that is right, because nobody can tell which 5 %
they are reading.

So the claim surface was halved and **every line below was checked against
the code that implements it**, with each section naming where to check.
A fourth review round then found errors in that too, at a similar rate,
which is the honest thing to know about this file:

- **Nothing enforces this mechanically.** No test fails when a line here
  goes stale. It is re-checked by hand, and hand-checking has a
  demonstrated error rate on this material.
- **The recurring mistake is depth, not honesty.** Checking that something
  exists is not checking which branches it does not cover, what the
  default path does, or whether any command reaches it. Several corrected
  lines were wrong in exactly that way.
- **Sequencing labels are the one exception to "checked against code".**
  "Planned (v1)" and "v2" come from [`docs/roadmap.md`](roadmap.md); no
  code can substantiate a schedule.
- **Do not promise a customer a ticked box** without reading the code named
  under the section, or asking.

This file lists unbuilt work on purpose, which deviates from the house rule
that a shipped document describes the system as-built.
`docs/design/architecture.md` takes the same deviation for its own reason.
The rule's real concern is that an unbuilt thing must never be described as
built; every unbuilt line below is marked unbuilt and says what is missing.

---

## 1. Layout and structure

Checked against `crates/dashscene-engine/src/lib.rs`,
`crates/dashscene-core/src/arena.rs`, `crates/dashc/src/figma/mod.rs`.

- [x] **Rows, columns, wrapping rows and grids** — including grid items
      that span more than one cell. Three auto-layout constructs are
      refused rather than lowered: distributing wrapped lines, a grid track
      sized `auto` or `min-content`, and a fill-sized child on an axis its
      parent shrinks to fit.
- [x] **Three sizing rules, with minimums and maximums** — hug (shrink to
      contents), fill (take the available space), fixed (an exact number),
      on either axis, with spacing, padding and alignment.
- [x] **Deliberate overlap** — negative spacing, so elements overlap on
      purpose. Stacked avatars and overlapping cards work, including
      inside a container that shrinks to fit them.
- [x] **Show, hide, and clip** — hiding an element removes it from the
      layout and its neighbours close up; a container crops what overflows
      it, round corners included.
- [x] **Components, instances and variants** — a component defined once and
      reused, with named states switched at runtime and the layout change
      animating. Components from a shared library file resolve too, but
      three cases fail the whole import today: a library component
      containing an image, one that instances a component from a second
      library, and one whose own referenced component the library data does
      not carry.
- [ ] **Baseline alignment** — **part built.** A row of mixed-size text
      lines up on the line the letters sit on. Two cases are not corrected
      and produce bottom-of-box alignment with no warning: a nested
      container inside such a row, and a row that wraps.
- [ ] **Rotation** — refused. Nothing in the system draws an element at an
      angle, so a rotated element is reported at import rather than drawn
      upright and wrong.
- [ ] **Absolutely-positioned children, layout-consuming strokes, and
      reversed paint order** — each refused by name. Defaulting any of them
      would silently move the elements around it.

## 2. Text and typography

Checked against `crates/dashscene-typeset/src/text/mod.rs`,
`crates/dashc/src/figma/mod.rs`, `crates/dashscene-validator/src/`.

- [x] **Latin and Arabic** — measurement, shaping, wrapping and drawing,
      with correct contextual letter forms and joining for Arabic.
- [x] **Mixed-direction text** — right-to-left and left-to-right in one
      paragraph, ordered correctly, with digit shapes chosen from context.
- [x] **Text drives layout** — a box grows to fit its text; a fixed-width
      box wraps and grows taller. Line wrapping happens at spaces and
      explicit line breaks only: no hyphenation, no mid-word breaking, and
      a word wider than its box overflows.
- [x] **Type controls** — line height, letter spacing, and horizontal and
      vertical alignment within the text box. Where a line mixes fonts, its
      height spans the tallest letters and deepest tails on that line.
- [x] **Several fonts and weights per style** — four Latin weights; a
      character missing from the first font is drawn from the next, and a
      weight the build does not carry is reported by name rather than
      silently substituted. A character **no** font covers is not reported:
      it draws as the font's empty box. See section 8.
- [x] **Sharp at any size, with a legibility floor** — letters are stored
      as shapes rather than fixed-size images. Below 14 pixels per em they
      smear, and the import warns. Recording a decision to accept that is
      designed but not usable yet — see the waiver item in section 8.
- [ ] **Several styles inside one text block** — planned (v1). A text
      element carries one style, so bold-inside-a-sentence needs separate
      elements.
- [ ] **Letter case applied as a property** — refused. There is no
      vocabulary for it anywhere in the system. Type the text in the case
      you want.
- [ ] **Scripts beyond Latin and Arabic** — planned (v1) as one piece of
      work, because the glyph-storage design cannot be settled without
      knowing which scripts it must hold. Arabic ships in one weight
      against Latin's four.

## 3. Visual styling and effects

Checked against `crates/dashc/src/figma/mod.rs`,
`crates/dashscene-validator/src/triage.rs`, `crates/dashpaint/src/lib.rs`,
`crates/dashscene-skia/src/`, `crates/dashscene-gpu/src/`.

- [x] **Fills** — solid, and four gradient types: linear, radial, angular
      (which is what makes a gauge sweep possible) and diamond. Images fill
      a shape in four modes — fill, fit, crop and tile — with a crop
      transform.
- [x] **Strokes and corners** — one width per shape, aligned inside,
      centred or outside the edge, with each corner rounded independently.
- [x] **Shadows** — drop and inner, any number on one element, each with
      its own colour, offset, blur and spread. Not available on text or on
      imported vector artwork: a shadow there is reported at import.
- [x] **Backdrop blur** — frosted-glass panels that blur what is behind
      them, and keep doing so while they move.
- [x] **Masks and opacity** — a shape stencils the shapes after it; an
      element or a whole group can be faded, and an overlapping group is
      composited separately so the fade looks right.
- [x] **Vector artwork** — vector shapes from the design file are converted
      at build time into a form that stays sharp at any size.
- [ ] **Stacking several fills on one shape** — **part built.** Works on
      frames, rectangles, instances, sections and groups. On a circle, on
      text, or on imported vector artwork a second fill is reported at
      import and the element is not drawn at all.
- [ ] **Blend modes, layer blur, corner smoothing, luminance masks, dashed
      and variable-width strokes, noise and progressive blur, and boolean
      shape operations** — none of these are drawn, and each is reported by
      name with a workaround. What the report does depends on the
      construct: blend modes, noise and progressive blur stop the file
      compiling; the rest drop the element and let the file emit, because
      the importer's default is to carry on rather than stop. A stricter
      mode exists and is not the default.
- [ ] **Different stroke widths per side** — **the one gap in this section
      that is not reported.** Nothing detects it. The design compiles using
      whichever single width the file reports, with no message. Workaround:
      four edge rectangles.

## 4. Motion and interaction

Checked against `crates/dashcue/`, `crates/dashlang/src/reactive.rs`,
`crates/dashscene-engine/src/flip.rs`.

- [x] **Animation is described, not programmed** — nothing anyone writes
      runs inside the frame loop, which is what makes the per-frame cost
      knowable in advance rather than discovered on the device.
- [x] **Tweens, springs and keyframes** — four easing curves, springs
      described by stiffness and damping in the same form Jetpack Compose
      uses, keyframes including overshoot, and a delay between successive
      elements.
- [x] **Interruptible, and reproducible** — retargeting a running animation
      continues from where it is and a spring keeps its speed, with no
      visible jump; and the same inputs produce the same frames on every
      machine, so animation can be tested rather than eyeballed.
- [x] **Layout transitions** — when a variant switch changes the layout,
      elements animate from where they were to where they are now.
- [x] **Values from the running application** — a number drives position,
      size, spacing, opacity, or one channel of a fill colour; a true/false
      value drives visibility; and a number can be turned into text through
      any function of it, then shown. The designer declares the connection
      as a Figma Variable and the application writes it by name.
- [ ] **Gauges and radial motion** — a value driving a rotation about a
      pivot, or an arc sweep. Designed, and a v1 candidate.
- [ ] **Known limit, and it is worse than it sounds** — a frame that
      changes only live text clears **all** the text on the screen,
      including the string that just changed, unless something on that same
      frame also changes the layout. There is a documented way to author
      around it, and a fix is tracked.

## 5. Runtime performance

Checked against `crates/dashscene-core/src/arena.rs`,
`crates/dashlang/src/reactive.rs`, `crates/dashscene-engine/src/lib.rs`.

- [x] **Everything shared is computed once** — layout and text placement
      run a single time per change, for every renderer, so two renderers
      cannot disagree about where anything sits.
- [x] **Cost scales with the change, not the screen** — a change that
      affects only appearance skips layout entirely, and a change that
      provably cannot move anything else replays cached geometry without
      running the layout step at all.
- [x] **The renderer is told what changed, and that list is proven
      correct** — a second drawing mode models the update and is checked
      pixel for pixel against the ordinary one, so a missed entry is a
      caught bug rather than a flicker found on a device months later.
- [x] **Startup cost tracks the screen shown, not the file size** — when a
      desktop host opens a design file by name. Showing one screen out of a
      sixty-five-screen file then costs the same as showing it out of a
      one-screen file. Not yet true in a browser, and not the path the
      demonstration takes by default. See section 6.
- [ ] **Tuned against the target device** — planned (v1). Around twenty
      specific improvements are identified and deliberately held: none has
      a frame budget or a device measurement behind it, so fixing one now
      would produce a change whose only success criterion is that the tests
      still pass.

## 6. The design file format

Checked against `crates/dashbuf/src/container.rs`, `bank.rs`, `prefix.rs`,
`crates/dashscene-core/src/load.rs`.

- [x] **One format for both sources** — a screen from Figma and one written
      in code compile to the same kind of file. For the layout and
      solid-fill vocabulary both express, the same screen authored either
      way produces a byte-identical image; extending that proof to text and
      live values is planned (v1).
- [x] **The file carries intent, never results** — no baked-in positions,
      no pixels, no letter placements. That is what lets one file serve
      devices with different screens and different renderers.
- [x] **Reproducible** — the same design always compiles to a
      byte-identical file. Caching, integrity checking and future signing
      all depend on this.
- [x] **Split into a small head and a bulk tail** — everything needed to
      lay out and draw sits at the front; images sit behind a page boundary
      so a device can verify what it needs without touching the rest.
      Mapping the file rather than reading it into memory is built, and is
      the path a desktop host takes when it opens a file by name. Both
      demonstrations take an older copying path by default, and so does the
      browser — which is what section 5's caveat and the item below are
      about.
- [x] **Assets identified by content** — an image is named by what it is,
      not where it sits, so payloads can be swapped for a different device
      build without touching the design. The name is a hash, and it is
      re-checked when the bytes are read, not just used to find them.
- [ ] **Startup cost proportional to what is shown, in a browser** —
      **part built.** On the desktop path this is done and measured:
      showing one screen costs 197 387 bytes whether the file holds one
      screen or sixty-five, asserted as an equality by a test that fails
      the build if it changes. The browser host still loads the older way —
      it fetches every image the file names, one request each, before it
      draws — so a sixty-five-screen file costs sixty-five downloads to
      show one. Being worked in the current slice.
- [ ] **Placeholders for content still loading** — planned (v1). Blocked on
      deciding what a not-yet-loaded image should show, which the design
      source does not supply.

## 7. Images, fonts and asset preparation

Checked against `crates/dashpack/`, `crates/dashbuf/src/bank.rs`,
`crates/dashscene-typeset/src/atlas/`, `crates/dashpaint/src/image_id.rs`,
`importers/figma/src/images.ts`, `goldens/tooling/src/profile.rs`.

**Read the last item in this section first.** The capabilities below are
implemented and covered by tests. The `dashpack` binary does not run them —
it reports its pinned versions and exits without packing anything — so
there is no packing tool to put in a build pipeline. The repository's own
preview and comparison commands do reach the packing code directly, which
is how the quality profiles get measured at all.

- [x] **Three quality profiles** — RAW (untouched), HiFi and LoFi. Each is
      a measured quality band rather than a fixed format: the packer walks
      a ladder of encodings and takes the cheapest that stays inside the
      band. One deliberate exception, disclosed rather than hidden — on
      photographs, HiFi stops at the finest lossy step even when that step
      sits outside its band, because the alternative quadruples memory.
- [x] **Every band ships with the change that breaks it** — a quality
      threshold nobody has seen fail is not a contract, so each is stored
      with the measured degradation that trips it.
- [x] **Textures the graphics hardware reads directly** — compressed into a
      form sampled without unpacking, which is what keeps memory down
      rather than only file size. Text and icon artwork is never
      compressed: too much legibility risk, too little gain.
- [x] **One set of sources, several device builds** — a derived build plus
      a record of exactly what was substituted, and the design half of the
      file is byte-for-byte unchanged by the quality choice, so a quality
      change cannot silently move a layout.
- [x] **Formats identified from content, never from a label** — a file
      whose contents contradict its declared format, or whose real size
      differs from the recorded one, is an error. PNG, JPEG and static GIF
      are accepted; an animated GIF is refused by name.
- [ ] **A way to run the packer** — the command exists and reports which
      encoder and container version it is pinned to, but it packs nothing.
      Until this lands, no build pipeline can produce a packed bank.
- [ ] **A memory budget for a device** — planned (v1). No number exists in
      the specification, so a build can succeed and still not fit, and
      nothing detects it. A stated, accepted gap.
- [ ] **Font preparation without an external tool** — glyph preparation
      runs a separately installed program (`msdf-atlas-gen`), so a build
      machine needs it. Texture encoding, by contrast, is built in.

## 8. Where designs come from

Checked against `importers/figma/src/`, `crates/dashc/src/`,
`crates/dashscene-validator/src/`.

- [x] **Figma import** — through Figma's own API, compiled by the same code
      that compiles everything else. Auto-layout, components, instances,
      variants, text, shapes, images and effects all import.
- [x] **Design tokens and designer intent** — Figma Variables reach the
      running application both as values and by name, without a plugin. A
      companion plugin lets a designer mark scaffolding that should not
      ship. Which elements are screens is chosen by whoever runs the
      import, not marked in Figma.
- [x] **Nothing is dropped silently** — every unsupported construct is a
      named message naming the workaround. This is enforced as a design
      principle rather than a habit, with **two** known exceptions: stroke
      widths that differ per side (section 3), and a character no font in
      the cascade covers, which draws as an empty box with nothing
      reported.
- [x] **Authoring in code** — a Rust interface fills the same model
      directly, with no file in between, and the two routes are proven to
      produce the same result over the vocabulary both express.
- [ ] **Waiving a warning per target** — **part built.** The rule exists
      and is tested: a waiver covers one rule at one place, and a release
      build should refuse a warning nobody waived. Nothing calls it and
      there is no format for writing waivers down, so today a warning
      blocks nothing.
- [ ] **A supported command that compiles a Figma file** — commands exist
      and work, but they are the repository's own development recipes
      rather than a shipped tool with a stable interface. Making the
      compiler a product is planned (v1) — see section 11.
- [ ] **Authoring from inside a game engine** — planned (v1), alongside the
      Unity renderer.

## 9. Rendering backends

Checked against `crates/dashpaint/src/lib.rs`,
`crates/dashscene-skia/src/`, `crates/dashscene-gpu/src/`,
`crates/dashscene-unity/src/lib.rs`.

- [x] **One renderer contract** — every renderer consumes the same finished
      data and only colours it in: it never measures text, wraps lines or
      moves anything. That is why two renderers cannot drift apart on
      layout — they are never given the chance to disagree.
- [x] **Reference renderer** — draws the whole feature set on the main
      processor, which makes its output exactly reproducible and therefore
      usable as the reference the others are compared against. It stays
      permanently for that reason.
- [x] **Lean GPU renderer** — draws every shape as a rectangle whose colour
      is computed by a small formula on the graphics card, rather than by
      tracing outlines, which keeps the data moved per frame low. It draws
      the whole feature set, on a computer and in a browser, from one
      codebase.
- [x] **The two agree** — measured on a developer machine (an Apple M3),
      not on a target device. One set of tolerance bands serves both, and
      no reference picture had to change. How either behaves on the
      hardware the product ships on has not been measured.
- [ ] **Unity renderer** — **part built.** The contract keeping the data
      readable from C# exists and the build enforces it. The renderer, its
      shader library and the C# projection are planned (v1), in a separate
      repository that does not exist yet.
- [ ] **Browsers without WebGPU** — WebGPU is the newer browser graphics
      standard the lean renderer needs. A browser lacking it is told so and
      draws nothing. Supporting the older standard is a redesign, and a v1
      question that depends on which browsers the product must reach.

## 10. Platform support

Checked against `demo/`, `demo-web/`, `crates/dashscene-web/`,
`crates/dashscene-unity/src/lib.rs`, and the absence of any mobile target in
the workspace.

- [x] **Desktop** — a windowed host with an event loop, pointer and
      keyboard input, and the showcase running in it. Built and run on
      macOS and Linux. Windows is expected to work, because nothing in the
      host is platform-specific, but it has never been built or tested —
      there is no Windows job in automation and no Windows-specific code.
- [x] **Browsers with WebGPU** — the same showcase on a canvas, with the
      design file fetched in pieces rather than as one download. It still
      fetches every image the file names, one request each, before it draws
      — see section 6.
- [ ] **Embedding in a real application** — **part built, and the current
      slice**, which covers the browser and desktop only. **The browser half
      is built**: story #741 made `crates/dashscene-web` the web integration
      crate, so a browser embedder consumes the canvas handoff, the frame
      loop, resize rebuilding and the byte-range load rather than copying them
      out of `demo-web/`. The desktop half is not — `demo/` is still
      `publish = false` and holds both halves, which is story #794. Nothing is
      published either way: this slice makes the crates publishable and the
      publish is a separate decision (epic #793).
- [ ] **A C interface for hosts written in other languages** — **part
      built.** The data a host would consume is already in a shape another
      language can read, and the build enforces that. Nothing is built on
      top of it. Planned for the slice that brings up Android, since both
      need it and neither the browser nor the desktop does.
- [ ] **Android and iOS** — nothing yet: no build target, no toolchain, no
      automation. Android is a later slice than the current one, over a
      proposed three-layer structure; iOS and the Unity host follow after
      it.

## 11. Quality tooling and workflow

Checked against `goldens/`, `.github/workflows/ci.yml`, `justfile`,
`crates/dashscene-typeset/tests/atlas_pipeline.rs`.

- [x] **Reference pictures for every construct** — committed images the
      build compares against, so a change that alters a pixel must be
      declared rather than discovered.
- [x] **Compared against Figma's own render** — seven frames measured
      against real Figma captures, each inside a declared tolerance, plus
      ten more self-authored frames covering what those seven do not reach.
      This has already caught two real bugs on first measurement.
- [x] **Every tolerance ships with the change that breaks it** — the same
      discipline as the asset quality bands, for the same reason.
- [x] **Three test tiers and one gate** — about five seconds between edits,
      thirty-three before pushing, and fifty-four for the run that
      re-derives the asset tables; plus a single check asserting all seven
      qualification criteria on one commit, with its membership pinned by
      name so a renamed test cannot leave the gate silently.
- [x] **Glyph preparation is checked on two processor architectures** —
      within a measured tolerance, not byte for byte: the tool's arithmetic
      differs between architectures by about one step per channel, and the
      check admits that noise and nothing more.
- [ ] **The compiler as a shipped product** — planned (v1): a stable
      command line, versioned diagnostics, a waiver workflow, lint rule
      packs and reporting for design review. Today it is an internal tool.
- [ ] **Anything that catches "it looks wrong on a real automotive
      driver"** — the only check that could is a measurement on the target
      device, and it is not automated.

## 12. Build, delivery and integrity

Checked against `crates/dashbuf/src/container.rs`, `prefix.rs`,
`crates/dashscene-gpu/Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml`.

- [x] **Reproducible builds** — the same design always produces a
      byte-identical file. Everything below depends on this holding.
- [x] **Checked before it is trusted** — the file's version and per-section
      cryptographic hashes are verified before anything in it is read, by
      code that deliberately depends on no parser and takes the writer's
      word for nothing. A field this version does not understand must be
      zero, or the file is refused.
- [x] **Assets authenticated, not just addressed** — the content hash is
      re-computed and compared when the bytes are read, so a substituted
      payload does not resolve.
- [x] **A small trusted surface by design** — image decoders are the
      classic source of vulnerabilities in a system like this, and the lean
      renderer links one rather than three. Note the intended pipeline that
      converts the other formats away cannot be run yet — see section 7.
- [x] **Nothing is fetched while drawing** — the browser host downloads
      before it draws, never during.
- [x] **Pinned dependency versions and a committed lock file.**
- [ ] **Signing** — the format reserves the header fields for a signature
      and refuses any file that uses them today. No signing tool, key
      handling or verification policy exists. The largest gap in this
      section.
- [ ] **An automated dependency audit** — the audit exists as a local
      command and runs from a developer's push hook. It has never been a
      continuous-integration job, so nothing checks it centrally.
- [ ] **Over-the-air delivery, and remote or streamed screens** — v2. The
      architecture is shaped for streaming and today's interfaces are
      constrained so it does not become a breaking change.

---

## Two things worth stating plainly

**Nothing here is released.** This is a private working repository. The
public package names were reserved before development started and hold no
code.

**The automated checks exist and are not currently running.** Continuous
integration is blocked at the account level: every job fails within seconds
having executed no steps, measured 2026-08-07. It is a billing block rather
than a code failure, but the consequence is real — recent work was verified
by running the same checks locally, with the evidence recorded on each
change. A ticked box means the tests exist and pass, not that a machine
other than a developer's ran them.

## What this document is not

It is not a schedule. No line carries a date, and the order inside a
section is by topic. [`docs/roadmap.md`](roadmap.md) carries the
sequencing.

It is not the evidence. A ticked box here is a summary;
[`docs/specification/05-qualification.md`](specification/05-qualification.md)
is where a claim is proven, and the design records under
[`docs/design/`](design/) describe behaviour and edge cases — with the
caveat this file opened with: those records have themselves drifted from
the code, so the code is the authority when they disagree.
