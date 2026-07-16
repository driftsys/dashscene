/**
 * Trim layers: the pass that removes design-time scaffolding from a captured
 * Figma file before the export closure walks it (docs/archive/2026-07-14-design-1-seed.md
 * §6.1). Three channels, machine truth first:
 *
 *   1. sharedPluginData roles — the annotator plugin (../plugin/) writes
 *      `role = placeholder | sample-content | redline | spec` under the
 *      `dashscene` namespace, stamped `v = "1"`
 *      (docs/decisions/annotator-plugin-contract-frozen.md). The REST API
 *      returns them via `?plugin_data=shared`.
 *   2. the `_` name-prefix, sugar for "trim this subtree".
 *   3. slot-child auto-replacement: a `placeholder` node keeps its own box,
 *      but its children are sample content by definition and are trimmed —
 *      at runtime the slot is filled with real content.
 *
 * Trimming is not hiding. A hidden node (`visible: false`) is NOT trimmed: it
 * may be a variant state, and it exports as `visible: false`. Trim reads roles
 * and names only.
 *
 * Every trimmed subtree leaves a named `TrimRecord` (P4: visible in the export
 * report, never a silent drop). Records follow document order, so a given
 * capture trims byte-for-byte the same way (R7).
 *
 * This pass runs before `computeClosure` (import.ts): a trimmed subtree never
 * enters the closure, so its node ids, image refs, and component references are
 * never pulled into the document.
 */

import type { ClosureFile, ClosureNode } from "./closure.ts";
import { rebuildChildren } from "./tree.ts";

/** The four annotator roles, per the frozen contract. */
export type SharedPluginRole =
  | "placeholder"
  | "sample-content"
  | "redline"
  | "spec";

/** sharedPluginData namespace and keys (the frozen contract). */
const NAMESPACE = "dashscene";
const ROLE_KEY = "role";
const VERSION_KEY = "v";
const CONTRACT_VERSION = "1";

const ROLES: ReadonlySet<string> = new Set<SharedPluginRole>([
  "placeholder",
  "sample-content",
  "redline",
  "spec",
]);

/** A closure node as it arrives from a `?plugin_data=shared` capture. */
type AnnotatedNode = ClosureNode & {
  readonly sharedPluginData?: Readonly<
    Record<string, Readonly<Record<string, string>>>
  >;
};

/**
 * Why a subtree was trimmed — one named reason per record. A `role:*` reason
 * is machine truth from the annotator; `name-prefix` is the `_` sugar;
 * `slot-children` is a placeholder's auto-replaced sample content.
 */
export type TrimReason =
  | "role:sample-content"
  | "role:redline"
  | "role:spec"
  | "slot-children"
  | "name-prefix";

/** One trimmed subtree root, named so the export report can list it (P4). */
export interface TrimRecord {
  readonly id: string;
  readonly name: string;
  readonly type: string;
  readonly reason: TrimReason;
}

/** A trim verdict that does not remove a node — named, never silent (P4). */
export interface TrimDiagnostic {
  readonly rule: string;
  /** Trim never blocks an export; every trim diagnostic is a warning. */
  readonly severity: "warning";
  readonly message: string;
  readonly nodeId: string;
}

export interface TrimResult {
  /**
   * The pruned file: trimmed subtrees removed, every kept node verbatim. An
   * untrimmed file is returned by reference, so a file with no annotations
   * serializes identically to the capture.
   */
  readonly file: ClosureFile;
  /** Every trimmed subtree root, in document order. */
  readonly trimmed: readonly TrimRecord[];
  readonly diagnostics: readonly TrimDiagnostic[];
}

/** Reads the dashscene role stamped on a node, if any. */
function roleOf(
  node: AnnotatedNode,
): { raw?: string; version?: string } {
  const ns = node.sharedPluginData?.[NAMESPACE];
  return { raw: ns?.[ROLE_KEY], version: ns?.[VERSION_KEY] };
}

/**
 * Removes design-time scaffolding from a captured file.
 *
 * The document node and its canvases are walked but never themselves subject to
 * a role or `_`-prefix rule at the document root; the rules apply from each
 * canvas downward, so a `_`-named page or a role-tagged page trims like any
 * other subtree.
 */
export function trimFile(file: ClosureFile): TrimResult {
  const trimmed: TrimRecord[] = [];
  const diagnostics: TrimDiagnostic[] = [];

  /** Returns the kept node, or null when the whole subtree is trimmed. */
  const visit = (node: AnnotatedNode): ClosureNode | null => {
    const { raw, version } = roleOf(node);
    if (raw !== undefined && version !== CONTRACT_VERSION) {
      diagnostics.push({
        rule: "figma.trim.contract-version",
        severity: "warning",
        message: `node ${node.id} ("${node.name}") carries a dashscene role ` +
          `stamped ${
            version === undefined ? "(no version)" : `"${version}"`
          } ` +
          `— the frozen contract is version "${CONTRACT_VERSION}"`,
        nodeId: node.id,
      });
    }
    const role = raw !== undefined && ROLES.has(raw) ? raw : undefined;
    if (raw !== undefined && role === undefined) {
      diagnostics.push({
        rule: "figma.trim.unknown-role",
        severity: "warning",
        message: `node ${node.id} ("${node.name}") carries an unknown ` +
          `dashscene role "${raw}" — known roles: placeholder, ` +
          `sample-content, redline, spec`,
        nodeId: node.id,
      });
    }

    // Machine truth: a sample-content, redline, or spec node and everything
    // under it leaves the export.
    if (role === "sample-content" || role === "redline" || role === "spec") {
      trimmed.push({
        id: node.id,
        name: node.name,
        type: node.type,
        reason: `role:${role}`,
      });
      return null;
    }

    // A placeholder keeps its own box; its children are sample content by
    // definition and are auto-replaced (removed here; the runtime fills them).
    if (role === "placeholder") {
      for (const child of node.children ?? []) {
        trimmed.push({
          id: child.id,
          name: child.name,
          type: child.type,
          reason: "slot-children",
        });
      }
      return node.children === undefined || node.children.length === 0
        ? node
        : { ...node, children: [] };
    }

    // The `_` prefix is sugar for the same "trim this subtree" intent.
    if (node.name.startsWith("_")) {
      trimmed.push({
        id: node.id,
        name: node.name,
        type: node.type,
        reason: "name-prefix",
      });
      return null;
    }

    // Kept node: recurse, rebuilding only when a descendant changed, so an
    // untouched subtree is returned by reference (R7: verbatim serialization).
    return rebuildChildren<ClosureNode>(
      node,
      (child) => visit(child as AnnotatedNode),
    );
  };

  // The document node itself is never subject to a rule; its canvases and
  // everything below them are. `rebuildChildren` returns `file.document` by
  // reference when nothing trimmed, so an untrimmed file is returned as-is.
  const document = rebuildChildren<ClosureNode>(
    file.document,
    (canvas) => visit(canvas as AnnotatedNode),
  );
  const pruned: ClosureFile = document === file.document
    ? file
    : { ...file, document };

  return { file: pruned, trimmed, diagnostics };
}
