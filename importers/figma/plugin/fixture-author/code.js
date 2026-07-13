// dashscene fixture author — development plugin, never published.
// Builds one tier-1 corpus fixture (SCOPE_DECISIONS §8) into the CURRENT
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
// Mixed-size baseline row (DESIGN Q-4: Taffy's least-exercised corner) +
// the RTL/Arabic locale variant with Arabic-Indic numerals (§8, E2).
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
    const comp = figma.createComponent();
    comp.name = "state=" + stateName;
    comp.layoutMode = "VERTICAL";
    comp.itemSpacing = 8;
    comp.paddingLeft = comp.paddingRight = 16;
    comp.paddingTop = comp.paddingBottom = 16;
    comp.fills = [solid(GRAY(0.96))];
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
