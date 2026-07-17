/**
 * The phase-2 join (story #167,
 * docs/decisions/token-resolution-phase-split.md): sidecar
 * `{ nodeId, property, variableId }` rows joined against the
 * plugin-exported vartable, each variable's value resolved for the
 * node's mode.
 *
 * The mode is the capture's: a subtree pinned with
 * `explicitVariableModes` resolves that collection's pinned mode; an
 * unpinned subtree resolves the collection's `defaultModeId`. A pinned
 * (non-default) mode qualifies the signal name — `size/gap@dark` — so
 * one variable resolved in two modes yields two signals: the document's
 * mode pins are authored intent, and a runtime producer drives each mode
 * context separately (docs/decisions/binding-table-in-the-document.md).
 *
 * The join knows nothing of binding channels: which property paths are
 * bindable is `dashc`'s Figma-aware half (P5,
 * `crates/dashc/src/figma/bindings.rs`). What this module owns is
 * everything about variables and modes — and every row it cannot join is
 * a named diagnostic, never a silent drop (P4).
 */

import type { ClosureFile, ClosureNode } from "./closure.ts";
import type { ResolvedVarsSidecar } from "./tokens.ts";
import {
  type Vartable,
  type VartableCollection,
  vartableStaleness,
} from "./vartable.ts";

/** One joined row, as the compile request carries it (ABI v2). */
export type JoinedBinding =
  & {
    readonly nodeId: string;
    readonly property: string;
    /** The mode-qualified signal name (`size/gap`, `size/gap@dark`). */
    readonly signal: string;
  }
  & (
    | { readonly resolvedType: "FLOAT"; readonly value: number }
    | {
      readonly resolvedType: "COLOR";
      readonly value: {
        readonly r: number;
        readonly g: number;
        readonly b: number;
        readonly a: number;
      };
    }
  );

/** A named join verdict (P4). */
export interface BindingDiagnostic {
  readonly rule: string;
  readonly severity: "error" | "warning";
  readonly message: string;
  readonly nodeId?: string;
}

/** The join's outcome: the rows that joined, and every named verdict. */
export interface JoinResult {
  readonly bindings: readonly JoinedBinding[];
  readonly diagnostics: readonly BindingDiagnostic[];
}

/**
 * A join the importer refuses to compile past: an error-severity verdict
 * means at least one authored binding cannot be carried faithfully, and
 * emitting a `.dsb` without it would be the silent drop P4 forbids —
 * the same posture as `TokensBlocked`.
 */
export class BindingsBlocked extends Error {
  readonly diagnostics: readonly BindingDiagnostic[];

  constructor(diagnostics: readonly BindingDiagnostic[]) {
    super(
      diagnostics
        .map((d) => `${d.severity}[${d.rule}]: ${d.message}`)
        .join("\n"),
    );
    this.name = "BindingsBlocked";
    this.diagnostics = diagnostics;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** The per-collection mode each node resolves in, walked once. */
function effectiveModes(
  file: ClosureFile,
): Map<string, Readonly<Record<string, string>>> {
  const modes = new Map<string, Readonly<Record<string, string>>>();
  const walk = (
    node: ClosureNode,
    inherited: Readonly<Record<string, string>>,
  ): void => {
    const pinned = (node as unknown as Record<string, unknown>)
      .explicitVariableModes;
    const effective = isRecord(pinned)
      ? { ...inherited, ...(pinned as Record<string, string>) }
      : inherited;
    modes.set(node.id, effective);
    for (const child of node.children ?? []) walk(child, effective);
  };
  walk(file.document, {});
  return modes;
}

/**
 * Joins the phase-1 sidecar against the vartable, resolving each
 * variable's value for its node's mode.
 *
 * Named verdicts (P4), all error severity — a binding the designer
 * authored either joins whole or blocks the export:
 *
 * - `figma.vartable.version-mismatch` — the vartable was exported from a
 *   different file version than the sidecar (the staleness guard).
 * - `figma.bindings.unknown-variable` — a sidecar id the vartable does
 *   not carry (the join is not total).
 * - `figma.bindings.no-mode-value` — the resolved mode has no value for
 *   the variable.
 * - `figma.bindings.alias-value` — the mode's value is itself a variable
 *   alias; alias chains are not resolved this slice.
 * - `figma.bindings.value-shape` — a FLOAT value that is not a number,
 *   or a COLOR value that is not an `{r,g,b,a}` object.
 * - `figma.bindings.ambiguous-signal` — two variables (or one variable
 *   in two collections' name spaces) yield one signal name.
 *
 * One warning: `figma.bindings.unsupported-type` — a STRING or BOOLEAN
 * variable. Text, variant, and visibility bindings are later slices; the
 * site is named and the resolved literal still ships.
 */
export function joinBindings(
  sidecar: ResolvedVarsSidecar,
  vartable: Vartable,
  file: ClosureFile,
): JoinResult {
  const diagnostics: BindingDiagnostic[] = [];
  const bindings: JoinedBinding[] = [];

  const stale = vartableStaleness(vartable, sidecar.version);
  if (stale !== null) {
    return { bindings, diagnostics: [stale] };
  }

  const modes = effectiveModes(file);
  /** signal name → the variable id + mode that first claimed it. */
  const claimed = new Map<string, string>();

  const fail = (rule: string, message: string, nodeId: string): void => {
    diagnostics.push({ rule, severity: "error", message, nodeId });
  };

  for (const row of sidecar.bindings) {
    const variable = vartable.variables[row.variableId];
    if (variable === undefined) {
      fail(
        "figma.bindings.unknown-variable",
        `${row.property}: variable ${row.variableId} has no vartable entry — ` +
          `the join is not total; re-run the annotator's token export`,
        row.nodeId,
      );
      continue;
    }
    if (
      variable.resolvedType !== "FLOAT" && variable.resolvedType !== "COLOR"
    ) {
      diagnostics.push({
        rule: "figma.bindings.unsupported-type",
        severity: "warning",
        message: `${row.property}: variable "${variable.name}" is ` +
          `${variable.resolvedType}; only FLOAT and COLOR bindings are ` +
          `carried this slice — the resolved literal still ships`,
        nodeId: row.nodeId,
      });
      continue;
    }

    const collection: VartableCollection | undefined =
      vartable.collections[variable.variableCollectionId];
    if (collection === undefined) {
      fail(
        "figma.bindings.unknown-variable",
        `${row.property}: variable "${variable.name}" references collection ` +
          `${variable.variableCollectionId}, which the vartable does not carry`,
        row.nodeId,
      );
      continue;
    }

    const pinned = modes.get(row.nodeId)?.[collection.id];
    const modeId = pinned ?? collection.defaultModeId;
    const value = variable.valuesByMode[modeId];
    if (value === undefined) {
      fail(
        "figma.bindings.no-mode-value",
        `${row.property}: variable "${variable.name}" has no value for mode ` +
          `${modeId} of collection "${collection.name}"`,
        row.nodeId,
      );
      continue;
    }
    if (isRecord(value) && value.type === "VARIABLE_ALIAS") {
      fail(
        "figma.bindings.alias-value",
        `${row.property}: variable "${variable.name}" resolves to another ` +
          `variable in mode ${modeId}; alias chains are not resolved this slice`,
        row.nodeId,
      );
      continue;
    }

    // A non-default resolved mode qualifies the signal name, so one
    // variable in two mode contexts is two signals.
    const modeName = collection.modes.find((m) => m.modeId === modeId)?.name ??
      modeId;
    const signal = modeId === collection.defaultModeId
      ? variable.name
      : `${variable.name}@${modeName}`;

    const claim = `${row.variableId}@${modeId}`;
    const earlier = claimed.get(signal);
    if (earlier !== undefined && earlier !== claim) {
      fail(
        "figma.bindings.ambiguous-signal",
        `signal name "${signal}" resolves from two different variables or ` +
          `modes (${earlier} and ${claim}); rename one variable`,
        row.nodeId,
      );
      continue;
    }
    claimed.set(signal, claim);

    if (variable.resolvedType === "FLOAT") {
      if (typeof value !== "number") {
        fail(
          "figma.bindings.value-shape",
          `${row.property}: FLOAT variable "${variable.name}" carries a ` +
            `non-number value in mode ${modeId}`,
          row.nodeId,
        );
        continue;
      }
      bindings.push({
        nodeId: row.nodeId,
        property: row.property,
        signal,
        resolvedType: "FLOAT",
        value,
      });
    } else {
      if (
        !isRecord(value) ||
        typeof value.r !== "number" || typeof value.g !== "number" ||
        typeof value.b !== "number" || typeof value.a !== "number"
      ) {
        fail(
          "figma.bindings.value-shape",
          `${row.property}: COLOR variable "${variable.name}" carries a ` +
            `non-color value in mode ${modeId}`,
          row.nodeId,
        );
        continue;
      }
      bindings.push({
        nodeId: row.nodeId,
        property: row.property,
        signal,
        resolvedType: "COLOR",
        value: { r: value.r, g: value.g, b: value.b, a: value.a },
      });
    }
  }

  return { bindings, diagnostics };
}
