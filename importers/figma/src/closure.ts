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
 * requirement (the contract #38 builds on) and diagnosed as an error naming
 * the library key, never silently skipped.
 */

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
    | { roots?: unknown; frozenVariants?: unknown }
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

  if (parsed.frozenVariants === undefined) return { roots };
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
      for (
        const paint of [...(node.fills ?? []), ...(node.strokes ?? [])]
      ) {
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
    if (meta.remote) {
      diagnostics.push({
        rule: "figma.closure.cross-file-component",
        severity: "error",
        message: `component ${componentId} (key ${meta.key}) lives in ` +
          `another file — cross-file library resolution is story #38`,
        nodeId: componentId,
      });
      continue;
    }

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
   * subtree with nothing to narrow is returned by reference, so everything
   * outside a narrowed set serializes verbatim. Recursive: the input is a
   * Figma REST response, whose nesting is bounded in practice (and capped
   * again by dashc's depth guard before it is compiled).
   */
  const narrowTree = (node: ClosureNode): ClosureNode => {
    const kids = node.children;
    if (kids === undefined || kids.length === 0) return node;
    const kept = node.type === "COMPONENT_SET" && narrowedSets.has(node.id)
      ? kids.filter((child) =>
        child.type !== "COMPONENT" || nodeIds.has(child.id)
      )
      : kids;
    const rebuilt = kept.map(narrowTree);
    if (kept === kids && rebuilt.every((child, at) => child === kids[at])) {
      return node;
    }
    return { ...node, children: rebuilt };
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
