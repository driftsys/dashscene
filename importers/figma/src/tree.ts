/**
 * The identity-preserving tree rebuild shared by the trim pass (trim.ts) and
 * the export closure (closure.ts). Both prune a captured Figma node tree and
 * both must serialize an untouched subtree byte-for-byte as it was captured
 * (R7), so both need the same rule: rebuild a node only when a child actually
 * changed, otherwise return it by reference.
 */

/** A node with an optional readonly children array — a captured Figma node. */
export interface HasChildren {
  readonly children?: readonly HasChildren[];
}

/**
 * Rebuilds `node` from a per-child mapping, preserving identity. `mapChild`
 * returns the child unchanged (by reference), a rebuilt child, or `null` to
 * drop it. If nothing changed — every child returned by reference and none
 * dropped — `node` itself is returned, so an untouched subtree is never
 * reallocated and serializes exactly as captured (R7).
 *
 * The spread replaces only `children`, so every other field a captured node
 * carries survives verbatim on a rebuilt node.
 */
export function rebuildChildren<N extends HasChildren>(
  node: N,
  mapChild: (child: N) => N | null,
): N {
  const kids = node.children as readonly N[] | undefined;
  if (kids === undefined || kids.length === 0) return node;
  const kept: N[] = [];
  let changed = false;
  for (const child of kids) {
    const next = mapChild(child);
    if (next === null) {
      changed = true;
      continue;
    }
    if (next !== child) changed = true;
    kept.push(next);
  }
  return changed ? { ...node, children: kept } : node;
}
