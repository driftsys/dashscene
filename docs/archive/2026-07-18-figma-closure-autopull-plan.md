# Figma closure: auto-pull local masters, warn on the unplaceable — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:test-driven-development
> to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a declared-root export reach `dashc` even when its instances
reference component masters that are buried under undeclared top-level nodes
(auto-pull them) or cannot be placed at all (downgrade to a named warning; the
baked instance still renders).

**Architecture:** A Deno-only change to the export closure. `computeClosure`
(`closure.ts`) auto-pulls a buried LOCAL master's definition subtree and lifts
it as a top-level node of the pruned file, and downgrades every unplaceable
LOCAL master to a warning. `resolveRemoteComponents` (`closure.ts`) downgrades
the "no declared library resolves it" remote case to a warning. `import.ts`
surfaces these closure warnings on stderr. No `dashc`/Rust change: `dashc`
already lowers every top-level node and skips COMPONENT/COMPONENT_SET
definitions (`docs/decisions/figma-component-lowering.md`, #242).

**Tech Stack:** Deno + TypeScript. Tests: `@std/assert`, `deno test`. The drift
oracle calls the compiled `dashc.wasm` via `loadDashc()`.

## Global Constraints

- **Deno-only.** Touch `importers/figma/src/closure.ts` and
  `importers/figma/src/import.ts` only. No `dashc`/Rust change.
- **P4 — named, never silent.** Every downgrade is a named warning; every
  excluded top-level node stays named in `excluded`.
- **P1 — closure is structural.** No resolved geometry or results enter the
  document.
- **Drift oracle invariant.** `closure.imageRefs` MUST equal
  `dashc.figmaImageRefs(closure.file)`. Auto-pulled defs become top-level nodes
  → dashc scans them → equal; downgraded (not pulled) masters are in neither →
  equal.
- **`ExportBlocked` fires only on error-severity closure diagnostics** — already
  true at `import.ts:246/264/274`; the downgrades rely on that.
- **Do NOT modify E7 fixtures/goldens.** New synthetic in-repo fixtures only.
- **Commit scopes** (git-std `strict = true` allowlist): `importers`, `docs`.
- **Never `keptTop.add(top.id)` of a buried master's containing frame** — that
  would silently export undeclared content. Pull the definition subtree only.

---

### Task 1: Auto-pull buried LOCAL masters

**Files:**

- Modify: `importers/figma/src/closure.ts` — `computeClosure`: the
  `includeDefinition` buried branch (`~410`), the component-set buried branch
  (`~523`), and the pruned-file construction (`~606-628`).
- Test: `importers/figma/src/closure_test.ts` — rewrite "a component buried in
  an undeclared subtree is a named error" (`~480`); add a set auto-pull +
  transitive + drift-oracle test.

**Interfaces:**

- Consumes: `computeClosure(file, manifest): Closure`, `Closure.file`,
  `Closure.nodeIds`, `Closure.imageRefs`, `Closure.excluded`,
  `Closure.diagnostics`; `loadDashc()` → `dashc.figmaImageRefs(json): string[]`.
- Produces: no signature change. Behavior: a buried LOCAL component/set is
  auto-pulled (walked + lifted top-level), never diagnosed; its containing frame
  is excluded (named), never kept. The rule `figma.closure.buried-component` is
  no longer minted (remote masters never reach the buried check —
  `if (meta.remote) continue` at `~458`).

- [ ] **Step 1: Rewrite the single-component buried test to assert auto-pull**

Replace the body of the existing test `"a component buried in an undeclared
subtree is a named error"` in `closure_test.ts` (keep the fixture, change the
name and assertions):

```typescript
Deno.test("a component buried in an undeclared subtree is auto-pulled and lifted", () => {
  // The component definition is reachable only through a top-level node the
  // manifest does not declare. Its baked instance renders without the master,
  // so the closure pulls JUST the definition subtree and lifts it as a
  // top-level node — it never keeps the burying frame (that would export
  // undeclared content), which stays named in `excluded`.
  const file = {
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [
        {
          id: "0:1",
          name: "Page 1",
          type: "CANVAS",
          children: [
            {
              id: "1:30",
              name: "library-scratch",
              type: "FRAME",
              children: [
                { id: "1:31", name: "chip", type: "COMPONENT" },
              ],
            },
            {
              id: "1:20",
              name: "home",
              type: "FRAME",
              children: [
                {
                  id: "1:21",
                  name: "chip-instance",
                  type: "INSTANCE",
                  componentId: "1:31",
                },
              ],
            },
          ],
        },
      ],
    },
    components: {
      "1:31": { key: "key-chip", remote: false },
    },
  };
  const closure = computeClosure(file, { roots: ["1:20"] });

  // Clean: no diagnostic — the buried local master is pulled, not refused.
  assertEquals(closure.diagnostics, []);
  // The pulled definition ships and is a top-level node of the pruned file,
  // ahead of the declared root; the burying frame is not in the tree.
  assert(closure.nodeIds.has("1:31"));
  const canvases = closure.file.document.children ?? [];
  assertEquals((canvases[0].children ?? []).map((n) => n.id), ["1:31", "1:20"]);
  // The burying frame is excluded by name (P4), never silently exported.
  assertEquals(closure.excluded.map((n) => n.id), ["1:30"]);
});
```

- [ ] **Step 2: Add the set auto-pull + transitive + drift-oracle test**

Append near the other component tests in `closure_test.ts`:

```typescript
Deno.test("a buried component set is auto-pulled, transitively, and the drift oracle holds", async () => {
  // The declared root instances a member of a set buried under one undeclared
  // frame; that set nests an instance of an inner component buried under a
  // SECOND undeclared frame. Auto-pull lifts both definitions top-level, follows
  // the nested instance transitively, keeps neither burying frame, and the
  // closure's refs still equal dashc's — the invariant this feature must not
  // break.
  const file = {
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [
        {
          id: "0:1",
          name: "Page 1",
          type: "CANVAS",
          children: [
            {
              id: "1:30",
              name: "set-scratch",
              type: "FRAME",
              children: [
                {
                  id: "1:11",
                  name: "outer",
                  type: "COMPONENT_SET",
                  children: [
                    {
                      id: "1:2",
                      name: "state=default",
                      type: "COMPONENT",
                      children: [
                        {
                          id: "1:12",
                          name: "inner-instance",
                          type: "INSTANCE",
                          componentId: "1:40",
                        },
                      ],
                    },
                  ],
                },
              ],
            },
            {
              id: "1:50",
              name: "inner-scratch",
              type: "FRAME",
              children: [
                {
                  id: "1:40",
                  name: "inner",
                  type: "COMPONENT",
                  fills: [{ type: "IMAGE", imageRef: "inner-image" }],
                },
              ],
            },
            {
              id: "1:20",
              name: "home",
              type: "FRAME",
              children: [
                {
                  id: "1:21",
                  name: "chip-instance",
                  type: "INSTANCE",
                  componentId: "1:2",
                },
              ],
            },
          ],
        },
      ],
    },
    components: {
      "1:2": { key: "key-default", remote: false, componentSetId: "1:11" },
      "1:40": { key: "key-inner", remote: false },
    },
    componentSets: { "1:11": { key: "key-set" } },
  };

  const closure = computeClosure(file, { roots: ["1:20"] });

  assertEquals(closure.diagnostics, []);
  // Both definitions were walked (the nested instance's target followed
  // transitively), and both are lifted top-level ahead of the root.
  assert(closure.nodeIds.has("1:11"));
  assert(closure.nodeIds.has("1:2"));
  assert(closure.nodeIds.has("1:40"));
  const canvases = closure.file.document.children ?? [];
  assertEquals((canvases[0].children ?? []).map((n) => n.id), [
    "1:11",
    "1:40",
    "1:20",
  ]);
  // Neither burying frame is kept; both are named in `excluded` (P4).
  assertEquals(closure.excluded.map((n) => n.id).sort(), ["1:30", "1:50"]);

  // The drift oracle: the closure's refs equal dashc's own scan across the
  // auto-pulled definitions.
  const dashc = await loadDashc();
  assertEquals(closure.imageRefs, ["inner-image"]);
  assertEquals(
    closure.imageRefs,
    dashc.figmaImageRefs(JSON.stringify(closure.file)),
  );
});
```

- [ ] **Step 3: Run the two tests to verify they fail**

Run:

```bash
cd importers/figma && deno test --allow-net=api.figma.com --allow-read=.,../../corpus/figma-fixtures,../../goldens/dsb,../../goldens/oracle,../../target/wasm32-unknown-unknown/release --allow-env=FIGMA_TOKEN src/closure_test.ts --filter "auto-pulled"
```

Expected: FAIL — the rewritten single-component test fails because the current
code mints `figma.closure.buried-component` (diagnostics is non-empty); the set
test fails on the same buried error and on the excluded/tree assertions.

- [ ] **Step 4: Implement auto-pull — collect a `pulled` list**

In `computeClosure`, after the `variantSets`/`components` declarations (near the
`resolvedSets` declaration, `~404`), add the pulled-definitions accumulator:

```typescript
// Buried LOCAL masters the walk auto-pulled: their instance renders from
// baked children, but their definition subtree is lifted as a top-level node
// so image_refs/the variant table still see it. NEVER the burying frame —
// that would export undeclared content (P4).
const pulled: ClosureNode[] = [];
```

- [ ] **Step 5: Implement auto-pull — `includeDefinition` buried branch**

Replace the buried error branch inside `includeDefinition` (`~410-420`):

```typescript
if (top.id !== node.id && !keptTop.has(top.id)) {
  // Buried under an undeclared top-level node: auto-pull JUST this
  // definition subtree (walk records its ids/refs and queues its nested
  // instances) and lift it top-level in the pruned file. The burying frame
  // is never kept.
  walk(node);
  pulled.push(node);
  return;
}
```

- [ ] **Step 6: Implement auto-pull — component-set buried branch**

Replace the buried error branch of the set inclusion (`~521-537`):

```typescript
if (!nodeIds.has(setNode.id)) {
  const top = index.topOf.get(setId) as ClosureNode;
  if (top.id !== setId && !keptTop.has(top.id)) {
    // Buried under an undeclared top-level node: auto-pull the set subtree
    // (frozen narrowing still applies inside walk) and lift it top-level;
    // the burying frame is never kept.
    walk(setNode);
    pulled.push(setNode);
  } else {
    keptTop.add(top.id);
    walk(setNode);
  }
}
```

- [ ] **Step 7: Implement auto-pull — splice pulled defs into the pruned file**

After `keptCanvases` is built (`~623`) and before `const pruned` (`~625`), lift
the pulled definitions into the leading canvas (mirrors
`resolveRemoteComponents`' splice, simpler because local ids need no re-id;
`narrowTree` applies any frozen narrowing to a pulled set):

```typescript
// Lift each auto-pulled definition as a top-level node, ahead of the leading
// canvas's own children (the first declared root's canvas leads). dashc lowers
// every top-level node and skips COMPONENT/COMPONENT_SET definitions (#242),
// so which canvas holds a definition does not change what paints.
if (pulled.length > 0 && keptCanvases.length > 0) {
  const lifted = pulled.map(narrowTree);
  keptCanvases[0] = {
    ...keptCanvases[0],
    children: [...lifted, ...(keptCanvases[0].children ?? [])],
  };
}
```

- [ ] **Step 8: Run the two tests to verify they pass**

Run:

```bash
cd importers/figma && deno test --allow-net=api.figma.com --allow-read=.,../../corpus/figma-fixtures,../../goldens/dsb,../../goldens/oracle,../../target/wasm32-unknown-unknown/release --allow-env=FIGMA_TOKEN src/closure_test.ts --filter "auto-pulled"
```

Expected: PASS (both).

- [ ] **Step 9: Run the whole closure suite (regression)**

Run:

```bash
cd importers/figma && deno test --allow-net=api.figma.com --allow-read=.,../../corpus/figma-fixtures,../../goldens/dsb,../../goldens/oracle,../../target/wasm32-unknown-unknown/release --allow-env=FIGMA_TOKEN src/closure_test.ts
```

Expected: PASS — the top-level-set and nested-set tests still pass (those sets
are not buried; the else branch preserves current behavior).

- [ ] **Step 10: Commit**

```bash
git add importers/figma/src/closure.ts importers/figma/src/closure_test.ts
git commit -m "feat(importers): auto-pull buried local component masters into the closure"
```

---

### Task 2: Downgrade unplaceable LOCAL masters to a named warning

**Files:**

- Modify: `importers/figma/src/closure.ts` — the three local-unresolved sites in
  `computeClosure`: `meta === undefined` (`~433`), local single component absent
  from the tree (`~464`), component set absent from the tree (`~495`).
- Test: `importers/figma/src/closure_test.ts` — rewrite "an unresolved
  componentId is a named error" (`~441`) and add two absent-master cases.

**Interfaces:**

- Consumes: `computeClosure`, `Closure.diagnostics`.
- Produces: the rule `figma.closure.local-master-unplaceable` at severity
  `"warning"` for a referenced LOCAL master that is not in the tree; the
  instance renders from its baked children. `figma.closure.unresolved-component`
  is no longer minted.

- [ ] **Step 1: Rewrite the unresolved-componentId test to a warning**

Replace the body of `"an unresolved componentId is a named error"`:

```typescript
Deno.test("a componentId absent from the components map is a named warning", () => {
  const file = componentFile();
  // An instance pointing at a component the file does not carry.
  const canvas = file.document.children[0];
  const home = canvas.children[1] as {
    children: Array<{ componentId?: string }>;
  };
  home.children[0].componentId = "9:9";

  const closure = computeClosure(file, { roots: ["1:20"] });

  // Downgraded, not blocked: the baked instance renders without the master.
  assertEquals(
    closure.diagnostics.map((d) => [d.rule, d.severity]),
    [["figma.closure.local-master-unplaceable", "warning"]],
  );
  assert(closure.diagnostics[0].message.includes("9:9"));
});
```

- [ ] **Step 2: Add the "local master absent from the tree" case**

```typescript
Deno.test("a local master in the map but absent from the tree is a named warning", () => {
  // The components map carries the id, but the definition node is not in the
  // document tree (e.g. trim removed it). The instance still renders from its
  // baked children, so the missing master is a warning, not a block.
  const file = {
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [
        {
          id: "0:1",
          name: "Page 1",
          type: "CANVAS",
          children: [
            {
              id: "1:20",
              name: "home",
              type: "FRAME",
              children: [
                {
                  id: "1:21",
                  name: "chip-instance",
                  type: "INSTANCE",
                  componentId: "1:31",
                },
              ],
            },
          ],
        },
      ],
    },
    components: { "1:31": { key: "key-chip", remote: false } },
  };

  const closure = computeClosure(file, { roots: ["1:20"] });

  assertEquals(
    closure.diagnostics.map((d) => [d.rule, d.severity]),
    [["figma.closure.local-master-unplaceable", "warning"]],
  );
  assert(closure.diagnostics[0].message.includes("1:31"));
});
```

- [ ] **Step 3: Add the "component set absent from the tree" case**

```typescript
Deno.test("a component set absent from the tree is a named warning", () => {
  // The member's set id is not a node in the tree. Same rule: the baked
  // instance renders, the set is a named warning.
  const file = {
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [
        {
          id: "0:1",
          name: "Page 1",
          type: "CANVAS",
          children: [
            {
              id: "1:20",
              name: "home",
              type: "FRAME",
              children: [
                {
                  id: "1:21",
                  name: "chip-instance",
                  type: "INSTANCE",
                  componentId: "1:2",
                },
              ],
            },
          ],
        },
      ],
    },
    components: {
      "1:2": { key: "key-collapsed", remote: false, componentSetId: "1:11" },
    },
    componentSets: { "1:11": { key: "key-set" } },
  };

  const closure = computeClosure(file, { roots: ["1:20"] });

  assertEquals(
    closure.diagnostics.map((d) => [d.rule, d.severity]),
    [["figma.closure.local-master-unplaceable", "warning"]],
  );
  assert(closure.diagnostics[0].message.includes("1:11"));
});
```

- [ ] **Step 4: Run the three tests to verify they fail**

Run:

```bash
cd importers/figma && deno test --allow-net=api.figma.com --allow-read=.,../../corpus/figma-fixtures,../../goldens/dsb,../../goldens/oracle,../../target/wasm32-unknown-unknown/release --allow-env=FIGMA_TOKEN src/closure_test.ts --filter "named warning"
```

Expected: FAIL — the current code mints `figma.closure.unresolved-component` at
severity `error`.

- [ ] **Step 5: Downgrade the `meta === undefined` site**

Replace the diagnostic pushed when `meta === undefined` (`~434-440`):

```typescript
if (meta === undefined) {
  diagnostics.push({
    rule: "figma.closure.local-master-unplaceable",
    severity: "warning",
    message: `an instance references component ${componentId}, which the ` +
      `file's components map does not carry — the instance renders from ` +
      `its baked children; the missing master is not shipped`,
    nodeId: componentId,
  });
  continue;
}
```

- [ ] **Step 6: Downgrade the local-single-component-absent site**

Replace the diagnostic when the local single component's node is absent
(`~464-471`):

```typescript
if (node === undefined) {
  diagnostics.push({
    rule: "figma.closure.local-master-unplaceable",
    severity: "warning",
    message: `component ${componentId} (key ${meta.key}) is in the ` +
      `components map but not in the document tree — the instance ` +
      `renders from its baked children; the missing master is not shipped`,
    nodeId: componentId,
  });
  continue;
}
```

- [ ] **Step 7: Downgrade the set-absent site**

Replace the diagnostic when the set node is absent (`~496-503`):

```typescript
if (setNode === undefined) {
  diagnostics.push({
    rule: "figma.closure.local-master-unplaceable",
    severity: "warning",
    message: `component ${componentId} belongs to component set ${setId}, ` +
      `which is not in the document tree — the instance renders from its ` +
      `baked children; the missing master is not shipped`,
    nodeId: setId,
  });
  continue;
}
```

- [ ] **Step 8: Run the three tests to verify they pass**

Run:

```bash
cd importers/figma && deno test --allow-net=api.figma.com --allow-read=.,../../corpus/figma-fixtures,../../goldens/dsb,../../goldens/oracle,../../target/wasm32-unknown-unknown/release --allow-env=FIGMA_TOKEN src/closure_test.ts --filter "named warning"
```

Expected: PASS (all three).

- [ ] **Step 9: Commit**

```bash
git add importers/figma/src/closure.ts importers/figma/src/closure_test.ts
git commit -m "feat(importers): downgrade an unplaceable local master to a named warning"
```

---

### Task 3: Downgrade the "no library resolves it" remote case to a warning

**Files:**

- Modify: `importers/figma/src/closure.ts` — `resolveRemoteComponents`, the
  `found === undefined` branch (`~854-864`). Update the doc comments at `~92`
  and `~803-807`.
- Test: `importers/figma/src/closure_test.ts` — rewrite "a remote key no
  declared library carries is a named error" (`~1158`) and "a remote component
  with no declared library is a named error" (`~1186`).

**Interfaces:**

- Consumes: `resolveRemoteComponents(file, remotes, libraries): RemoteResolution`,
  `RemoteResolution.diagnostics`, `RemoteResolution.resolved`,
  `RemoteResolution.file`.
- Produces: the rule `figma.closure.remote-master-unplaceable` at severity
  `"warning"` when no declared library carries a remote's key; nothing is
  spliced and the remote entry stays `remote: true`. Genuine resolution failures
  (`cross-file-image`, `cross-file-transitive-remote`, a matched library that
  carries no set for a variant) remain `error`.

- [ ] **Step 1: Rewrite the wrong-key-library test to a warning**

Replace the body of `"a remote key no declared library carries is a named
error"`:

```typescript
Deno.test("a remote key no declared library carries is a named warning", () => {
  const consumer = remoteConsumerFile();
  const remotes = remotesOf(consumer, ["1:20"]);

  // A library that carries a different key does not resolve this one.
  const other = chipLibraryFile();
  (other.components as Record<string, { key: string }>)["1:2"].key =
    "key-other";
  (other.components as Record<string, { key: string }>)["1:5"].key =
    "key-other-2";

  const resolution = resolveRemoteComponents(consumer, remotes, [
    { fileKey: "LIBKEY", file: other },
  ]);

  // Downgraded, not blocked: the baked instance renders without the master.
  assertEquals(
    resolution.diagnostics.map((d) => [d.rule, d.severity]),
    [["figma.closure.remote-master-unplaceable", "warning"]],
  );
  const message = resolution.diagnostics[0].message;
  assert(message.includes("key-collapsed"), message);
  assert(message.includes("LIBKEY"), message);
  // Nothing spliced: the remote entry stays remote, unresolved.
  assertEquals(resolution.resolved, []);
  assertEquals(resolution.file.components?.["9:2"]?.remote, true);
});
```

- [ ] **Step 2: Rewrite the no-library test to a warning**

Replace the body of `"a remote component with no declared library is a named
error"`:

```typescript
Deno.test("a remote component with no declared library is a named warning", () => {
  const consumer = remoteConsumerFile();
  const remotes = remotesOf(consumer, ["1:20"]);

  const resolution = resolveRemoteComponents(consumer, remotes, []);

  assertEquals(
    resolution.diagnostics.map((d) => [d.rule, d.severity]),
    [["figma.closure.remote-master-unplaceable", "warning"]],
  );
  const message = resolution.diagnostics[0].message;
  assert(message.includes("key-collapsed"), message);
  // With no library declared, the warning says so.
  assert(message.includes("(none)"), message);
});
```

- [ ] **Step 3: Run the two tests to verify they fail**

Run:

```bash
cd importers/figma && deno test --allow-net=api.figma.com --allow-read=.,../../corpus/figma-fixtures,../../goldens/dsb,../../goldens/oracle,../../target/wasm32-unknown-unknown/release --allow-env=FIGMA_TOKEN src/closure_test.ts --filter "named warning"
```

Expected: FAIL — the current code mints `figma.closure.cross-file-unresolved` at
severity `error`.

- [ ] **Step 4: Downgrade the `found === undefined` branch**

Replace the diagnostic in `resolveRemoteComponents` (`~855-863`):

```typescript
if (found === undefined) {
  // No declared library carries the key: the instance renders from its
  // baked children, so the missing remote master is a named warning, not a
  // block (docs/decisions/figma-component-lowering.md). A declared library
  // that is matched but cannot be fully resolved (a missing set, an image
  // fill, a transitive remote) is still an error below.
  diagnostics.push({
    rule: "figma.closure.remote-master-unplaceable",
    severity: "warning",
    message: `component ${remote.componentId} (key ${remote.key}) is ` +
      `remote and no declared library carries it — the instance renders ` +
      `from its baked children; the missing master is not shipped ` +
      `(declared libraries: ${declaredList})`,
    nodeId: remote.componentId,
  });
  continue;
}
```

- [ ] **Step 5: Update the two stale doc comments**

At `~90-93` (the `libraries` field comment), change the sentence that ends
"…is a named `cross-file-unresolved` error (P4)." to:

```typescript
* remote component the export reaches is a named
* `remote-master-unplaceable` warning and renders from its baked children
* (docs/decisions/figma-component-lowering.md).
```

At `~803-805` (the `resolveRemoteComponents` doc comment), change "a key no
declared library carries is `cross-file-unresolved`;" to:

```text
* a key no declared library carries is a `remote-master-unplaceable` warning
* (the instance renders from its baked children);
```

- [ ] **Step 6: Run the two tests to verify they pass**

Run:

```bash
cd importers/figma && deno test --allow-net=api.figma.com --allow-read=.,../../corpus/figma-fixtures,../../goldens/dsb,../../goldens/oracle,../../target/wasm32-unknown-unknown/release --allow-env=FIGMA_TOKEN src/closure_test.ts --filter "named warning"
```

Expected: PASS. Also confirm the shadow, image-fill, and transitive tests still
pass (they exercise a matched library, not the downgraded branch):

```bash
cd importers/figma && deno test --allow-net=api.figma.com --allow-read=.,../../corpus/figma-fixtures,../../goldens/dsb,../../goldens/oracle,../../target/wasm32-unknown-unknown/release --allow-env=FIGMA_TOKEN src/closure_test.ts
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add importers/figma/src/closure.ts importers/figma/src/closure_test.ts
git commit -m "feat(importers): downgrade an unresolved remote master to a named warning"
```

---

### Task 4: Surface closure warnings on the import success path

**Files:**

- Modify: `importers/figma/src/import.ts` — import `type ClosureDiagnostic`;
  collect resolution warnings; add `ImportOk.closureDiagnostics`; print them in
  `runImportCli`.
- Test: `importers/figma/src/import_test.ts` — rewrite "importFigmaFile blocks a
  remote component no declared library carries" (`~581`) into a "renders with a
  warning" test; add a CLI stderr surfacing test.

**Interfaces:**

- Consumes: `importFigmaFile(options): Promise<ImportOk>`, `ImportOk`;
  `runImportCli(argv, deps): Promise<number>`; `ClosureDiagnostic` from
  `closure.ts`.
- Produces: `ImportOk.closureDiagnostics: readonly ClosureDiagnostic[]` — the
  final closure's warnings plus the remote-resolution warnings. `runImportCli`
  prints each as `${severity}[${rule}]: ${message}` on stderr.

- [ ] **Step 1: Rewrite the remote-block import test to render-with-warning**

Replace the body of `"importFigmaFile blocks a remote component no declared
library carries"` in `import_test.ts`:

```typescript
Deno.test("importFigmaFile renders a remote instance from baked children with a warning", async () => {
  // The library declared carries a different key, so the remote does not
  // resolve. It is no longer a block: the baked instance renders and the
  // missing master is a named warning (docs/decisions/figma-component-lowering.md).
  const otherLibrary = JSON.parse(
    Deno.readTextFileSync(
      new URL("lowering-variant-topology.json", CORPUS),
    ),
  );
  otherLibrary.components["1:2"].key = "some-other-component-key";
  otherLibrary.components["1:5"].key = "some-other-component-key-2";
  const consumer = JSON.stringify(remoteConsumerCapture());
  const { fetchFn } = scriptedFiles({
    [FILE_KEY]: consumer,
    [LIBRARY_KEY]: JSON.stringify(otherLibrary),
  });

  const result = await importFigmaFile({
    client: createFigmaClient({ token: "x", fetchFn }),
    dashc,
    fileKey: FILE_KEY,
    profile: "core",
    manifest: { roots: ["1:12"], libraries: [LIBRARY_KEY] },
    fetchFn,
  });

  // The remote master is named as unplaceable (P4), at warning severity.
  const warnings = result.closureDiagnostics.filter(
    (d) => d.rule === "figma.closure.remote-master-unplaceable",
  );
  assert(warnings.length > 0, "the unresolved remote is named");
  assert(warnings.every((d) => d.severity === "warning"));
  // The instance renders from its baked subtree, so the bytes still match the
  // single-file local-component golden (the master never painted anyway).
  assertEquals(result.bytes, Deno.readFileSync(VARIANT_GOLDEN));
});
```

- [ ] **Step 2: Add a CLI stderr surfacing test**

Append to `import_test.ts` (near the other CLI tests). It reuses the buried
local master path so no library plumbing is needed:

```typescript
Deno.test("the CLI names a downgraded closure warning on stderr (success path)", async () => {
  // A declared root instances a local master absent from the tree: the export
  // succeeds (the baked instance renders) and the warning is surfaced on
  // stderr, never dropped (P4).
  const file = JSON.stringify({
    document: {
      id: "0:0",
      name: "Document",
      type: "DOCUMENT",
      children: [{
        id: "0:1",
        name: "Page 1",
        type: "CANVAS",
        children: [{
          id: "1:20",
          name: "home",
          type: "FRAME",
          children: [{
            id: "1:21",
            name: "chip-instance",
            type: "INSTANCE",
            componentId: "1:31",
          }],
        }],
      }],
    },
    components: { "1:31": { key: "key-chip", remote: false } },
    version: "v1",
  });
  const { fetchFn } = scriptedFetch(file, new Uint8Array());
  const { deps, err } = cliDeps(fetchFn);

  const code = await runImportCli(
    [FILE_KEY, "--root", "1:20", "-o", "out.dsb"],
    deps,
  );

  assertEquals(code, 0);
  assert(
    err.some((line) =>
      line.startsWith("warning[figma.closure.local-master-unplaceable]:") &&
      line.includes("1:31")
    ),
    err.join(" | "),
  );
});
```

- [ ] **Step 3: Run the two tests to verify they fail**

Run:

```bash
cd importers/figma && deno test --allow-net=api.figma.com --allow-read=.,../../corpus/figma-fixtures,../../goldens/dsb,../../goldens/oracle,../../target/wasm32-unknown-unknown/release --allow-env=FIGMA_TOKEN src/import_test.ts --filter "warning"
```

Expected: FAIL — `result.closureDiagnostics` does not exist (type error), and
the CLI prints no such line.

- [ ] **Step 4: Import the `ClosureDiagnostic` type**

In `import.ts`, add `type ClosureDiagnostic` to the closure.ts import block
(the `import { ... } from "./closure.ts"` at `~37-46`):

```typescript
import {
  type ClosureDiagnostic,
  computeClosure,
  excludeTopLevelNodes,
  exportableRoots,
  ExportBlocked,
  type ExportManifest,
  parseExportManifest,
  type ResolvedLibrary,
  resolveRemoteComponents,
} from "./closure.ts";
```

- [ ] **Step 5: Add the `closureDiagnostics` field to `ImportOk`**

In the `ImportOk` interface, after `bindingDiagnostics` (`~128`):

```typescript
/**
 * The closure's non-blocking verdicts: a local master absent from the tree,
 * or a remote master no declared library resolves — the instance renders from
 * its baked children, the missing master is a named warning (P4,
 * docs/decisions/figma-component-lowering.md). Empty for a clean closure.
 */
readonly closureDiagnostics: readonly ClosureDiagnostic[];
```

- [ ] **Step 6: Collect the remote-resolution warnings**

In `importFigmaFile`, change the remote-resolution block (`~258-269`) to keep
the resolution's warnings (errors already threw):

```typescript
let sourceFile = trimmedFile;
let splicedRootIds: readonly string[] = [];
const remoteDiagnostics: ClosureDiagnostic[] = [];
const remotes = discovery.components.filter((c) => c.remote);
if (remotes.length > 0) {
  const libraries = await fetchLibraries(client, manifest.libraries ?? []);
  const resolution = resolveRemoteComponents(trimmedFile, remotes, libraries);
  if (resolution.diagnostics.some((d) => d.severity === "error")) {
    throw withTrim(new ExportBlocked(resolution.diagnostics), trimContext);
  }
  sourceFile = resolution.file;
  splicedRootIds = resolution.splicedRootIds;
  // Warnings survive the error gate above: an unplaceable remote master, or a
  // shadowed library key. Surfaced with the final closure's warnings below.
  remoteDiagnostics.push(...resolution.diagnostics);
}
```

- [ ] **Step 7: Return the combined closure warnings**

In the `return` of `importFigmaFile` (`~344-351`), add the field (the final
closure's diagnostics are warnings — errors threw at `~274`):

```typescript
return {
  ...compiled,
  excluded: closure.excluded,
  trimmed,
  trimDiagnostics,
  sidecar,
  bindingDiagnostics,
  closureDiagnostics: [...closure.diagnostics, ...remoteDiagnostics],
};
```

- [ ] **Step 8: Print the warnings in `runImportCli`**

In `runImportCli`, after the `bindingDiagnostics` print loop and before the
`result.diagnostics` (dashc) loop (`~526-531`), add:

```typescript
for (const diagnostic of result.closureDiagnostics) {
  deps.error(
    `${diagnostic.severity}[${diagnostic.rule}]: ${diagnostic.message}`,
  );
}
```

- [ ] **Step 9: Run the two tests to verify they pass**

Run:

```bash
cd importers/figma && deno test --allow-net=api.figma.com --allow-read=.,../../corpus/figma-fixtures,../../goldens/dsb,../../goldens/oracle,../../target/wasm32-unknown-unknown/release --allow-env=FIGMA_TOKEN src/import_test.ts --filter "warning"
```

Expected: PASS (both).

- [ ] **Step 10: Run the full importer suite + check + lint**

Run:

```bash
cd importers/figma && deno task check && deno task test && deno task lint
```

Expected: all green.

- [ ] **Step 11: Commit**

```bash
git add importers/figma/src/import.ts importers/figma/src/import_test.ts
git commit -m "feat(importers): surface downgraded closure warnings on the import path"
```

---

### Task 5: Record the decision

**Files:**

- Modify: `docs/decisions/figma-component-lowering.md` — add a new section for
  this decision and extend the Trace.

**Interfaces:** none (documentation).

- [ ] **Step 1: Add the decision section**

Append a section after "Choice" (before "Consequences", or as a new numbered
choice) covering:

- **A baked instance renders without its master.** Restate the REST-bakes-the-
  subtree fact (already in this record) as the license for both behaviors.
- **Auto-pull a buried LOCAL master.** When a declared root's instance
  references a local master buried under an undeclared top-level node, the
  closure walks just that definition subtree and lifts it as a top-level node of
  the pruned file (relying on `pendingComponents` for transitivity); it never
  keeps the burying frame (that would export undeclared content). This keeps the
  drift oracle exact — a lifted definition is a top-level node dashc scans.
- **Downgrade an unplaceable master to a named warning** — both the local cases
  (`figma.closure.local-master-unplaceable`) and the remote "no declared library
  resolves it" case (`figma.closure.remote-master-unplaceable`). A genuine
  remote-resolution failure (a matched library that carries no set, an image
  fill, a transitive remote) stays an error.
- **The omission-vs-approximation line.** Baked children are Figma's own
  resolved content, not an approximation; the master is needed only to validate
  the reference and to ship the variant set for `image_refs`/the v0.4 switcher.
  This is the closure-stage sibling of the S0 partial-emit rule: skip-and-
  diagnose, never approximate.
- **Deferred:** proper remote-library resolution (#259/#261) for variant
  switching and complete library fidelity is still valuable, but not needed to
  render the authored state.

- [ ] **Step 2: Extend the Trace**

Add to the `Verified by:` line: `importers/figma/src/closure_test.ts`
(auto-pull, local-master-unplaceable, remote-master-unplaceable, drift oracle
across an auto-pull) and `importers/figma/src/import_test.ts` (a remote instance
renders from baked children with a warning; the CLI surfaces the warning).

- [ ] **Step 3: Format and verify the docs**

Run:

```bash
just fmt && just lint
```

Expected: green (or at minimum `dprint`/`markdownlint` clean for the edited
file).

- [ ] **Step 4: Commit**

```bash
git add docs/decisions/figma-component-lowering.md
git commit -m "docs(docs): record baked-instance auto-pull and unplaceable-master downgrade"
```

---

## Self-Review

**Spec coverage:**

- Auto-pull buried local masters → Task 1. ✓
- Downgrade local-unresolved (432/462/495) → Task 2. ✓
- Downgrade remote cross-file-unresolved → Task 3. ✓
- `ExportBlocked` only on error-severity → already true (`import.ts:246/264/274`);
  the downgrades rely on it; surfacing on stderr → Task 4. ✓
- Extend `docs/decisions/figma-component-lowering.md` → Task 5. ✓
- Drift-oracle case across an auto-pull → Task 1, Step 2. ✓
- Regression (single-root file unchanged) → existing closure_test cases run in
  Task 1, Step 9; the top-level-set/nested-set tests are unchanged. ✓
- E7 untouched → no fixture/golden edits; all new cases are synthetic. ✓

**Empirical re-probe** (not a code task; run after Task 5): rebuild wasm, run the
hero import live, confirm the closure passes and capture the new dashc-level
frontier. Clean up `.probe.dsb`/`.probe.vars.json`; never commit them.

**Type consistency:** `closureDiagnostics: readonly ClosureDiagnostic[]` is
defined in Task 4 (ImportOk) and consumed in the same task's CLI loop and tests.
The rule names `figma.closure.local-master-unplaceable` (Task 2) and
`figma.closure.remote-master-unplaceable` (Task 3) are used consistently in
their tests and impl. `pulled: ClosureNode[]` (Task 1) is local to
`computeClosure`.
