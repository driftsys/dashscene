/**
 * Export manifest and reachability closure (story #37; the design record is
 * docs/archive/2026-07-14-design-1-seed.md §6.1, owned Deno-side per
 * docs/decisions/figma-importer-deno-plus-dashc-wasm.md).
 *
 * An export is DECLARED, never positional: the manifest lists root frames by
 * stable node id, the closure proves what those roots require — children,
 * referenced components (per component SET), image fills — and nothing else
 * enters the document. A top-level node the manifest does not declare is
 * excluded **by name** (P4: never a silent drop). This replaces the
 * first-frame-wins selection on the importer path only: a direct caller of
 * the dashc ABI still hits `root_frame`'s positional selection, so debt
 * #147 stays open for that path. Variant closure is per component set: the
 * runtime can select any member, so the whole set ships unless the manifest
 * declares a frozen subset explicitly.
 *
 * The closure also names the `imageRef`s the export requires, which is what
 * lets the import flow cross the wasm ABI exactly once (debt #155). That
 * walk is a second copy of "where an imageRef lives in Figma's shape", so
 * `Dashc.figmaImageRefs` stays the drift oracle: closure_test.ts pins the two
 * answers equal on a frame-rooted captured fixture, on a component-carrying one
 * (the walk lowers COMPONENT_SET/INSTANCE roots since #242,
 * docs/decisions/figma-component-lowering.md), and on a synthetic case whose
 * ref lives inside a component definition. A ref the closure misses still fails
 * the compile loudly (`unresolvedImage`, R6) rather than dropping the fill
 * (docs/decisions/figma-image-refs-resolved-by-the-caller.md).
 *
 * Cross-file resolution is story #38: a `remote` component is recorded as a
 * requirement carrying its library key, and `resolveRemoteComponents` resolves
 * that key against the libraries the export manifest declares — splicing the
 * library's definition into the document before the final closure runs — or
 * names it unresolvable (P4). See docs/decisions/figma-cross-file-library-resolution.md.
 */

import { rebuildChildren } from "./tree.ts";

/** One fill or stroke, as far as the closure needs to read it. */
export interface ClosurePaint {
  readonly type: string;
  readonly imageRef?: string;
}

/** One node of the Figma tree, as far as the closure needs to read it. */
export interface ClosureNode {
  readonly id: string;
  readonly name: string;
  readonly type: string;
  readonly children?: readonly ClosureNode[];
  readonly fills?: readonly ClosurePaint[];
  readonly strokes?: readonly ClosurePaint[];
  /** Present on `INSTANCE` nodes: the component the instance references. */
  readonly componentId?: string;
}

/** One entry of the file's top-level `components` map. */
export interface ComponentMeta {
  readonly key: string;
  readonly remote?: boolean;
  readonly componentSetId?: string;
}

/** One entry of the file's top-level `componentSets` map. */
export interface ComponentSetMeta {
  readonly key: string;
}

/**
 * The `GET /file` shape the closure reads. Structural on purpose: the real
 * `GetFileResponse` satisfies it, and a synthetic test file does not have to
 * carry every REST field.
 */
export interface ClosureFile {
  readonly document: ClosureNode;
  readonly components?: Readonly<Record<string, ComponentMeta>>;
  readonly componentSets?: Readonly<Record<string, ComponentSetMeta>>;
}

/** What ships from a file: roots by stable id, subsets by explicit declaration. */
export interface ExportManifest {
  /** Stable node ids of the top-level frames to export. */
  readonly roots: readonly string[];
  /**
   * Component-set id → the explicit subset of member component ids that
   * ships. Absent set: the whole set ships (the runtime can select any
   * member). A frozen subset is a declaration, never an inference.
   */
  readonly frozenVariants?: Readonly<Record<string, readonly string[]>>;
  /**
   * Figma file keys of the libraries this export may resolve remote components
   * from (#38). A library dependency is declared, never auto-discovered — the
   * same principle as a declared root. Absent: no library resolves, so every
   * remote component the export reaches is a named `cross-file-unresolved`
   * error (P4).
   */
  readonly libraries?: readonly string[];
}

/** A closure verdict: named, never silent (P4). */
export interface ClosureDiagnostic {
  readonly rule: string;
  readonly severity: "error" | "warning";
  readonly message: string;
  /** The node the verdict points at, when there is one. */
  readonly nodeId?: string;
}

/** A top-level node excluded by declaration — named, never silently dropped. */
export interface ExcludedNode {
  readonly id: string;
  readonly name: string;
  readonly type: string;
  readonly canvas: string;
}

/** One component the closure proved the export requires. */
export interface ComponentRequirement {
  readonly componentId: string;
  /** The library key — the handle cross-file resolution (#38) resolves by. */
  readonly key: string;
  readonly remote: boolean;
  /** Present when the component is a variant in a component set. */
  readonly setId?: string;
}

/** One component set the closure pulled, with the members that ship. */
export interface VariantSetRequirement {
  readonly setId: string;
  readonly key: string;
  /** Member component ids — the whole set, or the declared frozen subset. */
  readonly members: readonly string[];
}

export interface Closure {
  /**
   * The pruned file: the declared roots and the component sets they require,
   * under their canvases, everything else removed. Node objects are the
   * input's own (verbatim), so serializing this preserves every field the
   * capture carried. The canvas holding the first declared root comes first,
   * because the walk in `dashc` starts at the first canvas.
   */
  readonly file: ClosureFile;
  /** Every node id that ships (roots' subtrees plus pulled components). */
  readonly nodeIds: ReadonlySet<string>;
  /** The image fills the closure requires, sorted and deduplicated. */
  readonly imageRefs: readonly string[];
  readonly components: readonly ComponentRequirement[];
  readonly variantSets: readonly VariantSetRequirement[];
  readonly excluded: readonly ExcludedNode[];
  /** Errors block the export (R6); see {@link ExportBlocked}. */
  readonly diagnostics: readonly ClosureDiagnostic[];
}

/** An export the closure refuses. R6: an error blocks, never a silent drop. */
export class ExportBlocked extends Error {
  readonly diagnostics: readonly ClosureDiagnostic[];

  constructor(diagnostics: readonly ClosureDiagnostic[]) {
    super(
      diagnostics
        .map((d) => `${d.severity}[${d.rule}]: ${d.message}`)
        .join("\n"),
    );
    this.name = "ExportBlocked";
    this.diagnostics = diagnostics;
  }
}

/** Parses an export manifest, throwing a named error on a malformed one. */
export function parseExportManifest(text: string): ExportManifest {
  const parsed = JSON.parse(text) as
    | { roots?: unknown; frozenVariants?: unknown; libraries?: unknown }
    | null;
  if (parsed === null || typeof parsed !== "object") {
    throw new Error("the export manifest is not a JSON object");
  }
  if (!Array.isArray(parsed.roots) || parsed.roots.length === 0) {
    throw new Error(
      "the export manifest declares no roots — an export is declared, " +
        "never positional",
    );
  }
  for (const root of parsed.roots) {
    if (typeof root !== "string" || root.length === 0) {
      throw new Error(
        `the export manifest has a root that is not a node id: ${
          JSON.stringify(root)
        }`,
      );
    }
  }
  const roots = parsed.roots as string[];

  let libraries: string[] | undefined;
  if (parsed.libraries !== undefined) {
    if (!Array.isArray(parsed.libraries)) {
      throw new Error("the export manifest's libraries must be an array");
    }
    for (const key of parsed.libraries) {
      if (typeof key !== "string" || key.length === 0) {
        throw new Error(
          `the export manifest has a library that is not a file key: ${
            JSON.stringify(key)
          }`,
        );
      }
    }
    libraries = parsed.libraries as string[];
  }

  if (parsed.frozenVariants === undefined) {
    return libraries === undefined ? { roots } : { roots, libraries };
  }
  const frozen = parsed.frozenVariants;
  if (frozen === null || typeof frozen !== "object" || Array.isArray(frozen)) {
    throw new Error("frozenVariants must map a set id to member node ids");
  }
  for (const [setId, members] of Object.entries(frozen)) {
    if (
      !Array.isArray(members) ||
      members.some((m) => typeof m !== "string" || m.length === 0)
    ) {
      throw new Error(
        `frozenVariants["${setId}"] must be an array of member node ids`,
      );
    }
  }
  return {
    roots,
    frozenVariants: frozen as Record<string, readonly string[]>,
    ...(libraries === undefined ? {} : { libraries }),
  };
}

/** One declarable export root: a top-level child of a canvas. */
export interface ExportableRoot {
  readonly canvas: string;
  readonly id: string;
  readonly name: string;
  readonly type: string;
}

/** Lists every top-level node, for the "declare your roots" error path. */
export function exportableRoots(file: ClosureFile): ExportableRoot[] {
  const roots: ExportableRoot[] = [];
  for (const canvas of canvasesOf(file)) {
    for (const node of canvas.children ?? []) {
      roots.push({
        canvas: canvas.name,
        id: node.id,
        name: node.name,
        type: node.type,
      });
    }
  }
  return roots;
}

function canvasesOf(file: ClosureFile): readonly ClosureNode[] {
  return (file.document.children ?? []).filter((n) => n.type === "CANVAS");
}

/** One node's paints — fills then strokes — the one place both walks read. */
function paintsOf(node: ClosureNode): readonly ClosurePaint[] {
  return [...(node.fills ?? []), ...(node.strokes ?? [])];
}

/** The whole-tree index the closure resolves ids against. */
interface Index {
  /** Every node in the document, by id. */
  readonly byId: ReadonlyMap<string, ClosureNode>;
  /** Node id → the top-level canvas child whose subtree holds it. */
  readonly topOf: ReadonlyMap<string, ClosureNode>;
  /** Top-level node id → its canvas. */
  readonly canvasOf: ReadonlyMap<string, ClosureNode>;
}

function indexFile(file: ClosureFile): Index {
  const byId = new Map<string, ClosureNode>();
  const topOf = new Map<string, ClosureNode>();
  const canvasOf = new Map<string, ClosureNode>();
  for (const canvas of canvasesOf(file)) {
    for (const top of canvas.children ?? []) {
      canvasOf.set(top.id, canvas);
      const stack = [top];
      while (stack.length > 0) {
        const node = stack.pop() as ClosureNode;
        byId.set(node.id, node);
        topOf.set(node.id, top);
        for (const child of node.children ?? []) stack.push(child);
      }
    }
  }
  return { byId, topOf, canvasOf };
}

export function computeClosure(
  file: ClosureFile,
  manifest: ExportManifest,
): Closure {
  const index = indexFile(file);
  const diagnostics: ClosureDiagnostic[] = [];
  const nodeIds = new Set<string>();
  const imageRefs = new Set<string>();
  const pendingComponents: string[] = [];
  /**
   * The frozenVariants keys the walk actually applied. A key that ends up
   * outside this set froze nothing, which is a named error, never a silent
   * no-op (see the post-pass below).
   */
  const narrowedSets = new Set<string>();

  /**
   * Walks one shipped subtree: records every node id, collects image refs
   * from fills and strokes, and queues the components its instances
   * reference.
   *
   * Refs are collected from visible and invisible paints alike — the same
   * deliberate superset `dashc`'s own scan takes: fetching an unused image
   * costs one download, missing one is a failed compile.
   *
   * A `COMPONENT_SET` with a frozen declaration is narrowed here, wherever
   * it lives — at the canvas top level or nested in a kept subtree: the
   * withdrawn members are never visited, so neither their node ids nor
   * their refs nor their instances enter the closure.
   */
  const walk = (root: ClosureNode): void => {
    const stack = [root];
    while (stack.length > 0) {
      const node = stack.pop() as ClosureNode;
      nodeIds.add(node.id);
      for (const paint of paintsOf(node)) {
        if (paint.type === "IMAGE" && paint.imageRef) {
          imageRefs.add(paint.imageRef);
        }
      }
      if (node.type === "INSTANCE" && node.componentId) {
        pendingComponents.push(node.componentId);
      }
      let children = node.children ?? [];
      const frozen = node.type === "COMPONENT_SET"
        ? manifest.frozenVariants?.[node.id]
        : undefined;
      if (frozen !== undefined) {
        narrowedSets.add(node.id);
        children = children.filter(
          (child) => child.type !== "COMPONENT" || frozen.includes(child.id),
        );
      }
      // Reversed so ids and refs accumulate in document order (refs are
      // sorted anyway; instances resolve in a stable, readable order).
      for (const child of [...children].reverse()) {
        stack.push(child);
      }
    }
  };

  // -- Declared roots -------------------------------------------------
  const keptTop = new Set<string>();
  for (const rootId of manifest.roots) {
    if (keptTop.has(rootId)) {
      diagnostics.push({
        rule: "figma.closure.duplicate-root",
        severity: "error",
        message: `root ${rootId} is declared more than once`,
        nodeId: rootId,
      });
      continue;
    }
    const node = index.byId.get(rootId);
    if (node === undefined) {
      diagnostics.push({
        rule: "figma.closure.unknown-root",
        severity: "error",
        message: `declared root ${rootId} is not in the file — ` +
          `declarable roots: ${
            exportableRoots(file)
              .map((r) => `${r.id} (${r.type} "${r.name}")`)
              .join(", ")
          }`,
        nodeId: rootId,
      });
      continue;
    }
    if (!index.canvasOf.has(rootId)) {
      const top = index.topOf.get(rootId) as ClosureNode;
      diagnostics.push({
        rule: "figma.closure.nested-root",
        severity: "error",
        message: `declared root ${rootId} ("${node.name}") is nested ` +
          `inside "${top.name}" (${top.id}) — a root must be a top-level ` +
          `child of a canvas`,
        nodeId: rootId,
      });
      continue;
    }
    keptTop.add(rootId);
    walk(node);
  }

  // -- Component closure, per component SET ---------------------------
  const components: ComponentRequirement[] = [];
  const variantSets: VariantSetRequirement[] = [];
  const resolvedComponents = new Set<string>();
  const resolvedSets = new Set<string>();

  /** Ships one component-definition subtree, or diagnoses why it cannot. */
  const includeDefinition = (node: ClosureNode): void => {
    if (nodeIds.has(node.id)) return; // already inside a kept subtree
    const top = index.topOf.get(node.id) as ClosureNode;
    if (top.id !== node.id && !keptTop.has(top.id)) {
      diagnostics.push({
        rule: "figma.closure.buried-component",
        severity: "error",
        message: `component ${node.id} ("${node.name}") is reachable only ` +
          `through the undeclared top-level node "${top.name}" (${top.id}) ` +
          `— declare it, or move the component to the canvas`,
        nodeId: node.id,
      });
      return;
    }
    keptTop.add(top.id);
    walk(node);
  };

  // An index pointer rather than shift(): the walk appends while this loop
  // runs, and shifting the head of a growing array is quadratic.
  for (let at = 0; at < pendingComponents.length; at++) {
    const componentId = pendingComponents[at];
    if (resolvedComponents.has(componentId)) continue;
    resolvedComponents.add(componentId);

    const meta = file.components?.[componentId];
    if (meta === undefined) {
      diagnostics.push({
        rule: "figma.closure.unresolved-component",
        severity: "error",
        message: `an instance references component ${componentId}, which ` +
          `the file's components map does not carry`,
        nodeId: componentId,
      });
      continue;
    }
    components.push({
      componentId,
      key: meta.key,
      remote: meta.remote ?? false,
      ...(meta.componentSetId === undefined
        ? {}
        : { setId: meta.componentSetId }),
    });
    // A remote component lives in a library file, so it is not in this
    // document's tree. The requirement is recorded (above) and the walk stops
    // here. `resolveRemoteComponents` (#38) is the single owner of the
    // cross-file verdict: it splices a declared library's definition in before
    // the final closure runs — so a resolved remote is local by the time this
    // branch sees it again — or it names the remote unresolvable (P4). The
    // closure alone never diagnoses a remote, which would double the verdict.
    if (meta.remote) continue;

    const setId = meta.componentSetId;
    if (setId === undefined) {
      const node = index.byId.get(componentId);
      if (node === undefined) {
        diagnostics.push({
          rule: "figma.closure.unresolved-component",
          severity: "error",
          message: `component ${componentId} (key ${meta.key}) is in the ` +
            `components map but not in the document tree`,
          nodeId: componentId,
        });
        continue;
      }
      includeDefinition(node);
      continue;
    }

    // A frozen subset that excludes a referenced member contradicts a
    // shipped instance: the instance's componentId would dangle. Named,
    // never silent (P4).
    const frozen = manifest.frozenVariants?.[setId];
    if (frozen !== undefined && !frozen.includes(componentId)) {
      diagnostics.push({
        rule: "figma.closure.frozen-variant-excluded",
        severity: "error",
        message: `an instance references component ${componentId} ` +
          `(key ${meta.key}), which frozenVariants["${setId}"] excludes — ` +
          `a frozen subset must cover every shipped instance`,
        nodeId: componentId,
      });
    }

    if (resolvedSets.has(setId)) continue;
    resolvedSets.add(setId);
    const setNode = index.byId.get(setId);
    if (setNode === undefined) {
      diagnostics.push({
        rule: "figma.closure.unresolved-component",
        severity: "error",
        message: `component ${componentId} belongs to component set ` +
          `${setId}, which is not in the document tree`,
        nodeId: setId,
      });
      continue;
    }

    const memberIds = (setNode.children ?? [])
      .filter((n) => n.type === "COMPONENT")
      .map((n) => n.id);
    variantSets.push({
      setId,
      key: file.componentSets?.[setId]?.key ?? "",
      members: frozen === undefined
        ? memberIds
        : frozen.filter((id) => memberIds.includes(id)),
    });

    // Ship the set. The walk itself applies the frozen narrowing, so a
    // withdrawn variant's node ids, image fills, and nested instances never
    // enter the closure. A set inside an already-kept subtree was walked —
    // and narrowed — with that subtree, so there is nothing to add.
    if (!nodeIds.has(setNode.id)) {
      const top = index.topOf.get(setId) as ClosureNode;
      if (top.id !== setId && !keptTop.has(top.id)) {
        diagnostics.push({
          rule: "figma.closure.buried-component",
          severity: "error",
          message: `component set ${setId} ("${setNode.name}") is ` +
            `reachable only through the undeclared top-level node ` +
            `"${top.name}" (${top.id}) — declare it, or move the set to ` +
            `the canvas`,
          nodeId: setId,
        });
        continue;
      }
      keptTop.add(top.id);
      walk(setNode);
    }
  }

  // Every frozen declaration must have done something. A key the walk never
  // applied froze nothing — a typo would otherwise ship every variant in
  // silence — and a member id that is not in its set is a declaration about
  // a node that does not exist.
  for (const [setId, frozen] of Object.entries(manifest.frozenVariants ?? {})) {
    if (!narrowedSets.has(setId)) {
      diagnostics.push({
        rule: "figma.closure.frozen-variants-unused",
        severity: "error",
        message: `frozenVariants["${setId}"] froze nothing — no component ` +
          `set with that id entered the closure`,
        nodeId: setId,
      });
      continue;
    }
    const setNode = index.byId.get(setId) as ClosureNode;
    const memberIds = (setNode.children ?? [])
      .filter((n) => n.type === "COMPONENT")
      .map((n) => n.id);
    for (const id of frozen) {
      if (memberIds.includes(id)) continue;
      diagnostics.push({
        rule: "figma.closure.frozen-variant-unknown",
        severity: "error",
        message: `frozenVariants["${setId}"] declares ${id}, which is ` +
          `not a member of the set (members: ${memberIds.join(", ")})`,
        nodeId: id,
      });
    }
  }

  // -- Exclusions, named (P4) ------------------------------------------
  const excluded: ExcludedNode[] = [];
  for (const canvas of canvasesOf(file)) {
    for (const top of canvas.children ?? []) {
      if (keptTop.has(top.id)) continue;
      excluded.push({
        id: top.id,
        name: top.name,
        type: top.type,
        canvas: canvas.name,
      });
    }
  }

  // -- The pruned file ---------------------------------------------------
  /**
   * Applies the frozen narrowing to the serialized tree, so the pruned file
   * agrees with `nodeIds` wherever the set lives — nested included. A
   * subtree with nothing to narrow is returned by reference (via
   * `rebuildChildren`), so everything outside a narrowed set serializes
   * verbatim. Recursive: the input is a Figma REST response, whose nesting is
   * bounded in practice (and capped again by dashc's depth guard before it is
   * compiled).
   */
  const narrowTree = (node: ClosureNode): ClosureNode => {
    const narrowed = node.type === "COMPONENT_SET" && narrowedSets.has(node.id);
    return rebuildChildren(
      node,
      (child) =>
        narrowed && child.type === "COMPONENT" && !nodeIds.has(child.id)
          ? null
          : narrowTree(child),
    );
  };

  const firstRootCanvas = manifest.roots
    .map((id) => index.canvasOf.get(id))
    .find((canvas) => canvas !== undefined);
  const keptCanvases = canvasesOf(file)
    .map((canvas) => ({
      canvas,
      children: (canvas.children ?? [])
        .filter((top) => keptTop.has(top.id))
        .map(narrowTree),
    }))
    .filter(({ children }) => children.length > 0)
    // The walk in dashc starts at the first canvas, so the first declared
    // root's canvas leads; the rest keep document order.
    .sort((a, b) =>
      Number(b.canvas === firstRootCanvas) -
      Number(a.canvas === firstRootCanvas)
    )
    .map(({ canvas, children }) => ({ ...canvas, children }));

  const pruned: ClosureFile = {
    ...file,
    document: { ...file.document, children: keptCanvases },
  };

  return {
    file: pruned,
    nodeIds,
    imageRefs: [...imageRefs].sort(),
    components,
    variantSets,
    excluded,
    diagnostics,
  };
}

// -- Cross-file resolution (#38) ---------------------------------------------

/**
 * A library file the export declared and the caller fetched, so its component
 * definitions can be resolved against the remote requirements a consumer file
 * carries. The importer fetches one with `GET /file` per declared library key
 * (docs/decisions/figma-cross-file-library-resolution.md).
 */
export interface ResolvedLibrary {
  /** The library's Figma file key — provenance for a resolved definition. */
  readonly fileKey: string;
  /** The library's `GET /file` response. */
  readonly file: ClosureFile;
}

/** One remote component resolved to a library definition — provenance. */
export interface ResolvedRemote {
  /** The consumer-side (phantom) component id the instance references. */
  readonly componentId: string;
  /** The library key the resolution matched on. */
  readonly key: string;
  /** The library file the definition came from. */
  readonly libraryFileKey: string;
}

/**
 * The consumer file with its resolved remote definitions spliced in, plus the
 * provenance of what resolved and named verdicts for what did not.
 */
export interface RemoteResolution {
  /**
   * The consumer file with each resolved library definition spliced in as a
   * local, resolve-but-do-not-paint top-level node. A remote requirement that
   * did not resolve is left untouched (still `remote: true`) and named in
   * {@link diagnostics}.
   */
  readonly file: ClosureFile;
  /** Remote requirements resolved to a library, in resolution order. */
  readonly resolved: readonly ResolvedRemote[];
  /**
   * The ids of the spliced definitions' top-level nodes. These resolve but do
   * not paint, and their bindings live in the library's variable space, so the
   * caller excludes them from sidecar derivation
   * (docs/decisions/figma-cross-file-library-resolution.md, C3/#167).
   */
  readonly splicedRootIds: readonly string[];
  /**
   * Named verdicts: an unresolvable key, a spliced definition this slice cannot
   * fully carry (a cross-file image fill), or a shadowed library key (a
   * warning). An error here blocks the export the same way an unknown root does
   * (P4).
   */
  readonly diagnostics: readonly ClosureDiagnostic[];
}

/** One declared library, indexed for resolution by key and by node id. */
interface LibraryIndex {
  readonly fileKey: string;
  readonly file: ClosureFile;
  /** Every node in the library document, by id. */
  readonly byId: ReadonlyMap<string, ClosureNode>;
}

/** Indexes every node in a document by id (the whole tree, not just tops). */
function indexDocument(document: ClosureNode): Map<string, ClosureNode> {
  const byId = new Map<string, ClosureNode>();
  const stack = [document];
  while (stack.length > 0) {
    const node = stack.pop() as ClosureNode;
    byId.set(node.id, node);
    for (const child of node.children ?? []) stack.push(child);
  }
  return byId;
}

/** True when the subtree carries an image fill or stroke on any node. */
function subtreeHasImagePaint(node: ClosureNode): boolean {
  if (paintsOf(node).some((paint) => paint.type === "IMAGE")) return true;
  return (node.children ?? []).some(subtreeHasImagePaint);
}

/**
 * Deep-clones a subtree, remapping every id reference through `mapId` — the
 * node's own `id` and the `componentId` a nested `INSTANCE` points at. Every
 * other field is copied verbatim, so a spliced definition keeps the paint,
 * layout, and text fields the drift oracle and the lowering read.
 *
 * Remapping `componentId` is what keeps a library and its consumer from ever
 * confusing ids: a nested instance inside a spliced definition points at a
 * library-space component id that could collide with a different consumer
 * component (both files mint ids from `0:0`), so the reference is remapped into
 * the same namespace as the node it targets.
 */
function reidSubtree(
  node: ClosureNode,
  mapId: (id: string) => string,
): ClosureNode {
  const children = node.children?.map((child) => reidSubtree(child, mapId));
  return {
    ...node,
    id: mapId(node.id),
    ...(node.componentId === undefined
      ? {}
      : { componentId: mapId(node.componentId) }),
    ...(children === undefined ? {} : { children }),
  };
}

/** Collects every node id inside the given subtree roots (library id space). */
function descendantIds(
  byId: ReadonlyMap<string, ClosureNode>,
  roots: readonly string[],
): Set<string> {
  const ids = new Set<string>();
  for (const root of roots) {
    const node = byId.get(root);
    if (node === undefined) continue;
    const stack = [node];
    while (stack.length > 0) {
      const at = stack.pop() as ClosureNode;
      ids.add(at.id);
      for (const child of at.children ?? []) stack.push(child);
    }
  }
  return ids;
}

/** A requirement matched to the declared library that carries its key. */
interface HitRequirement {
  readonly remote: ComponentRequirement;
  readonly library: LibraryIndex;
  /** The matched component's node id in the library's own id space. */
  readonly libNodeId: string;
}

/**
 * Resolves the remote component requirements a consumer file carries against
 * the libraries the export declared (#38).
 *
 * Resolution is by key, never by id: a library and the consumer that instances
 * it have independent id spaces, so a remote requirement carries the library
 * `key`, and the matching definition is found by inverting each library's
 * `components` map. A resolved definition is spliced into the consumer document
 * as a local, resolve-but-do-not-paint top-level node — its whole subtree re-id'd
 * into a per-library namespace (`<libraryFileKey>~<id>`), except the directly
 * required member and its set, which are anchored to the consumer's own phantom
 * ids. The final closure then treats a spliced definition as an ordinary
 * in-document one. The instance still paints from its own baked subtree
 * (docs/decisions/figma-component-lowering.md); the spliced definition only
 * makes the closure stop refusing the export.
 *
 * References are remapped, not just node ids: a nested `INSTANCE` inside a
 * spliced definition points at a library-space component, so its `componentId`
 * is remapped into the same namespace, and the library component it names is
 * spliced too — transitively — so the reference resolves to a real definition.
 * A library-internal reference resolves against that library's own `components`
 * map; a nested reference into yet another library is named and deferred.
 *
 * When the remote is a variant, the whole component set is spliced (variant
 * closure is per set), so frozen-variant narrowing applies across files exactly
 * as it does to a local set.
 *
 * Unresolvable requirements are named, never silently skipped (P4): a key no
 * declared library carries is `cross-file-unresolved`; a spliced definition
 * whose subtree carries an image fill is `cross-file-image`, deferred because
 * this slice resolves image bytes from the consumer file only; a key more than
 * one declared library carries is a `cross-file-key-shadowed` warning.
 */
export function resolveRemoteComponents(
  file: ClosureFile,
  remotes: readonly ComponentRequirement[],
  libraries: readonly ResolvedLibrary[],
): RemoteResolution {
  const diagnostics: ClosureDiagnostic[] = [];
  const resolved: ResolvedRemote[] = [];
  const splicedNodes: ClosureNode[] = [];
  const splicedRootIds: string[] = [];
  const localizedComponents: Record<string, ComponentMeta> = {};
  const localizedSets: Record<string, ComponentSetMeta> = {};

  const indexes: LibraryIndex[] = libraries.map((library) => ({
    fileKey: library.fileKey,
    file: library.file,
    byId: indexDocument(library.file.document),
  }));

  // Invert every library's components map: global key -> where it lives. The
  // first declared library that carries a key wins; a later library that also
  // carries it is shadowed (named below), never silently preferred.
  const byKey = new Map<string, { library: LibraryIndex; libNodeId: string }>();
  const carriers = new Map<string, string[]>();
  for (const library of indexes) {
    for (
      const [libNodeId, meta] of Object.entries(library.file.components ?? {})
    ) {
      // A library's OWN definitions are local to it; its remote entries point
      // at yet another file and are not what this library resolves.
      if (meta.remote) continue;
      let carrying = carriers.get(meta.key);
      if (carrying === undefined) carriers.set(meta.key, carrying = []);
      carrying.push(library.fileKey);
      if (!byKey.has(meta.key)) byKey.set(meta.key, { library, libNodeId });
    }
  }

  const declaredList = libraries.map((l) => l.fileKey).join(", ") || "(none)";

  // Match each requirement to its library; name the unresolvable ones and warn
  // once per shadowed key (P4/C4).
  const hits: HitRequirement[] = [];
  const shadowWarned = new Set<string>();
  for (const remote of remotes) {
    const found = byKey.get(remote.key);
    if (found === undefined) {
      diagnostics.push({
        rule: "figma.closure.cross-file-unresolved",
        severity: "error",
        message: `component ${remote.componentId} (key ${remote.key}) is ` +
          `remote and no declared library carries it — declared ` +
          `libraries: ${declaredList}`,
        nodeId: remote.componentId,
      });
      continue;
    }
    const declaredBy = carriers.get(remote.key) as string[];
    if (declaredBy.length > 1 && !shadowWarned.has(remote.key)) {
      shadowWarned.add(remote.key);
      const shadowed = declaredBy.filter((fk) => fk !== found.library.fileKey);
      diagnostics.push({
        rule: "figma.closure.cross-file-key-shadowed",
        severity: "warning",
        message: `key ${remote.key} is declared by more than one library — ` +
          `resolved from ${found.library.fileKey}; also carried by ` +
          `${shadowed.join(", ")} (ignored)`,
        nodeId: remote.componentId,
      });
    }
    hits.push({ remote, library: found.library, libNodeId: found.libNodeId });
  }

  // Splice per library, in declared order (determinism). Each library's spliced
  // content is one namespace, so transitive references stay inside it.
  for (const library of indexes) {
    const reqs = hits.filter((h) => h.library === library);
    if (reqs.length === 0) continue;

    const anchors = new Map<string, string>(); // library id -> consumer id
    const defRoots: string[] = []; // library node ids spliced as top-level nodes
    const defRootSet = new Set<string>();
    const reqKeys: string[] = [];
    const addDefRoot = (id: string) => {
      if (!defRootSet.has(id)) {
        defRootSet.add(id);
        defRoots.push(id);
      }
    };

    for (const req of reqs) {
      const meta = library.file.components?.[req.libNodeId];
      reqKeys.push(req.remote.key);
      if (req.remote.setId !== undefined) {
        const libSetId = meta?.componentSetId;
        const setNode = libSetId === undefined
          ? undefined
          : library.byId.get(libSetId);
        if (libSetId === undefined || setNode === undefined) {
          diagnostics.push({
            rule: "figma.closure.cross-file-unresolved",
            severity: "error",
            message: `component ${req.remote.componentId} (key ` +
              `${req.remote.key}) is a variant, but library ` +
              `${library.fileKey} carries no component set for it`,
            nodeId: req.remote.componentId,
          });
          continue;
        }
        anchors.set(libSetId, req.remote.setId);
        anchors.set(req.libNodeId, req.remote.componentId);
        addDefRoot(libSetId);
      } else {
        anchors.set(req.libNodeId, req.remote.componentId);
        addDefRoot(req.libNodeId);
      }
      resolved.push({
        componentId: req.remote.componentId,
        key: req.remote.key,
        libraryFileKey: library.fileKey,
      });
    }

    if (defRoots.length === 0) continue; // every requirement for this library failed

    // Transitive expansion: follow the componentId of every nested instance to
    // the library component it names, and splice that definition too, until no
    // reference points outside the spliced content.
    const prefix = `${library.fileKey}~`;
    let splicedIds = descendantIds(library.byId, defRoots);
    const seenRefs = new Set<string>();
    for (let changed = true; changed;) {
      changed = false;
      for (const id of [...splicedIds]) {
        const node = library.byId.get(id);
        if (node?.type !== "INSTANCE" || node.componentId === undefined) {
          continue;
        }
        const cid = node.componentId;
        if (seenRefs.has(cid) || splicedIds.has(cid)) continue;
        seenRefs.add(cid);
        const cmeta = library.file.components?.[cid];
        if (cmeta === undefined) {
          diagnostics.push({
            rule: "figma.closure.cross-file-unresolved",
            severity: "error",
            message: `a spliced definition from ${library.fileKey} instances ` +
              `${cid}, which the library's components map does not carry`,
            nodeId: prefix + id,
          });
          continue;
        }
        if (cmeta.remote) {
          diagnostics.push({
            rule: "figma.closure.cross-file-transitive-remote",
            severity: "error",
            message: `a spliced definition from ${library.fileKey} instances ` +
              `another library's component (key ${cmeta.key}) — transitive ` +
              `cross-library resolution is a follow-up`,
            nodeId: prefix + id,
          });
          continue;
        }
        addDefRoot(cmeta.componentSetId ?? cid);
        changed = true;
      }
      if (changed) splicedIds = descendantIds(library.byId, defRoots);
    }

    const mapId = (id: string): string => anchors.get(id) ?? prefix + id;

    let libraryHasImage = false;
    for (const defRoot of defRoots) {
      const node = library.byId.get(defRoot) as ClosureNode;
      const spliced = reidSubtree(node, mapId);
      if (subtreeHasImagePaint(spliced)) libraryHasImage = true;
      splicedNodes.push(spliced);
      splicedRootIds.push(spliced.id);
    }

    // Localize every component and set inside the spliced content, so a remapped
    // reference resolves to a local definition rather than dangling.
    for (const id of splicedIds) {
      const cmeta = library.file.components?.[id];
      if (cmeta !== undefined && !cmeta.remote) {
        localizedComponents[mapId(id)] = {
          key: cmeta.key,
          remote: false,
          ...(cmeta.componentSetId === undefined
            ? {}
            : { componentSetId: mapId(cmeta.componentSetId) }),
        };
      }
      const smeta = library.file.componentSets?.[id];
      if (smeta !== undefined) localizedSets[mapId(id)] = { key: smeta.key };
    }

    if (libraryHasImage) {
      diagnostics.push({
        rule: "figma.closure.cross-file-image",
        severity: "error",
        message: `a library definition in file ${library.fileKey} carries an ` +
          `image fill (reached by key(s) ${
            [...new Set(reqKeys)].join(", ")
          }) ` +
          `— cross-file image resolution is a follow-up`,
        nodeId: reqs[0].remote.componentId,
      });
    }
  }

  if (splicedNodes.length === 0) {
    // Nothing resolved: return the file untouched so the unresolved requirements
    // stay named, never localized behind a diagnostic.
    return { file, resolved, splicedRootIds, diagnostics };
  }

  // Splice the definitions into the first canvas, ahead of its own children, so
  // a resolved definition is a top-level child of a canvas (what the closure
  // requires of a definition). Resolution order is preserved and deterministic.
  const children = file.document.children ?? [];
  const firstCanvasAt = children.findIndex((n) => n.type === "CANVAS");
  const newChildren = children.map((node, at) =>
    at === firstCanvasAt
      ? { ...node, children: [...splicedNodes, ...(node.children ?? [])] }
      : node
  );

  const splicedFile: ClosureFile = {
    ...file,
    document: { ...file.document, children: newChildren },
    components: { ...file.components, ...localizedComponents },
    componentSets: { ...file.componentSets, ...localizedSets },
  };

  return { file: splicedFile, resolved, splicedRootIds, diagnostics };
}

/**
 * Returns the file with the named top-level canvas nodes removed. The importer
 * uses it to keep spliced library definitions out of sidecar derivation: they
 * resolve but do not paint, and their bindings' ids live in another file's
 * variable space (docs/decisions/figma-cross-file-library-resolution.md, C3/#167).
 */
export function excludeTopLevelNodes(
  file: ClosureFile,
  ids: ReadonlySet<string>,
): ClosureFile {
  const children = (file.document.children ?? []).map((canvas) =>
    canvas.type === "CANVAS"
      ? {
        ...canvas,
        children: (canvas.children ?? []).filter((n) => !ids.has(n.id)),
      }
      : canvas
  );
  return { ...file, document: { ...file.document, children } };
}
