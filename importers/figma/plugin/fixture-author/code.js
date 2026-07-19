// dashscene fixture author — development plugin, never published.
// Builds one tier-1 corpus fixture (corpus/figma-fixtures/README.md) into the CURRENT
// file. Run the menu command matching the file you have open:
//   blank file "grid-basic"  ->  Plugins > Development > ... > grid-basic
// Re-running a command replaces the previously generated frame, so
// fixtures are regenerable, not hand-built.
//
// Plain JS on purpose: no build step, manifest points straight here.

const INTER = { family: "Inter", style: "Regular" };
const INTER_BOLD = { family: "Inter", style: "Bold" };
// Arabic coverage for the RTL locale variant (§8: rides on lowering files).
const ARABIC = { family: "Noto Sans Arabic", style: "Regular" };
// Noto Sans Regular — the E7 text-oracle fixtures (text-latin, text-arabic)
// author in the fonts the committed corpus atlases are generated from
// (corpus/atlas/ascii from NotoSans-Regular, corpus/atlas/arabic from
// NotoSansArabic-Regular), so the render oracle measures the reference painter
// against Figma's render of the SAME font, not a substitution. Regular only:
// no committed bold atlas, so a bold run would not render faithfully.
const NOTO = { family: "Noto Sans", style: "Regular" };

const GRAY = (v) => ({ r: v, g: v, b: v });
const solid = (color, opacity) => ({
  type: "SOLID",
  color,
  opacity: opacity === undefined ? 1 : opacity,
});

function removePrevious(name) {
  for (const n of figma.currentPage.children) {
    if (n.name === name) n.remove();
  }
}

function baseFrame(name, w, h) {
  removePrevious(name);
  const f = figma.createFrame();
  f.name = name;
  f.resize(w, h);
  f.fills = [solid(GRAY(0.98))];
  figma.currentPage.appendChild(f);
  return f;
}

function label(text, font, size) {
  const t = figma.createText();
  t.fontName = font || INTER;
  t.fontSize = size || 14;
  t.characters = text;
  t.fills = [solid(GRAY(0.1))];
  return t;
}

function cell(name, color) {
  const c = figma.createFrame();
  c.name = name;
  c.fills = [solid(color)];
  c.cornerRadius = 4;
  return c;
}

// Shared by lowering-variant-topology and real-file: the scaffold of one
// variant COMPONENT, named "state=<name>", with auto-layout, uniform
// padding, and a solid fill. Each command appends its own children.
function variantShell(stateName, opts) {
  const comp = figma.createComponent();
  comp.name = "state=" + stateName;
  comp.layoutMode = opts.layoutMode;
  comp.itemSpacing = opts.itemSpacing;
  comp.paddingLeft = comp.paddingRight = opts.paddingX;
  comp.paddingTop = comp.paddingBottom = opts.paddingY;
  comp.fills = [solid(opts.fill)];
  if (opts.cornerRadius !== undefined) comp.cornerRadius = opts.cornerRadius;
  return comp;
}

// ----------------------------------------------------------------- v03-paint
// The v0.3 paint vocabulary — and nothing outside it (§8: a failure must
// bisect to one construct). Covers: solid fill; all four gradient kinds;
// an image fill with a scale mode; the three stroke aligns; uniform and
// per-corner radii; a clipping frame with an overflowing child. The frame
// stays layoutMode NONE, because v0.3 is the paint slice, not the flex
// slice, and it holds no text node, because text is v0.5/v0.6.
//
// Every swatch turns OFF the constructs it does not own — a Figma frame
// clips its content by default, so leaving that default in place would put
// `clipsContent: true` on all twelve cells and stop the clip case from
// bisecting to the one cell that is about to test it.

// A 16x16 RGB PNG checkerboard, 93 bytes, inlined as hex: the plugin sandbox
// has no network (manifest networkAccess "none") and no filesystem, so an
// image fill's bytes must come from the plugin source itself. Hex plus a
// two-line decode rather than base64, so nothing depends on a global the
// sandbox may not expose. figma.createImage takes exactly this Uint8Array and
// returns the Image whose `hash` an IMAGE paint refers to. A checkerboard
// rather than a flat color, so the scale mode is observable in the render.
const CHECKER_PNG_HEX =
  "89504e470d0a1a0a0000000d4948445200000010000000100802000000909168" +
  "36000000244944415478da637816600347721a6e70844b9c61106a204611b2f8" +
  "60d4301a0f83420300d2eaff01d0b08f690000000049454e44ae426082";
const CHECKER_PNG = new Uint8Array(
  CHECKER_PNG_HEX.match(/../g).map((byte) => parseInt(byte, 16)),
);

// Gradient geometry: Figma's plugin API takes a 2x3 gradientTransform (the
// REST capture reports the same geometry as gradientHandlePositions, which
// is what dashbuf's Gradient.handle_* fields mirror). The identity matrix
// runs the gradient left to right across the node's box.
const GRADIENT_TRANSFORM = [[1, 0, 0], [0, 1, 0]];

const stop = (position, color) => ({ position, color });

function gradient(kind, stops) {
  return {
    type: "GRADIENT_" + kind,
    gradientTransform: GRADIENT_TRANSFORM,
    gradientStops: stops,
    visible: true,
    opacity: 1,
    blendMode: "NORMAL",
  };
}

function v03Paint() {
  const root = baseFrame("v03-paint", 960, 680);
  root.layoutMode = "NONE"; // fixed layout: v0.3 is not the flex slice

  const CELL_W = 200;
  const CELL_H = 140;
  const place = (node, col, row) => {
    root.appendChild(node);
    node.x = 32 + col * (CELL_W + 24);
    node.y = 32 + row * (CELL_H + 24);
  };

  // A paint swatch that carries ONE construct: no radius, no stroke, no
  // clip, unless the cell under construction adds it back.
  const swatch = (name) => {
    const s = figma.createFrame();
    s.name = name;
    s.resize(CELL_W, CELL_H);
    s.cornerRadius = 0;
    s.strokes = [];
    s.clipsContent = false;
    return s;
  };

  // --- solid fill
  const solidCell = swatch("fill-solid");
  solidCell.fills = [solid({ r: 0.2, g: 0.5, b: 0.85 })];
  place(solidCell, 0, 0);

  // --- the four gradient kinds, one cell each
  const ramp = [
    stop(0, { r: 1, g: 0.85, b: 0.2, a: 1 }),
    stop(0.5, { r: 0.9, g: 0.3, b: 0.45, a: 1 }),
    stop(1, { r: 0.2, g: 0.25, b: 0.7, a: 1 }),
  ];
  // name -> [column, row] on the 4-column grid of swatches.
  const SLOT = {
    LINEAR: [1, 0],
    RADIAL: [2, 0],
    ANGULAR: [3, 0],
    DIAMOND: [0, 1],
    INSIDE: [2, 1],
    CENTER: [3, 1],
    OUTSIDE: [0, 2],
  };
  for (const kind of ["LINEAR", "RADIAL", "ANGULAR", "DIAMOND"]) {
    const g = swatch("gradient-" + kind.toLowerCase());
    g.fills = [gradient(kind, ramp)];
    place(g, SLOT[kind][0], SLOT[kind][1]);
  }

  // --- image fill with a scale mode. FIT rather than the FILL default, so
  // the captured JSON proves the scale mode round-trips rather than
  // matching whatever a reader would default to.
  const image = figma.createImage(CHECKER_PNG);
  const img = swatch("image-fit");
  img.fills = [{
    type: "IMAGE",
    scaleMode: "FIT",
    imageHash: image.hash,
    visible: true,
    opacity: 1,
    blendMode: "NORMAL",
  }];
  place(img, 1, 1);

  // --- the three stroke aligns, one cell each
  for (const align of ["INSIDE", "CENTER", "OUTSIDE"]) {
    const s = swatch("stroke-" + align.toLowerCase());
    s.fills = [solid(GRAY(0.93))];
    s.strokes = [solid({ r: 0.85, g: 0.25, b: 0.35 })];
    s.strokeWeight = 8;
    s.strokeAlign = align;
    place(s, SLOT[align][0], SLOT[align][1]);
  }

  // --- corner radii: uniform, then per-corner (non-uniform)
  const uniform = swatch("corners-uniform");
  uniform.fills = [solid({ r: 0.45, g: 0.75, b: 0.55 })];
  uniform.cornerRadius = 16;
  place(uniform, 1, 2);

  const perCorner = swatch("corners-per-corner");
  perCorner.fills = [solid({ r: 0.75, g: 0.6, b: 0.9 })];
  perCorner.topLeftRadius = 0;
  perCorner.topRightRadius = 24;
  perCorner.bottomRightRadius = 4;
  perCorner.bottomLeftRadius = 48;
  place(perCorner, 2, 2);

  // --- clipsContent with a child that overflows the frame, so the clip is
  // observable: the child is wider and taller than its parent and starts
  // outside it on both axes.
  const clip = swatch("clip-frame");
  clip.fills = [solid(GRAY(0.88))];
  clip.resize(440, 120);
  clip.clipsContent = true;
  root.appendChild(clip);
  clip.x = 32;
  clip.y = 32 + 3 * (CELL_H + 24);

  const overflow = figma.createFrame();
  overflow.name = "overflow-child";
  overflow.resize(520, 180);
  overflow.cornerRadius = 0;
  overflow.strokes = [];
  overflow.clipsContent = false;
  overflow.fills = [solid({ r: 0.95, g: 0.55, b: 0.2 })];
  clip.appendChild(overflow);
  overflow.x = -60; // left edge outside the parent
  overflow.y = -30; // top edge outside the parent

  return "v03-paint built: solid fill, 4 gradient kinds, image fill (FIT), " +
    "3 stroke aligns, uniform + per-corner radii, clipsContent frame with " +
    "an overflowing child; layoutMode NONE";
}

// ---------------------------------------------------------------- grid-basic
// GRID mode: row/column spans, FIXED + FLEX + HUG tracks, hug/fill
// children, min/max constraints (§8 grid-basic).
async function gridBasic() {
  await figma.loadFontAsync(INTER);
  const grid = baseFrame("grid-basic", 720, 480);
  grid.layoutMode = "GRID";
  // GRID roots do not honor primaryAxisSizingMode/counterAxisSizingMode
  // (those apply only to HORIZONTAL/VERTICAL). Fix the size through the
  // general sizing dropdowns and re-resize, so the FLEX tracks distribute a
  // fixed 720x480 instead of the frame hugging its content.
  grid.layoutSizingHorizontal = "FIXED";
  grid.layoutSizingVertical = "FIXED";
  grid.resize(720, 480);
  grid.gridRowCount = 3;
  grid.gridColumnCount = 3;
  grid.gridItemsPositioning = "MANUAL"; // MANUAL = explicit placement
  grid.gridColumnGap = 12; // GRID mode uses gridColumnGap/gridRowGap,
  grid.gridRowGap = 12; //     not itemSpacing/counterAxisSpacing
  grid.paddingLeft = grid.paddingRight = 16;
  grid.paddingTop = grid.paddingBottom = 16;

  // Track sizing: col 0 fixed, cols 1-2 flex; row 0 fixed, row 1 flex,
  // row 2 hug (sizes to fit the 60px fixed-size cell it holds).
  grid.gridColumnSizes[0].type = "FIXED";
  grid.gridColumnSizes[0].value = 160;
  grid.gridColumnSizes[1].type = "FLEX";
  grid.gridColumnSizes[2].type = "FLEX";
  grid.gridRowSizes[0].type = "FIXED";
  grid.gridRowSizes[0].value = 96;
  grid.gridRowSizes[1].type = "FLEX";
  grid.gridRowSizes[2].type = "HUG";

  // header: spans all 3 columns
  const header = cell("span-3-cols", { r: 0.55, g: 0.65, b: 0.95 });
  grid.appendChild(header);
  header.setGridChildPosition(0, 0);
  header.gridColumnSpan = 3;
  header.layoutSizingHorizontal = "FILL";
  header.layoutSizingVertical = "FILL";

  // sidebar: spans 2 rows in the fixed column
  const sidebar = cell("span-2-rows", { r: 0.6, g: 0.85, b: 0.7 });
  grid.appendChild(sidebar);
  sidebar.setGridChildPosition(1, 0);
  sidebar.gridRowSpan = 2;
  sidebar.layoutSizingHorizontal = "FILL";
  sidebar.layoutSizingVertical = "FILL";

  // fill cell with min/max constraints
  const constrained = cell("fill-minmax", { r: 0.95, g: 0.8, b: 0.55 });
  grid.appendChild(constrained);
  constrained.setGridChildPosition(1, 1);
  constrained.layoutSizingHorizontal = "FILL";
  constrained.layoutSizingVertical = "FILL";
  constrained.minWidth = 120;
  constrained.maxWidth = 400;

  // hug cell: auto-layout wrapping a text node
  const hug = cell("hug-content", { r: 0.9, g: 0.7, b: 0.85 });
  grid.appendChild(hug);
  hug.setGridChildPosition(1, 2);
  hug.layoutMode = "HORIZONTAL";
  hug.paddingLeft = hug.paddingRight = 12;
  hug.paddingTop = hug.paddingBottom = 8;
  hug.appendChild(await label("hug me"));
  hug.layoutSizingHorizontal = "HUG";
  hug.layoutSizingVertical = "HUG";

  // fixed-size cell
  const fixed = cell("fixed-size", { r: 0.75, g: 0.75, b: 0.75 });
  grid.appendChild(fixed);
  fixed.setGridChildPosition(2, 1);
  fixed.resize(140, 60);

  // bottom-right fill cell
  const br = cell("fill-plain", { r: 0.65, g: 0.85, b: 0.9 });
  grid.appendChild(br);
  br.setGridChildPosition(2, 2);
  br.layoutSizingHorizontal = "FILL";
  br.layoutSizingVertical = "FILL";

  return "grid-basic built: 3x3 GRID, fixed+flex+hug tracks, col/row spans, hug/fill/fixed/minmax children";
}

// ----------------------------------------------------------- variables-bound
// boundVariables on color + number props across light/dark modes (§8).
// Designated input for token-resolution phases 1 and 2 (§13).
async function variablesBound() {
  await figma.loadFontAsync(INTER);

  // Recreate the collection from scratch so re-runs stay deterministic.
  const existing = await figma.variables.getLocalVariableCollectionsAsync();
  for (const c of existing) {
    if (c.name === "fixture-tokens") c.remove();
  }
  const col = figma.variables.createVariableCollection("fixture-tokens");
  const light = col.modes[0].modeId;
  col.renameMode(light, "light");
  const dark = col.addMode("dark");

  const mk = (name, type, lightVal, darkVal) => {
    const v = figma.variables.createVariable(name, col, type);
    v.setValueForMode(light, lightVal);
    v.setValueForMode(dark, darkVal);
    return v;
  };
  const vBg = mk("color/bg", "COLOR", { r: 1, g: 1, b: 1, a: 1 }, {
    r: 0.08,
    g: 0.09,
    b: 0.11,
    a: 1,
  });
  const vAccent = mk("color/accent", "COLOR", {
    r: 0.13,
    g: 0.45,
    b: 0.9,
    a: 1,
  }, { r: 0.4, g: 0.65, b: 1, a: 1 });
  const vGap = mk("size/gap", "FLOAT", 16, 24);
  const vRadius = mk("size/radius", "FLOAT", 8, 2);

  const root = baseFrame("variables-bound", 640, 360);
  root.layoutMode = "HORIZONTAL";
  // Fix both axes before adding cards: the cards use layoutSizingHorizontal
  // "FILL", which is invalid on a hugging primary axis.
  root.primaryAxisSizingMode = "FIXED";
  root.counterAxisSizingMode = "FIXED";
  root.paddingLeft = root.paddingRight = 24;
  root.paddingTop = root.paddingBottom = 24;
  root.itemSpacing = 24;

  const makeCard = async (name) => {
    const card = figma.createFrame();
    card.name = name;
    card.layoutMode = "VERTICAL";
    card.paddingLeft = card.paddingRight = 20;
    card.paddingTop = card.paddingBottom = 20;
    // color binding via paint
    card.fills = [
      figma.variables.setBoundVariableForPaint(solid(GRAY(1)), "color", vBg),
    ];
    // number bindings
    card.setBoundVariable("itemSpacing", vGap);
    try {
      card.setBoundVariable("topLeftRadius", vRadius);
      card.setBoundVariable("topRightRadius", vRadius);
      card.setBoundVariable("bottomLeftRadius", vRadius);
      card.setBoundVariable("bottomRightRadius", vRadius);
    } catch (e) {
      console.warn("radius binding failed:", e);
    }
    const chip = figma.createFrame();
    chip.name = "accent-chip";
    chip.resize(120, 32);
    chip.fills = [
      figma.variables.setBoundVariableForPaint(
        solid(GRAY(0.5)),
        "color",
        vAccent,
      ),
    ];
    card.appendChild(chip);
    card.appendChild(await label(name));
    return card;
  };

  const a = await makeCard("card-inherits-mode");
  const b = await makeCard("card-explicit-dark");
  root.appendChild(a);
  root.appendChild(b);
  a.layoutSizingHorizontal = "FILL";
  b.layoutSizingHorizontal = "FILL";
  // One subtree pinned to dark: capture then shows BOTH modes resolved in
  // one file — exactly what the phase-1 sidecar needs to prove itself.
  b.setExplicitVariableModeForCollection(col, dark);

  return "variables-bound built: fixture-tokens collection (light/dark), color+number bindings, one subtree pinned dark";
}

// --------------------------------------------------------------- effects-2025
// REJECT-list diagnostic fixture (§8): noise, texture, progressive blur —
// plus variable-width stroke, which has NO plugin API and stays manual.
// The 2025 effect types are beta in the plugin API; each write is attempted
// independently and failures land on the manual checklist.
//
// Re-run is safe to iterate. Before rebuilding, a previous run's cells are
// mined for two things: the effects already on each cell (so effects a human
// applied through the Effects panel survive), and any child the plugin did
// not create (e.g. a manually drawn variable-width-stroke line). A cell whose
// fresh write fails falls back to its harvested effects; a cell that still
// has nothing goes on the checklist. Foreign children move into the new
// frame. Effect objects read from a node are plain data, so re-applying them
// is a straight assignment.
async function effects2025() {
  await figma.loadFontAsync(INTER);

  // The cells the plugin owns; every other child of the old frame is manual.
  const PLUGIN_CELLS = ["noise", "texture", "progressive-blur"];
  const oldRoot = figma.currentPage.findChild(
    (n) => n.type === "FRAME" && n.name === "effects-2025",
  );
  const harvestedEffects = {}; // cell name -> effects array from the old run
  const foreignChildren = []; // nodes the plugin did not create
  if (oldRoot) {
    for (const child of [...oldRoot.children]) {
      if (PLUGIN_CELLS.includes(child.name)) {
        harvestedEffects[child.name] = child.effects;
      } else {
        foreignChildren.push(child);
      }
    }
    // Detach foreign children before removePrevious deletes the old frame,
    // so the manual work is not destroyed with it.
    for (const child of foreignChildren) {
      figma.currentPage.appendChild(child);
    }
  }

  const root = baseFrame("effects-2025", 640, 300);
  root.layoutMode = "HORIZONTAL";
  // Fix both axes so the frame stays 640x300 instead of hugging the cells.
  root.primaryAxisSizingMode = "FIXED";
  root.counterAxisSizingMode = "FIXED";
  root.itemSpacing = 24;
  root.paddingLeft = root.paddingRight = 24;
  root.paddingTop = root.paddingBottom = 24;

  const manual = [];
  const tryEffect = (name, color, effect) => {
    const r = cell(name, color);
    root.appendChild(r);
    r.resize(160, 160);
    try {
      r.effects = [effect];
      return;
    } catch (e) {
      console.warn(name + " effect write failed:", e);
    }
    // Fresh write failed: fall back to the previous run's effects if any,
    // otherwise ask for the effect to be applied by hand.
    const salvaged = harvestedEffects[name];
    if (salvaged && salvaged.length > 0) {
      r.effects = salvaged;
    } else {
      manual.push(name);
    }
  };

  tryEffect("noise", { r: 0.85, g: 0.6, b: 0.6 }, {
    type: "NOISE",
    blendMode: "NORMAL",
    visible: true,
    noiseSize: 1,
    density: 0.5,
    noiseType: "MONOTONE",
    color: { r: 0, g: 0, b: 0, a: 0.5 },
  });

  tryEffect("texture", { r: 0.6, g: 0.8, b: 0.65 }, {
    type: "TEXTURE",
    visible: true,
    noiseSize: 4,
    radius: 2,
    clipToShape: true,
  });

  // Progressive blur is a LAYER_BLUR with blurType PROGRESSIVE; radius is the
  // end radius, startRadius/startOffset/endOffset drive the gradient.
  tryEffect("progressive-blur", { r: 0.6, g: 0.65, b: 0.9 }, {
    type: "LAYER_BLUR",
    blurType: "PROGRESSIVE",
    visible: true,
    radius: 20,
    startRadius: 0,
    startOffset: { x: 0.5, y: 0 },
    endOffset: { x: 0.5, y: 1 },
  });

  // Variable-width stroke: Figma Draw feature, not scriptable — always manual.
  // Carry any preserved manual node into the new frame; only ask for it again
  // if nothing was carried over from a previous run.
  if (foreignChildren.length > 0) {
    for (const child of foreignChildren) root.appendChild(child);
  } else {
    manual.push(
      "variable-width stroke (draw a line, Draw tools > variable width)",
    );
  }

  // Leave the checklist inside the file; `_` prefix = trimmed by convention.
  removePrevious("_manual-checklist");
  const note = await label(
    "_manual-steps:\n" + manual.map((m) => "  - " + m).join("\n"),
    INTER,
    12,
  );
  note.name = "_manual-checklist";
  figma.currentPage.appendChild(note);
  note.x = root.x;
  note.y = root.y + root.height + 24;

  return "effects-2025 built; manual steps remaining: " + manual.join("; ");
}

// ------------------------------------------------------------- lowering-wrap
async function loweringWrap() {
  await figma.loadFontAsync(INTER);
  const root = baseFrame("lowering-wrap", 420, 300);
  root.layoutMode = "HORIZONTAL";
  root.layoutWrap = "WRAP";
  // Keep width fixed at 420 so children actually wrap instead of the frame
  // hugging them into one row; height hugs the resulting rows.
  root.primaryAxisSizingMode = "FIXED";
  root.counterAxisSizingMode = "AUTO";
  root.itemSpacing = 12;
  root.counterAxisSpacing = 16;
  root.paddingLeft = root.paddingRight = 16;
  root.paddingTop = root.paddingBottom = 16;
  const widths = [120, 80, 160, 100, 140, 90, 110];
  for (let i = 0; i < widths.length; i++) {
    const chip = cell("chip-" + (i + 1), {
      r: 0.55 + (i % 3) * 0.12,
      g: 0.65,
      b: 0.9 - (i % 4) * 0.1,
    });
    root.appendChild(chip);
    chip.resize(widths[i], 40);
  }
  return "lowering-wrap built: 7 fixed-width chips wrapping in a 420px row";
}

// ------------------------------------------------------ lowering-hug-in-fill
async function loweringHugInFill() {
  await figma.loadFontAsync(INTER);
  const root = baseFrame("lowering-hug-in-fill", 480, 200);
  root.layoutMode = "VERTICAL";
  // Fix both axes before adding the fill-container: FILL needs a fixed-width
  // (counter-axis) parent to fill into.
  root.primaryAxisSizingMode = "FIXED";
  root.counterAxisSizingMode = "FIXED";
  root.paddingLeft = root.paddingRight = 16;
  root.paddingTop = root.paddingBottom = 16;
  root.itemSpacing = 12;

  const fill = cell("fill-container", { r: 0.85, g: 0.88, b: 0.95 });
  root.appendChild(fill);
  fill.layoutMode = "HORIZONTAL";
  fill.paddingLeft = fill.paddingRight = 12;
  fill.paddingTop = fill.paddingBottom = 12;
  fill.layoutSizingHorizontal = "FILL"; // fill parent...
  fill.layoutSizingVertical = "HUG";

  const hug = cell("hug-inside", { r: 0.95, g: 0.75, b: 0.6 });
  fill.appendChild(hug);
  hug.layoutMode = "HORIZONTAL";
  hug.paddingLeft = hug.paddingRight = 10;
  hug.paddingTop = hug.paddingBottom = 6;
  hug.appendChild(await label("hug inside fill"));
  hug.layoutSizingHorizontal = "HUG"; // ...containing a hug child
  hug.layoutSizingVertical = "HUG";

  return "lowering-hug-in-fill built: HUG child inside FILL container";
}

// ----------------------------------------------------- lowering-negative-gap
function loweringNegativeGap() {
  const root = baseFrame("lowering-negative-gap", 360, 120);
  root.layoutMode = "HORIZONTAL";
  // Fix both axes so the frame stays 360x120 instead of hugging the dots.
  root.primaryAxisSizingMode = "FIXED";
  root.counterAxisSizingMode = "FIXED";
  root.itemSpacing = -16; // the construct under test (lowers to margins)
  root.paddingLeft = root.paddingRight = 24;
  root.paddingTop = root.paddingBottom = 24;
  for (let i = 0; i < 5; i++) {
    const dot = figma.createEllipse();
    dot.name = "overlap-" + (i + 1);
    dot.resize(56, 56);
    dot.fills = [solid({ r: 0.3 + i * 0.15, g: 0.5, b: 0.9 - i * 0.15 }, 0.9)];
    root.appendChild(dot);
  }
  return "lowering-negative-gap built: itemSpacing -16 overlap row";
}

// --------------------------------------------------------- lowering-baseline
// Mixed-size baseline row (docs/technotes/open-questions.md, Q-4: Taffy's least-exercised corner) +
// the RTL/Arabic locale variant with Arabic-Indic numerals (corpus/figma-fixtures/README.md, E2).
async function loweringBaseline() {
  await figma.loadFontAsync(INTER);
  await figma.loadFontAsync(INTER_BOLD);
  let arabicFont = null;
  try {
    await figma.loadFontAsync(ARABIC);
    arabicFont = ARABIC;
  } catch (e) {
    console.warn("Noto Sans Arabic unavailable, Arabic run skipped:", e);
  }

  const root = baseFrame("lowering-baseline", 640, 160);
  root.layoutMode = "HORIZONTAL";
  // Fix both axes so the frame stays 640x160 instead of hugging the row.
  root.primaryAxisSizingMode = "FIXED";
  root.counterAxisSizingMode = "FIXED";
  root.counterAxisAlignItems = "BASELINE"; // the construct under test
  root.itemSpacing = 16;
  root.paddingLeft = root.paddingRight = 24;
  root.paddingTop = root.paddingBottom = 24;

  root.appendChild(await label("small", INTER, 12));
  root.appendChild(await label("MEDIUM", INTER_BOLD, 24));
  root.appendChild(await label("Large", INTER, 40));
  const boxed = cell("boxed-text", { r: 0.9, g: 0.9, b: 0.7 });
  root.appendChild(boxed);
  boxed.layoutMode = "HORIZONTAL";
  boxed.paddingLeft = boxed.paddingRight = 8;
  boxed.paddingTop = boxed.paddingBottom = 4;
  boxed.appendChild(await label("boxed 18", INTER, 18));

  if (arabicFont) {
    // Arabic + Arabic-Indic numerals: shaping, RTL, mixed numerals in one run.
    const ar = await label("السرعة ١٢٠ كم/س", arabicFont, 24);
    ar.name = "arabic-rtl";
    root.appendChild(ar);
  }
  return "lowering-baseline built: mixed-size baseline row" +
    (arabicFont
      ? " incl. Arabic RTL run"
      : " (Arabic font unavailable — add manually)");
}

// ------------------------------------------------- lowering-variant-topology
// A component set whose variants have DIFFERENT child counts — the variant
// topology change case (E3): switching variants adds/removes nodes.
async function loweringVariantTopology() {
  await figma.loadFontAsync(INTER);
  removePrevious("lowering-variant-topology");
  removePrevious("instance-collapsed"); // the instance is a separate page child

  const mkVariant = async (stateName, childCount) => {
    const comp = variantShell(stateName, {
      layoutMode: "VERTICAL",
      itemSpacing: 8,
      paddingX: 16,
      paddingY: 16,
      fill: GRAY(0.96),
    });
    comp.appendChild(await label("state: " + stateName, INTER_BOLD, 14));
    for (let i = 0; i < childCount; i++) {
      const row = cell("row-" + (i + 1), { r: 0.7, g: 0.75, b: 0.9 });
      comp.appendChild(row);
      row.resize(180, 28);
    }
    return comp;
  };
  await figma.loadFontAsync(INTER_BOLD);

  const a = await mkVariant("collapsed", 1);
  const b = await mkVariant("expanded", 4);
  const set = figma.combineAsVariants([a, b], figma.currentPage);
  set.name = "lowering-variant-topology";

  // An instance of the default variant so the exported tree exercises
  // instance-of-variant, not just the definitions.
  const inst = a.createInstance();
  inst.name = "instance-collapsed";
  figma.currentPage.appendChild(inst);
  inst.x = set.x;
  inst.y = set.y + set.height + 40;

  return "lowering-variant-topology built: 2 variants with different child counts + 1 instance";
}

// -------------------------------------------------------------------- real-file
// The v0.7 real-file import spike (story #37): production-shaped, not a
// single-construct fixture. It deliberately carries the shapes the export
// closure (importers/figma/src/closure.ts) must prove itself against:
// two pages; several top-level frames on page 1, of which an export
// manifest declares only "home"; a component set on the second page with an
// instance inside the declared root (per-set variant closure); a hidden
// layer (hidden != trimmed: it exports as visible:false, DESIGN §6.1); and
// an image fill, so the closure's ref scan meets a real capture.
async function realFile() {
  await figma.loadFontAsync(INTER);
  await figma.loadFontAsync(INTER_BOLD);

  // Page 2 first: the component definitions. Re-running clears the page
  // rather than deleting it — the current page cannot be removed.
  let defs = figma.root.children.find((p) => p.name === "real-file-components");
  if (!defs) {
    defs = figma.createPage();
    defs.name = "real-file-components";
  }
  for (const n of [...defs.children]) n.remove();

  const mkVariant = async (stateName, fillValue) => {
    const comp = variantShell(stateName, {
      layoutMode: "HORIZONTAL",
      itemSpacing: 8,
      paddingX: 12,
      paddingY: 6,
      fill: GRAY(fillValue),
      cornerRadius: 12,
    });
    comp.appendChild(await label("chip " + stateName, INTER, 12));
    return comp;
  };
  const on = await mkVariant("on", 0.85);
  const off = await mkVariant("off", 0.95);
  const set = figma.combineAsVariants([on, off], defs);
  set.name = "real-file-chip";

  // Page 1: the screens. "home" is the frame an export manifest declares;
  // "scratch" and the hidden "wip-banner" stay undeclared, so a capture
  // proves declared-root exclusion against a real response.
  const home = baseFrame("home", 420, 640);
  home.layoutMode = "VERTICAL";
  home.itemSpacing = 16;
  home.paddingLeft = home.paddingRight = 24;
  home.paddingTop = home.paddingBottom = 24;
  home.appendChild(await label("home", INTER_BOLD, 24));

  const hero = cell("hero", GRAY(1));
  const image = figma.createImage(CHECKER_PNG);
  hero.fills = [{ type: "IMAGE", scaleMode: "FILL", imageHash: image.hash }];
  home.appendChild(hero);
  hero.resize(372, 160);

  const chip = on.createInstance();
  chip.name = "chip-instance";
  home.appendChild(chip);

  const hidden = cell("wip-banner", { r: 0.95, g: 0.8, b: 0.8 });
  home.appendChild(hidden);
  hidden.resize(372, 40);
  hidden.visible = false;

  const scratch = baseFrame("scratch", 300, 200);
  scratch.x = home.x + home.width + 80;
  scratch.appendChild(await label("not part of any export", INTER, 14));

  return "real-file built: home (image fill, instance, hidden layer) + " +
    "scratch on page 1, chip variants on real-file-components";
}

// --------------------------------------------------------------------- trim-demo
// The trim-path exercise (story #39): one declared root frame holding a node
// for each trim case, so a capture replays annotate -> trim -> named record
// against a real response. This command builds the SCENE only; the roles are
// written by the SEPARATE annotator plugin (importers/figma/plugin/), which is
// the only tool that writes sharedPluginData roles
// (docs/decisions/annotator-plugin-contract-frozen.md). After building, follow
// the annotate step in this folder's README, then capture.
//
// Cases: real content (kept); a placeholder slot whose sample children are
// auto-replaced; a redline overlay; a spec note; a `_`-prefixed scratch layer
// (trimmed by name alone, no annotation); and a hidden layer (visible:false is
// NOT trimmed — it may be a variant state).
async function trimDemo() {
  await figma.loadFontAsync(INTER);
  await figma.loadFontAsync(INTER_BOLD);

  const root = baseFrame("trim-demo", 420, 640);
  root.layoutMode = "VERTICAL";
  root.itemSpacing = 16;
  root.paddingLeft = root.paddingRight = 24;
  root.paddingTop = root.paddingBottom = 24;
  root.appendChild(await label("trim-demo", INTER_BOLD, 24));

  // Real content — kept.
  const content = cell("real-content", { r: 0.85, g: 0.9, b: 0.95 });
  root.appendChild(content);
  content.resize(372, 80);

  // A slot: annotate this frame `placeholder`. Its children are sample content
  // by definition and are auto-replaced (trimmed) at import.
  const slot = cell("slot", { r: 0.9, g: 0.92, b: 0.85 });
  root.appendChild(slot);
  slot.layoutMode = "VERTICAL";
  slot.paddingLeft = slot.paddingRight = 12;
  slot.paddingTop = slot.paddingBottom = 12;
  slot.itemSpacing = 8;
  slot.appendChild(await label("sample-a", INTER, 12));
  slot.appendChild(await label("sample-b", INTER, 12));

  // A redline overlay: annotate `redline`.
  const redline = cell("redline-overlay", { r: 0.95, g: 0.6, b: 0.6 });
  root.appendChild(redline);
  redline.resize(372, 40);

  // A spec note: annotate `spec`.
  const spec = await label("spec: gap = 16, radius = 8", INTER, 12);
  spec.name = "spec-note";
  root.appendChild(spec);

  // `_`-prefixed scratch — trimmed by name alone, no annotation needed.
  const scratch = cell("_scratch", { r: 0.8, g: 0.8, b: 0.8 });
  root.appendChild(scratch);
  scratch.resize(372, 32);

  // Hidden layer — visible:false is NOT trimmed (it may be a variant state).
  const hidden = cell("hidden-state", { r: 0.8, g: 0.75, b: 0.95 });
  root.appendChild(hidden);
  hidden.resize(372, 32);
  hidden.visible = false;

  return "trim-demo built: real content, a placeholder slot (2 sample kids), " +
    "a redline overlay, a spec note, a _scratch layer, and a hidden layer. " +
    "Now annotate the roles with the dashscene annotator (see README), then " +
    "capture.";
}

// -------------------------------------------- text-latin (E7 render oracle)
// A Latin text scene authored in Noto Sans Regular — the font the committed
// ascii atlas (corpus/atlas/ascii) is generated from — so the E7 render oracle
// (goldens/oracle, frame v05-text-latin, msdf-text band) measures the reference
// painter's MSDF glyphs against Figma's own render of the SAME font at the same
// size, not a substitution. The frame is FIXED on both axes so our render and
// Figma's GET /images export are identical in size regardless of glyph metrics
// (the v08-baseline lesson: a substituted font resized the HUG root and the
// mismatch could not be diffed). The text nodes hug inside the fixed frame.
// Strings are printable ASCII, fully covered by the ascii atlas (0x20..0x7e).
async function textLatin() {
  await figma.loadFontAsync(NOTO);
  const root = baseFrame("text-latin", 480, 200);
  root.layoutMode = "VERTICAL";
  root.primaryAxisSizingMode = "FIXED"; // fixed height ...
  root.counterAxisSizingMode = "FIXED"; // ... and width: an identical box
  root.resize(480, 200); // re-fix after the sizing modes (the gridBasic pattern:
  // setting layoutMode collapses an empty frame to its padding, and FIXED would
  // otherwise lock that collapsed size)
  root.itemSpacing = 16;
  root.paddingLeft = root.paddingRight = 24;
  root.paddingTop = root.paddingBottom = 24;
  root.appendChild(label("Hello dashscene", NOTO, 28));
  root.appendChild(label("88 mph", NOTO, 44));
  return "text-latin built: Noto Sans 'Hello dashscene' (28) + '88 mph' (44) in a fixed 480x200 frame";
}

// ------------------------------------------- text-arabic (E7 render oracle)
// An Arabic RTL text scene in Noto Sans Arabic Regular — the font the committed
// arabic atlas (corpus/atlas/arabic) is generated from — for the E7 render
// oracle (frame v06-text-arabic, msdf-text band). It exercises RTL shaping, a
// harakat (diacritic) word, and Arabic-Indic numerals written EXPLICITLY
// (٠..٩, not European digits): the shaper renders European digits as
// Arabic-Indic in Arabic context, which would diverge from Figma's render, so
// authoring the Arabic-Indic codepoints keeps both sides on the same glyphs.
// Every glyph is in the arabic atlas (Arabic letters + harakat + Arabic-Indic
// digits + space). FIXED frame box for identical dimensions, as text-latin.
async function textArabic() {
  let haveArabic = false;
  try {
    await figma.loadFontAsync(ARABIC);
    haveArabic = true;
  } catch (e) {
    console.warn("Noto Sans Arabic unavailable:", e);
    await figma.loadFontAsync(INTER); // for the manual-steps note
  }

  const root = baseFrame("text-arabic", 520, 240);
  root.layoutMode = "VERTICAL";
  root.primaryAxisSizingMode = "FIXED";
  root.counterAxisSizingMode = "FIXED";
  root.resize(520, 240); // re-fix after the sizing modes (see textLatin)
  root.counterAxisAlignItems = "MAX"; // right-align the RTL runs
  root.itemSpacing = 16;
  root.paddingLeft = root.paddingRight = 24;
  root.paddingTop = root.paddingBottom = 24;

  if (!haveArabic) {
    // Same fallback shape as effects-2025: leave a `_`-prefixed checklist so
    // the missing runs are authored by hand in Noto Sans Arabic Regular.
    const note = label(
      "_manual-steps: Noto Sans Arabic unavailable. Add three text nodes in " +
        "Noto Sans Arabic Regular:\n  السلام عليكم (28)\n  مَرْحَبًا (32)\n" +
        "  سرعة ١٢٠ (36)",
      INTER,
      12,
    );
    note.name = "_manual-checklist";
    root.appendChild(note);
    return "text-arabic: Noto Sans Arabic unavailable — add the three Arabic runs manually (see _manual-checklist)";
  }

  const banner = label("السلام عليكم", ARABIC, 28);
  banner.name = "banner";
  root.appendChild(banner);
  const harakat = label("مَرْحَبًا", ARABIC, 32);
  harakat.name = "harakat";
  root.appendChild(harakat);
  const speed = label("سرعة ١٢٠", ARABIC, 36);
  speed.name = "speed";
  root.appendChild(speed);
  return "text-arabic built: Noto Sans Arabic banner + harakat word + Arabic-Indic numeral readout in a fixed 520x240 frame";
}

// ----------------------------------------- text-baseline (E7 render oracle)
// A mixed-size Latin row aligned on the BASELINE — the E7 render oracle frame
// v08-baseline, authored in Noto Sans Regular (the committed ascii atlas font)
// so the oracle measures the reference painter against Figma's render of the
// SAME font, not a substitution. It is authored to replace the earlier
// lowering-baseline fixture as the v08-baseline oracle frame once captured and
// wired (a later step; goldens/oracle/manifest.json still maps v08-baseline to
// lowering-baseline until then): lowering-baseline authors its Latin leaves in
// Inter (uncommitted), so rendered in Noto Sans its HUG root measured 621x160
// against Figma's 608x160 and could not be diffed. A HORIZONTAL row with
// counterAxisAlignItems BASELINE (lowers to CrossAxisAlign::Baseline since #264)
// and three Regular runs at 12/24/40 — no committed bold atlas, so Regular only.
// Unlike the stacked v05-text-latin/v06-text-arabic frames, this frame exercises
// baseline alignment of mixed-size runs: the engine aligns a leaf on the box
// bottom, not the glyph baseline (debt #272), so the oracle measures that
// alignment fidelity against Figma. FIXED frame box so our render and Figma's
// GET /images export are the same size regardless of glyph metrics.
async function textBaseline() {
  await figma.loadFontAsync(NOTO);
  const root = baseFrame("text-baseline", 380, 120);
  root.layoutMode = "HORIZONTAL";
  root.primaryAxisSizingMode = "FIXED"; // fixed width ...
  root.counterAxisSizingMode = "FIXED"; // ... and height: an identical box
  root.resize(380, 120); // re-fix after the sizing modes (see textLatin)
  root.counterAxisAlignItems = "BASELINE"; // the construct under test
  root.itemSpacing = 16;
  root.paddingLeft = root.paddingRight = 24;
  root.paddingTop = root.paddingBottom = 24;
  root.appendChild(label("small", NOTO, 12));
  root.appendChild(label("medium", NOTO, 24));
  root.appendChild(label("LARGE", NOTO, 40));
  return "text-baseline built: Noto Sans mixed-size BASELINE row (small 12, medium 24, LARGE 40) in a fixed 380x120 frame";
}

// ------------------------------------ drop-shadow / inner-shadow (E7 oracle)
// Two single-construct shadow scenes (§8: a failure must bisect to one
// construct), one per E7 oracle frame (v08-drop-shadow / v08-inner-shadow,
// blur-falloff band). Each is one card carrying exactly one shadow that LOWERS
// CLEAN under Profile::Core: a DROP_SHADOW / INNER_SHADOW with a present color,
// NORMAL blend, and a finite non-negative blur is on dashc's accept path
// (crates/dashc/src/figma/{triage.rs,mod.rs}). The parameters are the
// proven-rendered values from the v08_shadows golden (offset, blur 6, black
// alpha 0.55): the oracle pins the sigma = blur/2 mapping (G-1,
// docs/decisions/effects-vocabulary-shadows.md) against Figma's own render.
//
// The card sits centered in a 96x96 light frame with a wide margin so the
// soft falloff stays inside the frame (no clip divergence), and on a light
// background so a dark shadow's falloff is observable. Fixed frame box, so our
// render and Figma's GET /images export are the same size.
const SHADOW_INK = { r: 0, g: 0, b: 0, a: 0.55 };
const DROP_SHADOW = {
  type: "DROP_SHADOW",
  visible: true,
  blendMode: "NORMAL",
  color: SHADOW_INK,
  offset: { x: 0, y: 4 },
  radius: 6, // Figma "radius" == dashc blur; painter sigma = radius/2
  spread: 0,
  showShadowBehindNode: false, // required by the plugin API; dashc ignores it
};
const INNER_SHADOW = {
  type: "INNER_SHADOW",
  visible: true,
  blendMode: "NORMAL",
  color: SHADOW_INK,
  offset: { x: 0, y: 0 },
  radius: 6,
  spread: 0,
};

function shadowScene(name, cardFill, effect) {
  const root = baseFrame(name, 96, 96);
  root.layoutMode = "NONE"; // the card is absolutely placed, not laid out
  root.clipsContent = true;
  const card = figma.createFrame();
  card.name = "card";
  card.resize(40, 40);
  card.cornerRadius = 6;
  card.strokes = [];
  card.clipsContent = false;
  card.fills = [solid(cardFill)];
  card.effects = [effect];
  root.appendChild(card);
  card.x = 28; // centered: (96 - 40) / 2
  card.y = 28;
  return root;
}

function dropShadow() {
  shadowScene("drop-shadow", { r: 0.98, g: 0.78, b: 0.2 }, DROP_SHADOW);
  return "drop-shadow built: a 40x40 amber card (r=6) with a drop shadow " +
    "(offset y=4, blur 6, black alpha 0.55) centered in a 96x96 light frame";
}

function innerShadow() {
  shadowScene("inner-shadow", { r: 0.92, g: 0.94, b: 0.98 }, INNER_SHADOW);
  return "inner-shadow built: a 40x40 near-white card (r=6) with an inner " +
    "shadow (offset 0, blur 6, black alpha 0.55) centered in a 96x96 light frame";
}

// ------------------------------------------------------ liga-text (v0.10 A0)
// The standard-ligatures test vector (epic #343, story A0): a ligature-rich
// ASCII run ("waffle" -> ffl, "office" -> ffi) with standard ligatures turned
// OFF is what serializes OpenType LIGA:0, next to the same run at DEFAULT
// settings (ligatures on) for contrast. Noto Sans Regular, the committed
// ascii-atlas font (see text-latin above).
//
// The plugin API has NO writable ligature/OpenType-feature toggle:
// `openTypeFeatures` is `readonly` on TextNode and `getRangeOpenTypeFeatures`
// only reads a range's features back — @figma/plugin-typings defines no
// `setRangeOpenTypeFeatures` or equivalent setter. So this command builds
// both runs at DEFAULT and leaves a `_manual-checklist` (the effects-2025
// pattern) asking for the first run's ligatures to be disabled by hand.
async function ligaText() {
  await figma.loadFontAsync(NOTO);
  await figma.loadFontAsync(INTER); // for the manual-steps note

  const root = baseFrame("liga-text", 420, 200);
  root.layoutMode = "VERTICAL";
  root.primaryAxisSizingMode = "FIXED"; // fixed width ...
  root.counterAxisSizingMode = "FIXED"; // ... and height: an identical box
  root.resize(420, 200); // re-fix after the sizing modes (see textLatin)
  root.itemSpacing = 16;
  root.paddingLeft = root.paddingRight = 24;
  root.paddingTop = root.paddingBottom = 24;

  const ligaOff = label("waffle finish office", NOTO, 28);
  ligaOff.name = "liga-off";
  root.appendChild(ligaOff);

  const ligaOn = label("waffle finish office", NOTO, 28);
  ligaOn.name = "liga-on";
  root.appendChild(ligaOn);

  const manual = [
    "select 'liga-off' and disable standard ligatures (Type settings > " +
    "Details panel > Ligatures) — no plugin API can write this",
  ];
  removePrevious("_manual-checklist");
  const note = label(
    "_manual-steps:\n" + manual.map((m) => "  - " + m).join("\n"),
    INTER,
    12,
  );
  note.name = "_manual-checklist";
  figma.currentPage.appendChild(note);
  note.x = root.x;
  note.y = root.y + root.height + 24;

  return "liga-text built: 'waffle finish office' x2 in Noto Sans (liga-off, " +
    "liga-on); MANUAL STEP required — select liga-off and disable ligatures " +
    "via Type settings > Details panel (no writable plugin API for OpenType " +
    "features)";
}

// ------------------------------------------------------ jpeg-fill (v0.10 A0)
// The JPEG-format image-fill test vector (epic #343, story A0): the paint's
// imageRef bytes must actually decode as a JPEG (magic FF D8 FF), not a PNG
// or GIF relabeled — v03Paint's CHECKER_PNG proves PNG, gif-fill below proves
// GIF, this proves JPEG. A 16x16 solid-color baseline JPEG, generated once
// with ImageMagick (`magick -size 16x16 xc:'#3399cc' -quality 90 tiny.jpg`)
// and inlined as hex for the same sandbox-safety reason as CHECKER_PNG_HEX
// above (no network, no filesystem — figma.createImage takes exactly this
// Uint8Array).
const JPEG_FILL_HEX =
  "ffd8ffe000104a46494600010100000100010000ffdb004300030202030202030303" +
  "0304030304050805050404050a070706080c0a0c0c0b0a0b0b0d0e12100d0e110e0b" +
  "0b1016101113141515150c0f171816141812141514ffdb0043010304040504050905" +
  "0509140d0b0d14141414141414141414141414141414141414141414141414141414" +
  "14141414141414141414141414141414141414141414ffc000110800100010030111" +
  "00021101031101ffc40014000100000000000000000000000000000000ffc4001410" +
  "0100000000000000000000000000000000ffc4001601010101000000000000000000" +
  "00000000000708ffc40014110100000000000000000000000000000000ffda000c03" +
  "010002110311003f002e6cd800003fffd9";
const JPEG_FILL_BYTES = new Uint8Array(
  JPEG_FILL_HEX.match(/../g).map((byte) => parseInt(byte, 16)),
);

function jpegFill() {
  const root = baseFrame("jpeg-fill", 160, 160);
  root.layoutMode = "NONE"; // fixed layout: the fill is the only construct
  const image = figma.createImage(JPEG_FILL_BYTES);
  root.fills = [{
    type: "IMAGE",
    scaleMode: "FILL",
    imageHash: image.hash,
    visible: true,
    opacity: 1,
    blendMode: "NORMAL",
  }];
  return "jpeg-fill built: root frame filled with a 16x16 opaque baseline " +
    "JPEG image fill (magic FF D8 FF, " + JPEG_FILL_BYTES.length + " bytes)";
}

// ------------------------------------------------------- gif-fill (v0.10 A0)
// Same shape as jpeg-fill, but the embedded bytes are a real static GIF
// (magic 47 49 46 = "GIF"), generated with the same tool
// (`magick -size 16x16 xc:'#33cc66' tiny.gif`).
const GIF_FILL_HEX =
  "47494638396110001000f0000033cc6600000021f90400000000002c000000001000" +
  "100000020e848fa9cbed0fa39cb4da8bb33e05003b";
const GIF_FILL_BYTES = new Uint8Array(
  GIF_FILL_HEX.match(/../g).map((byte) => parseInt(byte, 16)),
);

function gifFill() {
  const root = baseFrame("gif-fill", 160, 160);
  root.layoutMode = "NONE"; // fixed layout: the fill is the only construct
  const image = figma.createImage(GIF_FILL_BYTES);
  root.fills = [{
    type: "IMAGE",
    scaleMode: "FILL",
    imageHash: image.hash,
    visible: true,
    opacity: 1,
    blendMode: "NORMAL",
  }];
  return "gif-fill built: root frame filled with a 16x16 opaque static GIF " +
    "image fill (magic GIF89a, " + GIF_FILL_BYTES.length + " bytes)";
}

// -------------------------------------------------- vector-shapes (v0.10 A0)
// Four VECTOR nodes built with figma.createVector() + explicit .vectorPaths
// (epic #343, story A0): a 5-point star (many straight segments), an arrow
// (a single closed polygon), a curved organic path (cubic-bezier `C`
// commands), and a shape with a hole (an outer + inner subpath in ONE
// vectorPaths entry, windingRule "EVENODD" — the fill-rule case: a point
// under an odd number of subpaths is filled, under an even number is not, so
// the inner subpath punches a hole regardless of its winding direction).
function starPathData(cx, cy, points, outerR, innerR) {
  const cmds = [];
  for (let i = 0; i < points * 2; i++) {
    const r = i % 2 === 0 ? outerR : innerR;
    const angle = -Math.PI / 2 + (i * Math.PI) / points;
    const x = cx + r * Math.cos(angle);
    const y = cy + r * Math.sin(angle);
    cmds.push((i === 0 ? "M " : "L ") + x.toFixed(2) + " " + y.toFixed(2));
  }
  cmds.push("Z");
  return cmds.join(" ");
}

function vectorShapes() {
  const root = baseFrame("vector-shapes", 460, 160);
  root.layoutMode = "NONE";

  const CELL = 80;
  const GAP = 24;
  const MARGIN = 24;
  const slotX = (i) => MARGIN + i * (CELL + GAP);

  // figma.createVector() gives a new vector a default 1px black stroke; these
  // fixtures are filled shapes, so clear it (`strokes = []`) or the coloured
  // fill plus the black stroke lowers as a differently-coloured fill+stroke,
  // which dashc refuses (a v0.11-deferred case) — the same clear the other
  // shape commands make.
  const star = figma.createVector();
  star.name = "star-5-point";
  star.vectorPaths = [{
    windingRule: "NONZERO",
    data: starPathData(40, 40, 5, 38, 15),
  }];
  star.fills = [solid({ r: 0.95, g: 0.75, b: 0.2 })];
  star.strokes = [];
  root.appendChild(star);
  star.x = slotX(0);
  star.y = 24;

  const arrow = figma.createVector();
  arrow.name = "arrow";
  arrow.vectorPaths = [{
    windingRule: "NONZERO",
    data: "M 0 20 L 50 20 L 50 5 L 80 30 L 50 55 L 50 40 L 0 40 Z",
  }];
  arrow.fills = [solid({ r: 0.35, g: 0.6, b: 0.9 })];
  arrow.strokes = [];
  root.appendChild(arrow);
  arrow.x = slotX(1);
  arrow.y = 40;

  const blob = figma.createVector();
  blob.name = "organic-blob";
  blob.vectorPaths = [{
    windingRule: "NONZERO",
    data: "M 40 5 C 65 5 75 30 70 50 C 65 72 45 78 25 68 " +
      "C 8 60 5 35 15 18 C 20 8 30 5 40 5 Z",
  }];
  blob.fills = [solid({ r: 0.55, g: 0.8, b: 0.55 })];
  blob.strokes = [];
  root.appendChild(blob);
  blob.x = slotX(2);
  blob.y = 24;

  const withHole = figma.createVector();
  withHole.name = "square-with-hole";
  withHole.vectorPaths = [{
    windingRule: "EVENODD",
    data: "M 0 0 L 80 0 L 80 80 L 0 80 Z M 20 20 L 60 20 L 60 60 L 20 60 Z",
  }];
  withHole.fills = [solid({ r: 0.85, g: 0.4, b: 0.55 })];
  withHole.strokes = [];
  root.appendChild(withHole);
  withHole.x = slotX(3);
  withHole.y = 24;

  return "vector-shapes built: 5-point star, arrow, cubic-bezier organic " +
    "blob, and a square-with-hole (EVENODD) in a row";
}

// -------------------------------------------------- stacked-fills (v0.10 A0)
// The multiple-visible-fills test vector (epic #343, story A0): one
// RECTANGLE with TWO paints in `fills` — a solid at fills[0] (bottom, Figma
// paints fills in array order, later entries on top) and a semi-transparent
// GRADIENT_LINEAR at fills[1] (top, opacity < 1 so the solid underneath
// stays visible through it) — so both are visible and stacked, not just the
// topmost paint.
function stackedFills() {
  const root = baseFrame("stacked-fills", 200, 200);
  root.layoutMode = "NONE";

  const rect = figma.createRectangle();
  rect.name = "stacked-fills-rect";
  rect.resize(140, 140);

  const bottom = solid({ r: 0.25, g: 0.45, b: 0.85 });
  const top = gradient("LINEAR", [
    stop(0, { r: 1, g: 1, b: 1, a: 1 }),
    stop(1, { r: 1, g: 0.3, b: 0.5, a: 1 }),
  ]);
  top.opacity = 0.55; // semi-transparent: the solid below stays visible
  rect.fills = [bottom, top];

  root.appendChild(rect);
  rect.x = 30;
  rect.y = 30;

  return "stacked-fills built: one RECTANGLE with two visible fills — a " +
    "solid at fills[0] (bottom) and a semi-transparent GRADIENT_LINEAR at " +
    "fills[1] (opacity 0.55, top)";
}

// ------------------------------------------------------- node-fx (v0.10 A0)
// Four node-effects lowering paths in one frame (epic #343, story A0): a
// rotated rectangle, a partially-opaque node, a hidden layer (visible:false
// is exported as such, not trimmed — same construct real-file's wip-banner
// already exercises, repeated here as its own single-construct fixture), and
// a mask pair. The mask pair sits in its own sub-frame: isMask masks ALL
// subsequent siblings in the children array (BlendMixin.isMask), so
// containing the mask and the node it masks in a dedicated frame keeps the
// propagation scoped to that pair and out of the other three constructs.
//
// story C2 (#143) fix: the mask shape is a full circle (not an oblong
// ellipse) with maskType explicitly "VECTOR" — the Plugin API's name for
// the REST API's "OUTLINE" — so it exercises dashc's box-outline mask
// lowering rather than the default ALPHA mask type, which dashc refuses by
// name (a soft mask has no hard box-clip lowering). A file captured before
// this fix carries the old ALPHA/oblong mask shape and needs this command
// re-run in Figma, then a re-capture, before its mask exercises the
// lowering.
function nodeFx() {
  const root = baseFrame("node-fx", 540, 160);
  root.layoutMode = "NONE";

  const CELL = 100;
  const GAP = 24;
  const MARGIN = 30;
  const Y0 = 30;
  const slotX = (i) => MARGIN + i * (CELL + GAP);

  // (a) rotation
  const rotated = figma.createRectangle();
  rotated.name = "rotated-15deg";
  rotated.resize(CELL, CELL);
  rotated.fills = [solid({ r: 0.35, g: 0.6, b: 0.9 })];
  rotated.rotation = 15;
  root.appendChild(rotated);
  rotated.x = slotX(0);
  rotated.y = Y0;

  // (b) partial opacity
  const halfOpacity = figma.createRectangle();
  halfOpacity.name = "half-opacity";
  halfOpacity.resize(CELL, CELL);
  halfOpacity.fills = [solid({ r: 0.9, g: 0.35, b: 0.4 })];
  halfOpacity.opacity = 0.5;
  root.appendChild(halfOpacity);
  halfOpacity.x = slotX(1);
  halfOpacity.y = Y0;

  // (c) hidden layer
  const hidden = figma.createRectangle();
  hidden.name = "hidden-layer";
  hidden.resize(CELL, CELL);
  hidden.fills = [solid({ r: 0.4, g: 0.8, b: 0.5 })];
  hidden.visible = false;
  root.appendChild(hidden);
  hidden.x = slotX(2);
  hidden.y = Y0;

  // (d) a mask pair: mask-shape is appended first (the "lower" sibling in
  // the children array) and masks masked-content, its one subsequent
  // sibling, via isMask.
  const maskPair = figma.createFrame();
  maskPair.name = "mask-pair";
  maskPair.resize(CELL, CELL);
  maskPair.fills = [];
  maskPair.clipsContent = false;
  root.appendChild(maskPair);
  maskPair.x = slotX(3);
  maskPair.y = Y0;

  // A full circle (equal width/height): the ellipse-as-circle lowering
  // limit (docs/decisions/figma-ellipse-as-circle.md) — a non-circular
  // ellipse refuses regardless of masking. `maskType` defaults to `"ALPHA"`
  // (the Plugin API's `MaskType`), which dashc refuses by name (a soft mask
  // has no hard box-clip lowering, docs/decisions/masks-and-group-opacity.md);
  // `"VECTOR"` is the Plugin API's name for what the REST API reports as
  // `"OUTLINE"` — the box-outline mask dashc's lowering accepts.
  const maskShape = figma.createEllipse();
  maskShape.name = "mask-shape";
  maskShape.resize(CELL, CELL);
  maskShape.fills = [solid(GRAY(0))];
  maskPair.appendChild(maskShape);
  maskShape.x = 0;
  maskShape.y = 0;

  const maskedContent = figma.createRectangle();
  maskedContent.name = "masked-content";
  maskedContent.resize(CELL, CELL);
  maskedContent.fills = [solid({ r: 0.9, g: 0.7, b: 0.2 })];
  maskPair.appendChild(maskedContent);
  maskedContent.x = 0;
  maskedContent.y = 0;
  maskShape.isMask = true;
  maskShape.maskType = "VECTOR";

  return "node-fx built: rotated-15deg rect, half-opacity rect, a hidden " +
    "layer, and a mask pair (full-circle ellipse isMask masking a rect, " +
    "maskType VECTOR) in a 540x160 frame";
}

// ------------------------------------------------------------------ dispatch
const COMMANDS = {
  "v03-paint": v03Paint,
  "grid-basic": gridBasic,
  "variables-bound": variablesBound,
  "effects-2025": effects2025,
  "lowering-wrap": loweringWrap,
  "lowering-hug-in-fill": loweringHugInFill,
  "lowering-negative-gap": loweringNegativeGap,
  "lowering-baseline": loweringBaseline,
  "lowering-variant-topology": loweringVariantTopology,
  "real-file": realFile,
  "trim-demo": trimDemo,
  "text-latin": textLatin,
  "text-arabic": textArabic,
  "text-baseline": textBaseline,
  "drop-shadow": dropShadow,
  "inner-shadow": innerShadow,
  "liga-text": ligaText,
  "jpeg-fill": jpegFill,
  "gif-fill": gifFill,
  "vector-shapes": vectorShapes,
  "stacked-fills": stackedFills,
  "node-fx": nodeFx,
};

(async () => {
  const fn = COMMANDS[figma.command];
  if (!fn) {
    figma.closePlugin("unknown command: " + figma.command);
    return;
  }
  try {
    const msg = await fn();
    figma.viewport.scrollAndZoomIntoView(figma.currentPage.children);
    figma.closePlugin(msg);
  } catch (e) {
    console.error(e);
    figma.closePlugin("FAILED: " + (e && e.message ? e.message : String(e)));
  }
})();
