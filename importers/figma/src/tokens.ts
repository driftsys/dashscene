/**
 * Design tokens, phase 1: the resolved-literal sidecar
 * (docs/decisions/token-resolution-phase-split.md).
 *
 * `GET /file` returns a bound property's **resolved literal** in the node
 * itself — a bound fill carries its resolved `color`, a bound `itemSpacing`
 * its resolved number — alongside a `boundVariables` entry naming the
 * variable it came from. The lowering already emits those literals (it reads
 * the plain properties and never looks at `boundVariables`), so a phase-1
 * document is P1-clean: the resolved value is what the frame displays. What
 * the `.dsb` does not carry is the variable identity behind each literal;
 * that is what this sidecar preserves.
 *
 * `deriveVarsSidecar` walks the shipped (closure-pruned) nodes and records
 * every `boundVariables` id it finds — node-level, on each visible paint and
 * its gradient stops, and on effects — keyed by the node and the property path
 * where it sits. The sidecar is a faithful projection of the capture — an
 * R7-safe receipt, byte-reproducible from the same input — not a second
 * source of truth. Phase 2 (id -> name/collection/mode) joins these ids
 * against a plugin-exported table and switches the `.dsb` to token refs; that
 * table producer (the annotator plugin's token-export command) and the
 * `.dsb`-side token refs are out of phase 1's scope.
 *
 * P4: an id that cannot be preserved — a `boundVariables` leaf that is not a
 * variable alias, an alias with no id, or an object/array holding no alias at
 * all — is a named diagnostic, never a silent drop. See {@link
 * deriveVarsSidecar} for exactly what is scanned and why.
 */

import type { ClosureFile } from "./closure.ts";

/**
 * The version of the sidecar shape. A consumer (phase 2, #167) that reads a
 * sidecar checks this before trusting its layout, so the shape can evolve
 * without a silent misread.
 */
export const SIDECAR_CONTRACT = 1;

/** One preserved binding: a variable id and the node property it sits on. */
export interface TokenBinding {
  readonly nodeId: string;
  /**
   * The JSON path of the alias within the node — `itemSpacing`,
   * `rectangleCornerRadii.RECTANGLE_TOP_LEFT_CORNER_RADIUS`, `fills[0].color`
   * (a paint's own colour binding), `fills[0].gradientStops[2].color` (a
   * gradient stop's colour), or `effects[0].color`. A paint's index is its
   * raw Figma position among the **visible** paints the sidecar keeps; see
   * {@link deriveVarsSidecar} for how it lines up with the `.dsb`.
   */
  readonly property: string;
  readonly variableId: string;
}

/** The `<out>.vars.json` sidecar written beside a phase-1 `.dsb`. */
export interface ResolvedVarsSidecar {
  readonly sidecarContract: number;
  /** The Figma file `version` this sidecar was derived from (staleness guard). */
  readonly version: string;
  readonly bindings: readonly TokenBinding[];
}

/** A binding whose id could not be preserved — named, never dropped (P4). */
export interface TokenDiagnostic {
  readonly rule: string;
  readonly severity: "error" | "warning";
  readonly message: string;
  /** The node the verdict points at. */
  readonly nodeId: string;
}

export interface VarsSidecarResult {
  readonly sidecar: ResolvedVarsSidecar;
  readonly diagnostics: readonly TokenDiagnostic[];
}

/** The rule name a phase-1 binding failure carries. */
export const UNRESOLVABLE_BINDING = "figma.tokens.unresolvable-binding";

/**
 * A phase-1 sidecar the importer refuses to emit. R6/P4: an error blocks the
 * document rather than shipping a `.dsb` whose bound intent was silently lost.
 */
export class TokensBlocked extends Error {
  readonly diagnostics: readonly TokenDiagnostic[];

  constructor(diagnostics: readonly TokenDiagnostic[]) {
    super(
      diagnostics
        .map((d) => `${d.severity}[${d.rule}]: ${d.message}`)
        .join("\n"),
    );
    this.name = "TokensBlocked";
    this.diagnostics = diagnostics;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isAlias(value: unknown): value is { type: string; id?: unknown } {
  return isRecord(value) && value.type === "VARIABLE_ALIAS";
}

function describe(value: unknown): string {
  return value === null ? "null" : typeof value;
}

/**
 * Derives the phase-1 sidecar from the closure's pruned file. Deriving from
 * the pruned file — not the raw capture — keeps the sidecar and the `.dsb` in
 * agreement on which nodes exist: a top-level node the export excludes
 * contributes no binding, exactly as its resolved literals are absent from
 * the document.
 *
 * The sidecar's coverage tracks what the lowering emits, so every preserved id
 * has a resolved literal in the `.dsb` to pair with:
 *
 * - Node-level `boundVariables` (itemSpacing, cornerRadii, opacity, …) except
 *   the `fills`/`strokes` array mirror: Figma stores a fill-colour binding both
 *   in `node.boundVariables.fills[i]` and in the paint's own `boundVariables`,
 *   and the array mirror carries no `visible` flag to filter on. Recording only
 *   the paint-level site drops no id and gives one entry per fill.
 * - Each **visible** paint's own `boundVariables` (its `color`) and each of its
 *   gradient stops' `boundVariables`. The lowering resolves only visible paints
 *   and lowers gradient-stop colours into the `.dsb`, so both are scanned; a
 *   `visible: false` paint is not lowered and not recorded. `background` is
 *   Figma's deprecated mirror of `fills`, which the lowering ignores — so does
 *   this.
 * - Each effect's `boundVariables`, preserved ahead of effect lowering: effect
 *   params are triaged, not yet lowered into the `.dsb` (debt #144), so an
 *   effect binding has no literal to pair with yet — it is kept rather than
 *   dropped so no id goes silently unscanned (P4).
 *
 * P4: a `boundVariables` value that yields no alias — a bare literal, an alias
 * with no id, or an object/array with no alias anywhere inside it — is a named
 * `figma.tokens.unresolvable-binding` error, never a silent drop.
 */
export function deriveVarsSidecar(
  file: ClosureFile,
  version: string,
): VarsSidecarResult {
  const bindings: TokenBinding[] = [];
  const diagnostics: TokenDiagnostic[] = [];

  const fail = (message: string, nodeId: string): void => {
    diagnostics.push({
      rule: UNRESOLVABLE_BINDING,
      severity: "error",
      message,
      nodeId,
    });
  };

  const visit = (node: Record<string, unknown>): void => {
    const nodeId = typeof node.id === "string" ? node.id : "";

    /**
     * Records the aliases under one binding value, returning how many leaves
     * it accounted for — an alias recorded, or a diagnostic raised. A record
     * or array that accounts for none of its own contents is itself a named
     * failure, so a shape like `{ opacity: {} }` cannot vanish in silence (P4).
     */
    const extract = (value: unknown, path: string): number => {
      if (isAlias(value)) {
        const id = value.id;
        if (typeof id === "string" && id.length > 0) {
          bindings.push({ nodeId, property: path, variableId: id });
        } else {
          fail(`${path}: a variable alias carries no id`, nodeId);
        }
        return 1;
      }
      if (Array.isArray(value)) {
        let accounted = 0;
        value.forEach((element, index) => {
          accounted += extract(element, `${path}[${index}]`);
        });
        if (accounted === 0) fail(`${path}: yields no variable alias`, nodeId);
        return accounted === 0 ? 1 : accounted;
      }
      if (isRecord(value)) {
        let accounted = 0;
        for (const [key, inner] of Object.entries(value)) {
          accounted += extract(inner, path === "" ? key : `${path}.${key}`);
        }
        if (accounted === 0) fail(`${path}: yields no variable alias`, nodeId);
        return accounted === 0 ? 1 : accounted;
      }
      fail(
        `${path}: expected a variable alias, found ${describe(value)}`,
        nodeId,
      );
      return 1;
    };

    /** Harvests one `boundVariables` map, when the object carries one. */
    const harvest = (
      map: unknown,
      basePath: string,
      skip: ReadonlySet<string> = new Set(),
    ): void => {
      if (map === undefined) return;
      if (!isRecord(map)) {
        fail(
          `${basePath || "boundVariables"}: boundVariables is not a map`,
          nodeId,
        );
        return;
      }
      for (const [key, value] of Object.entries(map)) {
        if (skip.has(key)) continue;
        extract(value, basePath === "" ? key : `${basePath}.${key}`);
      }
    };

    // Node-level bindings, minus the `fills`/`strokes` array mirror (recorded
    // through the visible paints below instead).
    harvest(node.boundVariables, "", new Set(["fills", "strokes"]));

    for (const key of ["fills", "strokes"] as const) {
      const paints = node[key];
      if (!Array.isArray(paints)) continue;
      paints.forEach((paint, index) => {
        // A hidden paint is not lowered, so its binding is not recorded — the
        // sidecar and the `.dsb` keep the same paints.
        if (!isRecord(paint) || paint.visible === false) return;
        const base = `${key}[${index}]`;
        harvest(paint.boundVariables, base);
        const stops = paint.gradientStops;
        if (Array.isArray(stops)) {
          stops.forEach((stop, at) => {
            if (isRecord(stop)) {
              harvest(stop.boundVariables, `${base}.gradientStops[${at}]`);
            }
          });
        }
      });
    }

    const effects = node.effects;
    if (Array.isArray(effects)) {
      effects.forEach((effect, index) => {
        if (isRecord(effect)) {
          harvest(effect.boundVariables, `effects[${index}]`);
        }
      });
    }

    const children = node.children;
    if (Array.isArray(children)) {
      for (const child of children) if (isRecord(child)) visit(child);
    }
  };

  visit(file.document as unknown as Record<string, unknown>);

  return {
    sidecar: { sidecarContract: SIDECAR_CONTRACT, version, bindings },
    diagnostics,
  };
}

/** Serializes a sidecar deterministically — trailing newline, 2-space indent. */
export function formatSidecar(sidecar: ResolvedVarsSidecar): string {
  return JSON.stringify(sidecar, null, 2) + "\n";
}
