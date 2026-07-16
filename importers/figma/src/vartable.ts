/**
 * The token-export vartable: the id -> name/collection/mode table the annotator
 * plugin (../plugin/) produces and phase-2 token resolution / #167 join the
 * phase-1 sidecar against (docs/decisions/token-resolution-phase-split.md).
 *
 * This module is the importer-side load guard. It does not perform the #167
 * join — that is #167's consumer. It parses a committed vartable and refuses a
 * malformed one by name (P4), and it names a staleness mismatch, so a blank or
 * wrong `version` can never slip through unnoticed:
 *
 * - `parseVartable` rejects a blank `version`, a wrong contract, or a missing
 *   `collections`/`variables` map. REST carries no variable names, so the
 *   vartable is the only source of them; a table with no valid version stamp is
 *   unjoinable.
 * - `vartableStaleness` names the mismatch when a vartable was exported from a
 *   different file version than the capture it is paired with.
 */

/** The vartable contract version this loader understands. */
export const VARTABLE_CONTRACT = 1;

export interface VartableMode {
  readonly modeId: string;
  readonly name: string;
}

export interface VartableCollection {
  readonly id: string;
  readonly name: string;
  readonly defaultModeId: string;
  readonly modes: readonly VartableMode[];
}

export interface VartableVariable {
  readonly id: string;
  readonly name: string;
  readonly variableCollectionId: string;
  readonly resolvedType: string;
  readonly valuesByMode: Readonly<Record<string, unknown>>;
}

export interface Vartable {
  readonly vartableContract: number;
  /** The Figma file version the table was exported from — the staleness stamp. */
  readonly version: string;
  readonly fileKey?: string | null;
  readonly collections: Readonly<Record<string, VartableCollection>>;
  readonly variables: Readonly<Record<string, VartableVariable>>;
}

/** A named vartable verdict (P4). */
export interface VartableDiagnostic {
  readonly rule: string;
  readonly severity: "error";
  readonly message: string;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

/**
 * Parses a committed vartable, refusing a malformed one by name.
 *
 * @throws when the JSON is not a vartable object, the contract is not
 * {@link VARTABLE_CONTRACT}, the `version` stamp is blank, or the
 * `collections`/`variables` maps are missing — every one a named refusal, so a
 * table that cannot be joined never loads silently (P4).
 */
export function parseVartable(text: string): Vartable {
  const parsed: unknown = JSON.parse(text);
  if (!isObject(parsed)) {
    throw new Error(
      "figma.vartable.malformed: the vartable is not a JSON object",
    );
  }
  if (parsed.vartableContract !== VARTABLE_CONTRACT) {
    throw new Error(
      `figma.vartable.contract: the vartable contract is ` +
        `${JSON.stringify(parsed.vartableContract)}, this loader reads ` +
        `${VARTABLE_CONTRACT}`,
    );
  }
  if (typeof parsed.version !== "string" || parsed.version.length === 0) {
    throw new Error(
      "figma.vartable.no-version: the vartable has no `version` stamp — it is " +
        "the staleness guard the join checks against the capture, so a blank " +
        "one is refused, never joined stale",
    );
  }
  if (!isObject(parsed.collections) || !isObject(parsed.variables)) {
    throw new Error(
      "figma.vartable.malformed: the vartable needs `collections` and " +
        "`variables` maps",
    );
  }
  return parsed as unknown as Vartable;
}

/**
 * Names a staleness mismatch between a vartable and the capture it is joined
 * with. The vartable is stamped with the file version it was exported from; a
 * capture from a different version means variables may have been renamed,
 * added, or removed since, so the join is unsound — a named error (P4), never a
 * silent bad join. Returns null when the versions agree.
 */
export function vartableStaleness(
  vartable: Vartable,
  captureVersion: string,
): VartableDiagnostic | null {
  if (vartable.version === captureVersion) return null;
  return {
    rule: "figma.vartable.version-mismatch",
    severity: "error",
    message: `the vartable was exported from file version ` +
      `"${vartable.version}", but the capture is version "${captureVersion}" ` +
      `— re-run the annotator's token export against the current file`,
  };
}
