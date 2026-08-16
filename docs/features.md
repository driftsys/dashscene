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

dashscene turns a screen designed in Figma — or written in code — into pixels,
on more than one kind of device, with every device agreeing to the pixel about
where each rectangle and each letter sits.

This file answers one question the engineering records answer badly: can this
screen be built, and if not, what happens instead. The roadmap is organised by
delivery slice and the design records by component; neither is a list a designer
can scan for the effect they wanted to use.

## How to read this

- **`- [x]`** — built and working today.
- **`- [ ]` with a bold "part built" note** — some of it works. The line says
  which part, and what is missing.
- **`- [ ]`** — not built. The line says whether it is planned, or refused on
  purpose.

**Refused on purpose is a real answer, not a gap.** A construct this system will
not draw is reported by name when the design is imported, with the workaround,
rather than dropped quietly or approximated. That is one of the five principles
the system is built on, and it is why several lines say "refused" rather than
"not yet".

**Where a feature has a limit, the limit is on the same line.** A tick with a
caveat attached is the normal case here, not an exception.

## How this file is kept honest

This file is deliberately short. An earlier draft listed 186 features, and three
review rounds found factual errors in it at a rate that was not falling — most
of them because the claims had been written from this repository's own design
and specification records, which had themselves drifted from the code in four
places. A catalogue that is 95 % accurate is worse than a short one that is
right, because nobody can tell which 5 % they are reading.

So the claim surface was halved and **every line below was checked against the
code that implements it**, with each section naming where to check. A fourth
review round then found errors in that too, at a similar rate, which is the
honest thing to know about this file:

- **Nothing enforces this mechanically.** No test fails when a line here goes
  stale. It is re-checked by hand, and hand-checking has a demonstrated error
  rate on this material.
- **The recurring mistake is depth, not honesty.** Checking that something
  exists is not checking which branches it does not cover, what the default path
  does, or whether any command reaches it. Several corrected lines were wrong in
  exactly that way.
- **Sequencing labels are the one exception to "checked against code".**
  "Planned (v1)" and "v2" come from [`docs/roadmap.md`](roadmap.md); no code can
  substantiate a schedule.
- **Do not promise a customer a ticked box** without reading the code named
  under the section, or asking.

This file lists unbuilt work on purpose, which deviates from the house rule that
a shipped document describes the system as-built. `docs/design/architecture.md`
takes the same deviation for its own reason. The rule's real concern is that an
unbuilt thing must never be described as built; every unbuilt line below is
marked unbuilt and says what is missing.

---

## 1. Layout and structure

Checked against `crates/dashscene-engine/src/lib.rs`,
`crates/dashscene-core/src/arena.rs`, `crates/dashc/src/figma/mod.rs`.

- [x] **Rows, columns, wrapping rows and grids** — including grid items that
      span more than one cell. Three auto-layout constructs are refused rather
      than lowered: distributing wrapped lines, a grid track sized `auto` or
      `min-content`, and a fill-sized child on an axis its parent shrinks to
      fit.
- [x] **Three sizing rules, with minimums and maximums** — hug (shrink to
      contents), fill (take the available space), fixed (an exact number), on
      either axis, with spacing, padding and alignment.
- [x] **Deliberate overlap** — negative spacing, so elements overlap on purpose.
      Stacked avatars and overlapping cards work, including inside a container
      that shrinks to fit them.
- [x] **Show, hide, and clip** — hiding an element removes it from the layout
      and its neighbours close up; a container crops what overflows it, round
      corners included.
- [x] **Components, instances and variants** — a component defined once and
      reused, with named states switched at runtime and the layout change
      animating. Components from a shared library file resolve too, but three
      cases fail the whole import today: a library component containing an
      image, one that instances a component from a second library, and one whose
      own referenced component the library data does not carry.
- [ ] **Baseline alignment** — **part built.** A row of mixed-size text lines up
      on the line the letters sit on. Two cases are not corrected and produce
      bottom-of-box alignment with no warning: a nested container inside such a
      row, and a row that wraps.
- [ ] **Rotation** — **part built.** An element without children draws at an
      angle, about a stated turning point, through **both** painters, and an
      imported rotated element is measured against the design tool's own render
      pixel for pixel. Three gaps, each reported rather than drawn wrong: a
      rotated element **containing** other elements is refused, because a
      rotation here does not apply to what is inside it; a **mirrored** element
      — one flipped left-to-right or top-to-bottom — is refused, because there
      is no way to say "mirrored" here and drawing it unflipped would be a
      picture the designer never drew; and scale and skew remain absent.
- [ ] **Absolutely-positioned children, layout-consuming strokes, and reversed
      paint order** — each refused by name. Defaulting any of them would
      silently move the elements around it.

## 2. Text and typography

Checked against `crates/dashscene-typeset/src/text/mod.rs`,
`crates/dashc/src/figma/mod.rs`, `crates/dashscene-validator/src/`.

- [x] **Latin and Arabic** — measurement, shaping, wrapping and drawing, with
      correct contextual letter forms and joining for Arabic.
- [x] **Mixed-direction text** — right-to-left and left-to-right in one
      paragraph, ordered correctly, with digit shapes chosen from context.
- [x] **Text drives layout** — a box grows to fit its text; a fixed-width box
      wraps and grows taller. Line wrapping happens at spaces and explicit line
      breaks only: no hyphenation, no mid-word breaking, and a word wider than
      its box overflows.
- [x] **Type controls** — line height, letter spacing, and horizontal and
      vertical alignment within the text box. Where a line mixes fonts, its
      height spans the tallest letters and deepest tails on that line.
- [x] **Several fonts and weights per style** — four Latin weights; a character
      missing from the first font is drawn from the next, and a weight the build
      does not carry is reported by name rather than silently substituted. A
      character **no** font covers is not reported: it draws as the font's empty
      box. See section 8.
- [x] **Sharp at any size, with a legibility floor** — letters are stored as
      shapes rather than fixed-size images. Below 14 pixels per em they smear,
      and the import warns. Recording a decision to accept that is designed but
      not usable yet — see the waiver item in section 8.
- [ ] **Several styles inside one text block** — planned (v1). A text element
      carries one style, so bold-inside-a-sentence needs separate elements.
- [ ] **Letter case applied as a property** — refused. There is no vocabulary
      for it anywhere in the system. Type the text in the case you want.
- [ ] **Scripts beyond Latin and Arabic** — planned (v1) as one piece of work,
      because the glyph-storage design cannot be settled without knowing which
      scripts it must hold. Arabic ships in one weight against Latin's four.

## 3. Visual styling and effects

Checked against `crates/dashc/src/figma/mod.rs`,
`crates/dashscene-validator/src/triage.rs`, `crates/dashpaint/src/lib.rs`,
`crates/dashscene-skia/src/`, `crates/dashscene-gpu/src/`.

- [x] **Fills** — solid, and four gradient types: linear, radial, angular (which
      is what makes a gauge sweep possible) and diamond. Images fill a shape in
      four modes — fill, fit, crop and tile — with a crop transform.
- [x] **Strokes and corners** — one width per shape, aligned inside, centred or
      outside the edge, with each corner rounded independently.
- [x] **Shadows** — drop and inner, any number on one element, each with its own
      colour, offset, blur and spread. Not available on text or on imported
      vector artwork: a shadow there is reported at import.
- [x] **Backdrop blur** — frosted-glass panels that blur what is behind them,
      and keep doing so while they move.
- [x] **Masks and opacity** — a shape stencils the shapes after it; an element
      or a whole group can be faded, and an overlapping group is composited
      separately so the fade looks right.
- [x] **Vector artwork** — vector shapes from the design file are converted at
      build time into a form that stays sharp at any size.
- [ ] **Stacking several fills on one shape** — **part built.** Works on frames,
      rectangles, instances, sections and groups. On a circle, on text, or on
      imported vector artwork a second fill is reported at import and the
      element is not drawn at all.
- [ ] **Blend modes, layer blur, corner smoothing, luminance masks, dashed and
      variable-width strokes, noise and progressive blur, and boolean shape
      operations** — none of these are drawn, and each is reported by name with
      a workaround. What the report does depends on the construct, in three
      groups rather than two: blend modes, noise and progressive blur stop the
      file compiling; **layer blur and corner smoothing leave the element drawn
      and report the one property**, because they travel the validator's triage
      path where the node still lowers; and luminance masks, dashed and
      variable-width strokes and boolean operations drop the element and let the
      file emit, because the importer's default is to carry on rather than stop.
      A stricter mode exists and is not the default.
- [ ] **Different stroke widths per side** — **the one gap in this section that
      is not reported.** Nothing detects it. The design compiles using whichever
      single width the file reports, with no message. Workaround: four edge
      rectangles.

## 4. Motion and interaction

Checked against `crates/dashcue/`, `crates/dashlang/src/reactive.rs`,
`crates/dashscene-engine/src/flip.rs`, `crates/dashscene-core/src/bindings.rs`,
and the motion tables in `crates/dashbuf/schema/dashbuf.fbs`.

- [x] **Animation is described, not programmed** — nothing anyone writes runs
      inside the frame loop, which is what makes the per-frame cost knowable in
      advance rather than discovered on the device.
- [x] **Tweens, springs and keyframes** — four easing curves, springs described
      by stiffness and damping in the same form Jetpack Compose uses, keyframes
      including overshoot, and a delay between successive elements.
- [x] **Interruptible, and reproducible** — retargeting a running animation
      continues from where it is and a spring keeps its speed, with no visible
      jump; and the same inputs produce the same frames on every machine, so
      animation can be tested rather than eyeballed.
- [x] **Layout transitions** — when a variant switch changes the layout,
      elements animate from where they were to where they are now.
- [x] **The motion ships in the file** — a transition is carried in the design
      file rather than written in code beside it, so an animation travels with
      the design. Before this, the file held the two ends of a change and the
      wiring, and the motion between them had to be written in Rust.
- [x] **Ambient motion** — a shimmer, spinner or pulse that runs without
      anything triggering it: one channel of one element repeating a curve, with
      an offset that staggers a row of them out of step. **It is restricted to
      appearance, and the restriction is enforced at load** — a fill channel,
      opacity, or a rotation and its pivot. A loop on position or size is
      refused by name, so a "breathing" effect must be built from opacity or a
      fill rather than from width and height. It is also refused if the curve is
      a spring, if the same channel is already driven by something else, or if
      the element's fill is a gradient or an image rather than a solid colour.

      Figma **can** author this class, with a timeout-triggered variant
      switch; dashscene does not import that trigger, so an ambient effect
      is written in code or in the file rather than brought in from a
      design.
- [x] **Values from the running application** — the designer declares the
      connection as a Figma Variable and the application writes it by name.
      **What a Figma Variable carries today is spacing, opacity and one channel
      of a fill colour.** Position, size, rotation and the pivot are drivable,
      but only from code or a hand-authored file — the importer recognises three
      property paths and nothing else.
- [ ] **A true/false value driving visibility, and a number shown as text** —
      **part built.** Both work from code. Neither survives a Figma import: the
      importer names those rows itself and does not send them, and the file
      format carries no numeric-to-text transform. Planned (v1), tracked as
      issues #252 and #256.
- [x] **Rotation about a pivot** — an element turns about a point given
      explicitly rather than about its centre, which is what both Figma and SVG
      mean by a rotation. The angle and both coordinates of the pivot are each
      drivable, so a gauge needle is expressible — from code, per the item
      above.
- [ ] **Arc sweep** — a value driving the sweep of an arc, which is the other
      half of a gauge. Not built; no such property exists. Designed, and a v1
      candidate.
- [ ] **Known limit, and it is worse than it sounds** — a frame that changes
      only live text clears **all** the text on the screen, including the string
      that just changed, unless something on that same frame also changes the
      layout. There is a documented way to author around it, and a fix is
      tracked.
- [ ] **Known limit: a switch animates position and size, not colour** — when a
      variant switch changes a fill, a rotation or whether something is visible,
      the change **applies at the switch rather than animating**, while position
      and size changes animate normally. The element ends up correct either way;
      it arrives immediately rather than travelling.

      **You are told this rather than left to find it.** A file that asks
      for such an animation is refused by name when it loads, and the Figma
      importer reports it as a motion degrade rather than emitting it. So
      nothing is silently dropped, and there is no runtime behaviour to
      debug — the compiler has already said so.

      Why it cannot simply be lifted: the destination's appearance is
      resolved ahead of the value travelling towards it, so every sample of
      such an animation would be masked by the state it is heading for.
      That was built, measured and reverted, and is tracked as issue #891.

## 5. Runtime performance

Checked against `crates/dashscene-core/src/arena.rs`,
`crates/dashlang/src/reactive.rs`, `crates/dashscene-engine/src/lib.rs`.

- [x] **Everything shared is computed once** — layout and text placement run a
      single time per change, for every renderer, so two renderers cannot
      disagree about where anything sits.
- [x] **Cost scales with the change, not the screen** — a change that affects
      only appearance skips layout entirely, and a change that provably cannot
      move anything else replays cached geometry without running the layout step
      at all.
- [x] **The renderer is told what changed, and that list is proven correct** — a
      second drawing mode models the update and is checked pixel for pixel
      against the ordinary one, so a missed entry is a caught bug rather than a
      flicker found on a device months later.
- [x] **Startup cost tracks the screen shown, not the file size** — when a
      desktop host opens a design file by name. Showing one screen out of a
      sixty-five-screen file then costs the same as showing it out of a
      one-screen file. **Conditionally true in a browser** since story #792, and
      the condition is the point: the runtime draws every screen in a file, not
      only the one being shown, so a browser — which has no bytes it did not
      download — can skip a screen's images only when no other screen uses any.
      See section 6.
- [ ] **Tuned against the target device** — planned (v1). Around twenty specific
      improvements are identified and deliberately held: none has a frame budget
      or a device measurement behind it, so fixing one now would produce a
      change whose only success criterion is that the tests still pass.

## 6. The design file format

Checked against `crates/dashbuf/src/container.rs`, `bank.rs`, `prefix.rs`,
`crates/dashscene-core/src/load.rs`.

- [x] **One format for both sources** — a screen from Figma and one written in
      code compile to the same kind of file. For the layout and solid-fill
      vocabulary both express, the same screen authored either way produces a
      byte-identical image; extending that proof to text and live values is
      planned (v1).
- [x] **The file carries intent, never results** — no baked-in positions, no
      pixels, no letter placements. That is what lets one file serve devices
      with different screens and different renderers.
- [x] **Reproducible** — the same design always compiles to a byte-identical
      file. Caching, integrity checking and future signing all depend on this.
- [x] **Split into a small head and a bulk tail** — everything needed to lay out
      and draw sits at the front; images sit behind a page boundary so a device
      can verify what it needs without touching the rest. Mapping the file
      rather than reading it into memory is built, and is the path a desktop
      host takes when it opens a file by name. The browser takes the equivalent
      path since story #792 — it binds ranges into a buffer of the pieces it
      downloaded rather than copying each one — and what is left of the caveat
      is the item below.
- [x] **Assets identified by content** — an image is named by what it is, not
      where it sits, so payloads can be swapped for a different device build
      without touching the design. The name is a hash, and it is re-checked when
      the bytes are read, not just used to find them.
- [ ] **Startup cost proportional to what is shown, in a browser** — **part
      built, and the remaining part is not in the browser.** On the desktop path
      this is done and measured: showing one screen costs 197 387 bytes whether
      the file holds one screen or sixty-five, asserted as an equality by a test
      that fails the build if it changes. The browser downloads only the images
      the shown screen uses since story #792, asserted the same way — **but only
      when no other screen in the file uses an image**. When one does, it
      downloads every image any screen uses, and says which it did.

      The reason is not in the loader. Everything below it draws every
      screen in the file rather than the one being shown, so a screen the
      browser skipped downloading is still a screen something asks for
      pixels of. A desktop host survives that because a mapped file makes
      every byte addressable whether or not it was checked; a browser has
      no such thing. Issue #822 is the change that would remove the
      condition, and it is in the runtime rather than in either host.
- [ ] **Placeholders for content still loading** — planned (v1). Blocked on
      deciding what a not-yet-loaded image should show, which the design source
      does not supply.

## 7. Images, fonts and asset preparation

Checked against `crates/dashpack/`, `crates/dashbuf/src/bank.rs`,
`crates/dashscene-typeset/src/atlas/`, `crates/dashpaint/src/image_id.rs`,
`importers/figma/src/images.ts`, `goldens/tooling/src/profile.rs`.

**Read the last item in this section first.** The capabilities below are
implemented and covered by tests. The `dashpack` binary does not run them — it
reports its pinned versions and exits without packing anything — so there is no
packing tool to put in a build pipeline. The repository's own preview and
comparison commands do reach the packing code directly, which is how the quality
profiles get measured at all.

- [x] **Three quality profiles** — RAW (untouched), HiFi and LoFi. Each is a
      measured quality band rather than a fixed format: the packer walks a
      ladder of encodings and takes the cheapest that stays inside the band. One
      deliberate exception, disclosed rather than hidden — on photographs, HiFi
      stops at the finest lossy step even when that step sits outside its band,
      because the alternative quadruples memory.
- [x] **Every band ships with the change that breaks it** — a quality threshold
      nobody has seen fail is not a contract, so each is stored with the
      measured degradation that trips it.
- [x] **Textures the graphics hardware reads directly** — compressed into a form
      sampled without unpacking, which is what keeps memory down rather than
      only file size. Text and icon artwork is never compressed: too much
      legibility risk, too little gain.
- [x] **One set of sources, several device builds** — a derived build plus a
      record of exactly what was substituted, and the design half of the file is
      byte-for-byte unchanged by the quality choice, so a quality change cannot
      silently move a layout.
- [x] **Formats identified from content, never from a label** — a file whose
      contents contradict its declared format, or whose real size differs from
      the recorded one, is an error. PNG, JPEG and static GIF are accepted; an
      animated GIF is refused by name.
- [ ] **A way to run the packer** — the command exists and reports which encoder
      and container version it is pinned to, but it packs nothing. Until this
      lands, no build pipeline can produce a packed bank.
- [ ] **A memory budget for a device** — planned (v1). No number exists in the
      specification, so a build can succeed and still not fit, and nothing
      detects it. A stated, accepted gap.
- [ ] **Font preparation without an external tool** — glyph preparation runs a
      separately installed program (`msdf-atlas-gen`), so a build machine needs
      it. Texture encoding, by contrast, is built in.

## 8. Where designs come from

Checked against `importers/figma/src/`, `crates/dashc/src/`,
`crates/dashscene-validator/src/`.

- [x] **Figma import** — through Figma's own API, compiled by the same code that
      compiles everything else. Auto-layout, components, instances, variants,
      text, shapes, images and effects all import.
- [ ] **Prototype interactions** — **part built.** A Figma prototype's
      interactions become the switches between variants and its Smart Animate
      transitions become the motion between them, but only along a narrow path.
      Everything outside it is reported by name; nothing is approximated.
      **Three limits, and they do not behave the same way:**

      **Only a click that changes to another variant lowers at all.** Any
      other trigger — hover, timeout, drag, key press — and any other
      action or navigation is dropped whole, taking its switch with it.
      Only Smart Animate carries motion; Figma's four spring presets and
      its custom bezier are refused, so a switch authored with Figma's
      default spring easing lands in one frame.

      **A variant set lowers only when every difference between its members
      is one the format can express.** A member with an extra child, a
      different corner radius or a different auto-layout mode makes the
      whole set unlowerable.

      **Of the differences that do lower, only position and size animate** —
      a fill, rotation or visibility difference is carried by the switch and
      arrives immediately. See the known limit in section 4.

      **Only part of the first refuses the file under the strict setting; the
      other two never do.** Limit 1 folds two behaviours together, and they
      differ: a refused trigger, action or navigation is an omission and is an
      error under strict, while a refused **easing** — Figma's spring presets
      and its custom bezier — is a warning wherever the switch it animates
      still ships, because that switch lands in one frame and the picture is
      unchanged. Where the switch itself is dropped, the refused easing is part
      of that omission and carries its severity instead — there is no state
      change left for it to degrade. The same applies wherever the switch reaches
      no variant table — a component set that lowers none, an instance whose
      own table was refused, or a layer whose switch the table never carries.
      In each the easing is still reported, as a warning that says the switch
      reached nothing rather than that it lands in one frame, and none of them
      refuses the file. An unlowerable set and an
      unanimated difference each leave a correct picture, so refusing the file
      would withhold something that renders properly. A dropped interaction is
      authored intent going missing, which strict mode exists to catch —
      so a prototype built on hover or timeout triggers will fail a strict
      build rather than import silently.

      **A variant switch on an instance of a component the file does not
      contain is a warning, not a refusal.** That is what every instance of a
      published library component looks like when the export did not include
      the library, and it loses the same thing an unlowerable set loses — the
      variant table — while the instance still paints exactly what the
      designer sees. The switch is what this covers, and not the rest of the
      interaction: a trigger, action or navigation outside the vocabulary is
      refused on such an instance exactly as it is anywhere else, because it
      has no lowering whichever file carries the component.

      **A click authored on a layer inside a component drives the component it
      sits inside**, which is how designers usually build an interactive
      component, and it carries its own Smart Animate timing. So does a
      component nested inside another one whose click changes the outer one's
      state. **One mistake is reported once per layer it was
      written on**, however many copies of that layer are on screen: the design
      tool repeats a component's interactions onto every copy, and a file with
      fifty copies used to report the same problem fifty-one times. It now
      reports it twice — once for each of the two states the layer was drawn in
      — and a report that stands for more than one copy says how many.
- [x] **Design tokens and designer intent** — Figma Variables reach the running
      application both as values and by name, without a plugin. A companion
      plugin lets a designer mark scaffolding that should not ship. Which
      elements are screens is chosen by whoever runs the import, not marked in
      Figma.
- [ ] **Nothing is dropped silently** — every construct the importer _reads_ is
      a named message naming the workaround, and that half holds. **What it does
      not read is a structural gap rather than a short list of exceptions**
      (issue #802): the Figma data model in `rest.rs` does not set
      `deny_unknown_fields`, so any property it was never taught is dropped with
      nothing reported. `docs/figma-support.md`'s "Read nowhere" entries
      enumerate the class against the code — constraints (the largest of them),
      stroke cap and join, layout grids, export settings, paragraph spacing, and
      stroke widths that differ per side (section 3). A character no font in the
      cascade covers is a separate silent case, drawing as an empty box.

      This entry read "**two** known exceptions" until the profile was corrected
      against the lowering. Counting the exceptions was the wrong shape for the
      claim: the parser reports nothing it was not taught, so the list is open
      until it is closed at the parse boundary.
- [x] **Authoring in code** — a Rust interface fills the same model directly,
      with no file in between, and the two routes are proven to produce the same
      result over the vocabulary both express.
- [ ] **Waiving a warning per target** — **part built.** The rule exists and is
      tested: a waiver covers one rule at one place, and a release build should
      refuse a warning nobody waived. Nothing calls it and there is no format
      for writing waivers down, so today a warning blocks nothing.
- [ ] **A supported command that compiles a Figma file** — commands exist and
      work, but they are the repository's own development recipes rather than a
      shipped tool with a stable interface. Making the compiler a product is
      planned (v1) — see section 11.
- [ ] **Authoring from inside a game engine** — planned (v0.21), alongside the
      Unity renderer.

## 9. Rendering backends

Checked against `crates/dashpaint/src/lib.rs`, `crates/dashscene-skia/src/`,
`crates/dashscene-gpu/src/`, `crates/dashscene-unity/src/lib.rs`.

- [x] **One renderer contract** — every renderer consumes the same finished data
      and only colours it in: it never measures text, wraps lines or moves
      anything. That is why two renderers cannot drift apart on layout — they
      are never given the chance to disagree.
- [x] **Reference renderer** — draws the whole feature set on the main
      processor, which makes its output exactly reproducible and therefore
      usable as the reference the others are compared against. It stays
      permanently for that reason.
- [x] **Lean GPU renderer** — draws every shape as a rectangle whose colour is
      computed by a small formula on the graphics card, rather than by tracing
      outlines, which keeps the data moved per frame low. It draws the whole
      feature set, on a computer and in a browser, from one codebase.
- [x] **The two agree** — measured on a developer machine (an Apple M3), not on
      a target device. One set of tolerance bands serves both, and no reference
      picture had to change. How either behaves on the hardware the product
      ships on has not been measured.
- [ ] **Unity renderer** — **part built.** The contract keeping the data
      readable from C# exists and the build enforces it. The renderer, its
      shader library and the C# projection are planned (v0.21), in a separate
      repository that does not exist yet.
- [ ] **Browsers without WebGPU** — WebGPU is the newer browser graphics
      standard the lean renderer needs. A browser lacking it is told so and
      draws nothing. Supporting the older standard is a redesign, and a v1
      question that depends on which browsers the product must reach.

## 10. Platform support

Checked against `demo/`, `demo-web/`, `crates/dashscene-web/`,
`crates/dashscene-desktop/`, `crates/dashscene-unity/src/lib.rs`, and the
absence of any mobile target in the workspace.

- [x] **Desktop** — a windowed host with an event loop, pointer and keyboard
      input, and the showcase running in it. Built and run on macOS and Linux.
      Windows is expected to work, because nothing in the host is
      platform-specific, but it has never been built or tested — there is no
      Windows job in automation and no Windows-specific code.
- [x] **Browsers with WebGPU** — the same showcase on a canvas, with the design
      file fetched in pieces rather than as one download. It still fetches every
      image the file names, one request each, before it draws — see section 6.
- [x] **Embedding in a real application** — **for the browser and the desktop
      only**, which is what the current slice covers. Story #741 made
      `crates/dashscene-web` the web integration crate and story #794 added
      `crates/dashscene-desktop`, so an embedder on either target consumes the
      surface handoff, the frame loop, the generation gate, resize rebuilding
      and the document load rather than copying them out of a demonstration.
      `demo/tests/integration_surface.rs` asserts that for both halves, and
      fails in either direction. What an embedder still writes for itself is
      named in each crate's own module documentation. No mobile target exists,
      and nothing is published: this slice makes the crates publishable and the
      publish is a separate decision (epic #793).
- [x] **A C interface for hosts written in other languages** — **built, and used
      by one host.** `crates/dashscene-ffi` (story #840) exports a version to
      negotiate against, the runtime lifecycle, a document load, a second
      document load carrying the fonts and sheets a `.dsb` cannot (story #947),
      the surface handoff, the tick, resize, a draw call, a surface detach and
      an error channel, with a committed header a C caller compiles against;
      `just c-abi` exercises it as one. No panic crosses the boundary and no
      failure is reachable only as a formatted string. `dashscene-android`
      (story #841) drives it through those entry points as a C caller would,
      which is what established that it was sufficient for layer 0 — and what
      established that it was not quite: `ds_runtime_detach_surface` was added
      there, because the destroy handshake needs a call that drops the surface
      and keeps the document. No iOS or Unity host exists. **Root selection is
      on the mapped load and on no other** (issue #925):
      `ds_runtime_load_document_mapped` takes a path and a required ordinal,
      maps the file, and reads only the assets the named root's subtree draws,
      which is R5 on this path. The two byte-taking loads keep no selection
      deliberately — they use the owning loader, which copies every payload
      whatever is shown, so an ordinal on them would bound nothing while reading
      as though it did. The root is named once, at load; no entry point changes
      it afterwards. **No host calls the mapped load yet**: `dashscene-android`
      still hands over a `byte[]`, which is issue #1035. **A scene built in code
      cannot be expressed through it at all** — there is no builder entry point,
      that being layer 2 (D8), so a host wanting one links the crates directly
      as `demo` and `demo-web` do.
- [ ] **Android and iOS** — **Android part built; iOS nothing.** The
      `aarch64-linux-android` target, an NDK toolchain and a CI job exist (story
      #839), the painter cross-compiles for it, and the C ABI above builds for
      it too. `crates/dashscene-android` (story #841) is the host: an
      `android.view.Surface` reaches the painter, an `AChoreographer` loop on
      its own thread ticks and draws, and `surfaceDestroyed` blocks until that
      loop has stopped and the surface has been dropped — which rotation,
      backgrounding and split-screen have each confirmed.

      **It has run on an emulator and not on a device**, and the distinction is
      the whole of what is unresolved. A compiled `.dsb` draws, rotation and
      backgrounding each run the destroy handshake without a crash, and both
      were observed on the automotive emulator — which is interim evidence and
      is labelled as such, not the D3a measurement. **Split-screen has since
      been exercised, and it did not pass**: the automotive image declares no
      multi-window, freeform or split-screen feature at all, so D4's third case
      was run on 2026-08-14 against a handheld emulator image instead, where
      the harness entered the destroy handshake and never returned (issue
      #960). **On 2026-08-15 that was re-derived and explained**: the render
      thread never returned from its attach, and the cause is the build rather
      than the transition — 0.74 s from cold launch to first frame for a
      release build against over 218 s for a debug one, on the same emulator,
      and `just android` builds debug. With a release library the split-screen
      case passes end to end and the handshake completes in 27 ms.
      `just android-splitscreen` is that run, and it packages what
      `just android` built. **The release library is necessary and not
      sufficient**: the emulator also has to be started with `-gpu host`, or the
      painter obtains no device, the harness draws black and the run fails at
      `assert-drew` (issue #1158). The D3a measurement that
      says the painter's four fragment-stage storage buffers fit a target
      device **has not been taken** — see `docs/design/android-toolchain.md`,
      which records why an emulator cannot take it. Nothing here says Android
      works.

      **The showcase runs on Android** (story #842). `demo-android` is a third
      demonstration host beside `demo` and `demo-web`: `typography` draws MSDF
      Latin and shaped Arabic, `surfaces` draws the full paint vocabulary
      including the backdrop blur, and the scripted pulse animates them. It
      does **not** go through the C ABI — a scene built in code needs an
      `Arena`, and the ABI's lives inside an opaque `DsRuntime` with no builder
      entry point (layer 2, D8) — so it implements `dashscene_android::Frames`
      and shares the render thread, the vsync loop and the destroy handshake
      rather than a second copy of them. Text draws because each scene brings
      its own solver.

      **A loaded document can be given text on this path too, and nothing has
      run it.** Story #863 let a host supply a cascade and its atlases on
      desktop and on the web, and story #947 carried the same thing across the
      C ABI: `ds_runtime_load_document_with_text` takes one descriptor per face
      pairing the font file's bytes with the committed sheet its glyphs sample,
      and `dashscene-android` exposes it as a second JNI entry point,
      `nativeSurfaceCreatedWithText`. **Nothing bakes a sheet at run time** —
      the MSDF generator is an external pinned binary reading its font from a
      path — so a host arrives with a committed PNG and its metrics blob or its
      text is measured and never drawn. **The harness runs it, on an emulator**
      — it stages a committed cascade (Inter at weight 400, its font file and
      the `corpus/atlas/inter-ascii` sheet) beside a text-carrying document and
      calls `nativeSurfaceCreatedWithText`, and the glyphs are drawn. That was
      not true until 2026-08-15: the JNI half had been compiled and never
      executed, which is what issue #969 records. **No device has run it**, so
      the measurement issue #885 owes is unchanged, and #969 stays open for
      that half. A document loaded through `nativeSurfaceCreated` — the
      no-faces call, which is what an embedder supplying no cascade gets —
      still draws no glyphs **and lays its text nodes out as empty leaves**,
      which moves their siblings too.

      **On an emulator, and the frame rate is not a device measurement.** The
      instrument reports in the desktop host's units — one sample read
      `typography over 240 frames — tick 0.19 ms, draw mean 32.89 p50 32.01
      p95 62.46 max 81.85 ms (30.2 fps if unpaced)` — but the only
      painter-capable adapter there is SwiftShader on the CPU, so that
      describes a development machine. Story #842's own deliverable, a
      frame-rate number for a named device, is still owed.

      iOS and the Unity host
      have no target, no toolchain and no automation. The Unity host is v0.21
      and iOS is v1.

## 11. Quality tooling and workflow

Checked against `goldens/`, `.github/workflows/ci.yml`, `justfile`,
`crates/dashscene-typeset/tests/atlas_pipeline.rs`.

- [x] **Reference pictures for every construct** — committed images the build
      compares against, so a change that alters a pixel must be declared rather
      than discovered.
- [x] **Compared against Figma's own render** — seven frames measured against
      real Figma captures, each inside a declared tolerance, plus ten more
      self-authored frames covering what those seven do not reach. This has
      already caught two real bugs on first measurement.
- [x] **Every tolerance ships with the change that breaks it** — the same
      discipline as the asset quality bands, for the same reason.
- [x] **Three test tiers and one gate** — about five seconds between edits,
      thirty-three before pushing, and fifty-four for the run that re-derives
      the asset tables; plus a single check asserting all seven qualification
      criteria on one commit, with its membership pinned by name so a renamed
      test cannot leave the gate silently.
- [x] **Glyph preparation is checked on two processor architectures** — within a
      measured tolerance, not byte for byte: the tool's arithmetic differs
      between architectures by about one step per channel, and the check admits
      that noise and nothing more.
- [ ] **The compiler as a shipped product** — planned (v1): a stable command
      line, versioned diagnostics, a waiver workflow, lint rule packs and
      reporting for design review. Today it is an internal tool.
- [ ] **Anything that catches "it looks wrong on a real automotive driver"** —
      the only check that could is a measurement on the target device, and it is
      not automated.

## 12. Build, delivery and integrity

Checked against `crates/dashbuf/src/container.rs`, `prefix.rs`,
`crates/dashscene-gpu/Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml`.

- [x] **Reproducible builds** — the same design always produces a byte-identical
      file. Everything below depends on this holding.
- [x] **Checked before it is trusted** — the file's version and per-section
      cryptographic hashes are verified before anything in it is read, by code
      that deliberately depends on no parser and takes the writer's word for
      nothing. A field this version does not understand must be zero, or the
      file is refused.
- [x] **Assets authenticated, not just addressed** — the content hash is
      re-computed and compared when the bytes are read, so a substituted payload
      does not resolve.
- [x] **A small trusted surface by design** — image decoders are the classic
      source of vulnerabilities in a system like this, and the lean renderer
      links one rather than three. Note the intended pipeline that converts the
      other formats away cannot be run yet — see section 7.
- [x] **Nothing is fetched while drawing** — the browser host downloads before
      it draws, never during.
- [x] **Pinned dependency versions and a committed lock file.**
- [ ] **Signing** — the format reserves the header fields for a signature and
      refuses any file that uses them today. No signing tool, key handling or
      verification policy exists. The largest gap in this section.
- [ ] **An automated dependency audit** — the audit exists as a local command
      and runs from a developer's push hook. It has never been a
      continuous-integration job, so nothing checks it centrally.
- [ ] **Over-the-air delivery, and remote or streamed screens** — v2. The
      architecture is shaped for streaming and today's interfaces are
      constrained so it does not become a breaking change.

---

## Two things worth stating plainly

**Nothing here is released.** This repository is public, which is not the same
thing: all 21 public package names are reserved on crates.io at `0.1.0`, and
none holds code from this repository. **Nine of the twenty-one were reserved
after development started**, not before it, as the crates that need them arrived
— the most recent two on 2026-08-09. The other twelve come from the family
reserved on 2026-03-18, before this repository's first commit, and they are
exactly the twelve stubs in the archived
[`driftsys/dashscene-name-reservations`](https://github.com/driftsys/dashscene-name-reservations)
([`docs/decisions/crate-name-map.md`](decisions/crate-name-map.md) is the list;
re-derive from it rather than from a count in prose).

**The automated checks run, and a green `ci` still does not mean the suite
ran.** The billing block that stopped every job within seconds having executed
no steps — recorded here from 2026-08-08 — is over: no recent run on `main` has
failed, which `gh run list --branch main --workflow ci` is the derivation for.
Expect the occasional `cancelled` among them rather than an unbroken row of
`success`; the workflow sets `cancel-in-progress`, so two merges close together
leave one behind. What replaces that caveat is a narrower one. The compile and
test jobs are gated on whether the diff touches code, so a documentation-only
change skips them and the aggregate passes anyway, and the tier that re-derives
the committed asset tables sits outside `just build` altogether. A ticked box
here means the tests exist and pass, not that CI ran them on the change you are
looking at (`docs/decisions/test-tiers.md`).

## What this document is not

It is not a schedule. No line carries a date, and the order inside a section is
by topic. [`docs/roadmap.md`](roadmap.md) carries the sequencing.

It is not the evidence. A ticked box here is a summary;
[`docs/specification/05-qualification.md`](specification/05-qualification.md) is
where a claim is proven, and the design records under [`docs/design/`](design/)
describe behaviour and edge cases — with the caveat this file opened with: those
records have themselves drifted from the code, so the code is the authority when
they disagree.
