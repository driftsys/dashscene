# Partial-emit mode — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a partial-emit policy to `dashc`'s Figma front end so an unsupported node is skipped with a warning and the document still emits, with a strict (all-or-nothing) mode kept for "correct or refused."

**Architecture:** A new `EmitPolicy { Strict, Partial }` threads from the ABI request into the Figma walk. It changes exactly one thing — the severity of the `figma.unsupported` omission diagnostic (`Warning` under Partial, `Error` under Strict). The node/subtree is omitted either way, so the existing emit gate (`if report.has_errors()`) passes under Partial with the gaps riding back as warnings. Approximation-if-shipped diagnostics (REJECT-band triaged constructs) and structural errors stay fatal in both modes. No `.dsb` schema change, no new IR node.

**Tech Stack:** Rust (crate `dashc`, published as `dashc_wasm`), the wasm ABI wire format, Deno/TypeScript importer.

## Global Constraints

- Rust edition 2024, `resolver = "3"`, clippy `-D warnings`, `cargo fmt`.
- P1 — the document carries intent, never results (a skipped node leaves nothing behind: no baked box, no placeholder extent).
- P4 — every gap is a named diagnostic, never a silent drop (partial-emit only changes a diagnostic's severity; nothing becomes silent).
- The Rust API default stays **Strict** (zero churn to existing call sites/tests). Only the importer defaults to **Partial**.
- Wire v1 stays compatible: the new request field is appended and its absence decodes as **Strict** (`docs/decisions/dashc-wasm-abi.md`).
- "Never approximate": omission diagnostics (`figma.unsupported`) downgrade under Partial; approximation-if-shipped diagnostics (REJECT-band constructs on a lowered node) and `figma.no-content` do NOT.
- Read the design doc first: `docs/wip/2026-07-18-partial-emit-mode-design.md`.

---

## File structure

- `crates/dashc/src/lib.rs` — define `EmitPolicy`; add `compile_figma_with_bindings_and_policy`; existing `compile_figma`/`compile_figma_with_bindings` delegate with `EmitPolicy::Strict`.
- `crates/dashc/src/figma/mod.rs` — add `policy` to `Walk`; add `lower_with_bindings_and_policy`; `unsupported_at` reads `self.policy` for severity.
- `crates/dashc/src/abi/wire.rs` — `CompileRequest` gains `policy: EmitPolicy`; `decode_compile_request` reads a trailing u32 (absent ⇒ Strict); test encoder appends it.
- `crates/dashc/src/abi/mod.rs` — `compile_figma_response` passes `request.policy`.
- `crates/dashc/tests/figma_lowering.rs` — the emit-policy behavior tests.
- `importers/figma/src/wasm.ts` — `compileFigma` gains a `strict: boolean` arg, writes it as a trailing `u32`.
- `importers/figma/src/import.ts` — parse `--strict` (default Partial), thread it to `compileFigma`.
- `importers/figma/src/import_test.ts` (or the existing import test file) — default-partial / `--strict` behavior.
- `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md` — record the mode.
- `docs/specification/06-dashc-figma-lowering.md` — Compilation §2 + Refusal §1.
- `docs/specification/05-qualification.md` — E4/E7 narrative touch (only if it asserts all-or-nothing).

---

### Task 1: `EmitPolicy` and the Rust threading (Strict unchanged, Partial emits)

**Files:**

- Modify: `crates/dashc/src/lib.rs` (add enum + `_and_policy` entry point)
- Modify: `crates/dashc/src/figma/mod.rs` (`Walk.policy`, `lower_with_bindings_and_policy`, `unsupported_at`)
- Test: `crates/dashc/tests/figma_lowering.rs`

**Interfaces:**

- Produces:
  - `pub enum EmitPolicy { Strict, Partial }` (in `dashc_wasm`, re-exported from crate root).
  - `pub fn compile_figma_with_bindings_and_policy(json: &str, profile: Profile, images: &BTreeMap<String, ImageAsset>, bindings: &[figma::BoundVariable], policy: EmitPolicy) -> Result<(Vec<u8>, Report), CompileError>`
  - `pub fn figma::lower_with_bindings_and_policy(file: &FigmaFile, profile: Profile, images: &BTreeMap<String, ImageAsset>, bindings: &[BoundVariable], policy: EmitPolicy) -> Result<(Document, Vec<Diagnostic>), CompileError>` (mirror the exact current return type of `lower_with_bindings`).
- Consumes: existing `compile_figma`, `compile_figma_with_bindings`, `lower_with_bindings`, `Walk`, `unsupported_at`.

- [ ] **Step 1: Write the failing test — Strict still refuses, Partial emits+warns**

In `crates/dashc/tests/figma_lowering.rs`, add (uses the existing `document_json` helper and `compile_figma_with_bindings_and_policy`; import `EmitPolicy`):

```rust
use dashc_wasm::EmitPolicy;
use dashscene_validator::Severity;

/// A FRAME whose only problem is a VECTOR child: an omission-class gap
/// (`figma.unsupported`, "node type VECTOR"). Strict refuses the file;
/// Partial omits the VECTOR and emits the frame with a warning.
fn frame_with_vector_child() -> serde_json::Value {
    document_json(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "clipsContent": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "children": [{
            "name": "glyph",
            "type": "VECTOR",
            "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 }
        }],
    }))
}

#[test]
fn strict_refuses_a_file_with_an_unsupported_construct() {
    let json = frame_with_vector_child().to_string();
    let images = BTreeMap::new();
    let result =
        compile_figma_with_bindings_and_policy(&json, Profile::Core, &images, &[], EmitPolicy::Strict);
    assert!(matches!(result, Err(CompileError::Diagnostics(_))));
}

#[test]
fn partial_emits_the_frame_and_warns_on_the_skipped_vector() {
    let json = frame_with_vector_child().to_string();
    let images = BTreeMap::new();
    let (bytes, report) =
        compile_figma_with_bindings_and_policy(&json, Profile::Core, &images, &[], EmitPolicy::Partial)
            .expect("partial-emit returns a document");
    assert!(!bytes.is_empty(), "a document is emitted");
    // The gap survives as a WARNING (P4), never dropped.
    let warnings: Vec<_> = report
        .diagnostics()
        .iter()
        .filter(|d| d.rule == dashc_wasm::figma::rule::UNSUPPORTED)
        .collect();
    assert_eq!(warnings.len(), 1, "one figma.unsupported for the VECTOR");
    assert_eq!(warnings[0].severity, Severity::Warning);
    // The frame is present, the VECTOR is omitted: exactly one node.
    let arena = Arena::new();
    let root = load_document(&bytes, &arena).expect("the emitted document loads");
    assert_eq!(root.descendants_including_self().count(), 1);
}
```

Note: if `rule::UNSUPPORTED` is not `pub` at `dashc_wasm::figma::rule`, make the `rule` module `pub` (it is already `pub mod rule` in `figma/mod.rs`). Adjust the `load_document`/arena node-count call to whatever the existing tests use to count nodes (grep this file for `load_document` and `descendants` — mirror the exact API already used here).

- [ ] **Step 2: Run the tests, verify they fail to compile**

Run: `cargo test -p dashc --test figma_lowering strict_refuses partial_emits 2>&1 | tail -20`
Expected: FAIL — `cannot find function compile_figma_with_bindings_and_policy` / `EmitPolicy`.

- [ ] **Step 3: Add `EmitPolicy` and the `_and_policy` entry points**

In `crates/dashc/src/lib.rs`, near the other public items:

```rust
/// How the Figma front end treats a construct the document cannot express.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitPolicy {
    /// All-or-nothing: any vocabulary gap refuses the whole file (R6, the
    /// original `unsupported-figma-constructs-refuse-the-compile.md` posture).
    Strict,
    /// Skip an unsupported node with a warning, still emit. Never approximates:
    /// a construct that could only ship approximately still refuses.
    Partial,
}
```

Refactor the existing `compile_figma_with_bindings` to delegate, and add the policy-taking function:

```rust
pub fn compile_figma_with_bindings(
    json: &str,
    profile: Profile,
    images: &BTreeMap<String, ImageAsset>,
    bindings: &[figma::BoundVariable],
) -> Result<(Vec<u8>, Report), CompileError> {
    compile_figma_with_bindings_and_policy(json, profile, images, bindings, EmitPolicy::Strict)
}

/// [`compile_figma_with_bindings`], choosing the emit policy
/// (`docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`).
pub fn compile_figma_with_bindings_and_policy(
    json: &str,
    profile: Profile,
    images: &BTreeMap<String, ImageAsset>,
    bindings: &[figma::BoundVariable],
    policy: EmitPolicy,
) -> Result<(Vec<u8>, Report), CompileError> {
    let file = figma::parse_file(json)?;
    let (doc, found) =
        figma::lower_with_bindings_and_policy(&file, profile, images, bindings, policy)?;

    let mut report: Report = found.into_iter().collect();

    let (bytes, load_report) = emit_and_validate(&doc);
    report.extend(load_report.diagnostics().iter().cloned());

    if report.has_errors() {
        return Err(CompileError::Diagnostics(report));
    }
    Ok((bytes, report))
}
```

(Keep `compile_figma` as-is — it already delegates to `compile_figma_with_bindings`, which is now Strict.)

- [ ] **Step 4: Thread `policy` into the walk**

In `crates/dashc/src/figma/mod.rs`:

- Refactor `lower_with_bindings` to delegate:

```rust
pub fn lower_with_bindings(
    file: &FigmaFile,
    profile: Profile,
    images: &BTreeMap<String, ImageAsset>,
    bindings: &[BoundVariable],
) -> Result<(Document, Vec<Diagnostic>), CompileError> {
    lower_with_bindings_and_policy(file, profile, images, bindings, crate::EmitPolicy::Strict)
}
```

(Copy the CURRENT signature of `lower_with_bindings` verbatim — match its exact return type; the snippet above assumes `(Document, Vec<Diagnostic>)`, verify against the source.)

- Add `lower_with_bindings_and_policy` with the same body as the current `lower_with_bindings`, plus a `policy: crate::EmitPolicy` parameter that is stored on the `Walk` it constructs.

- Add a `policy: crate::EmitPolicy` field to `struct Walk` and set it wherever `Walk` is constructed.

- In `unsupported_at`, pick the severity from the policy:

```rust
fn unsupported_at(&mut self, index: u32, path: &str, what: String) {
    let severity = match self.policy {
        crate::EmitPolicy::Strict => Severity::Error,
        crate::EmitPolicy::Partial => Severity::Warning,
    };
    self.diagnostics.push(Diagnostic {
        rule: rule::UNSUPPORTED,
        severity,
        at: Location::Node(NodePath::new(index, path)),
        message: format!("{what} is not in the document vocabulary yet"),
    });
}
```

Do NOT change `figma.no-content` minting (stays `Severity::Error`) or the `dashscene_validator::triage(...)` pushes (they keep their band-derived severity). Only `unsupported_at` becomes policy-sensitive.

- [ ] **Step 5: Run the tests, verify they pass**

Run: `cargo test -p dashc --test figma_lowering strict_refuses partial_emits 2>&1 | tail -20`
Expected: PASS (both).

- [ ] **Step 6: Run the whole crate's tests to prove zero churn**

Run: `cargo test -p dashc 2>&1 | tail -20`
Expected: PASS — every existing test still green (they all run at the Strict default).

- [ ] **Step 7: Commit**

```bash
git add crates/dashc/src/lib.rs crates/dashc/src/figma/mod.rs crates/dashc/tests/figma_lowering.rs
git commit -m "feat(dashc): add EmitPolicy and partial-emit for figma.unsupported gaps"
```

---

### Task 2: The never-approximate and structural pins under Partial

**Files:**

- Test: `crates/dashc/tests/figma_lowering.rs`

**Interfaces:**

- Consumes: `compile_figma_with_bindings_and_policy`, `EmitPolicy` (Task 1).

- [ ] **Step 1: Write the failing test — a REJECT-band construct still refuses under Partial**

```rust
/// A noise effect is REJECT-band: shipping the node without it would be an
/// approximation, so Partial must still refuse — the never-approximate line.
fn frame_with_noise_effect() -> serde_json::Value {
    document_json(serde_json::json!({
        "name": "root",
        "type": "FRAME",
        "clipsContent": true,
        "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0 },
        "effects": [{ "type": "NOISE", "visible": true }],
    }))
}

#[test]
fn partial_still_refuses_a_reject_band_construct() {
    let json = frame_with_noise_effect().to_string();
    let images = BTreeMap::new();
    let result =
        compile_figma_with_bindings_and_policy(&json, Profile::Core, &images, &[], EmitPolicy::Partial);
    assert!(
        matches!(result, Err(CompileError::Diagnostics(_))),
        "a REJECT-band construct is never shipped approximated, even under Partial",
    );
}
```

- [ ] **Step 2: Write the failing test — a zero-node file still refuses under Partial**

```rust
/// A canvas holding only a COMPONENT resolves to no paintable content:
/// figma.no-content, a zero-node .dsb that panics a loader. Always an error.
#[test]
fn partial_still_refuses_a_no_content_file() {
    let json = serde_json::json!({
        "document": {
            "name": "Document", "type": "DOCUMENT",
            "children": [{
                "name": "Page 1", "type": "CANVAS",
                "children": [{ "name": "def", "type": "COMPONENT",
                    "absoluteBoundingBox": { "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 } }],
            }],
        },
    })
    .to_string();
    let images = BTreeMap::new();
    let result =
        compile_figma_with_bindings_and_policy(&json, Profile::Core, &images, &[], EmitPolicy::Partial);
    assert!(matches!(result, Err(CompileError::Diagnostics(_))));
}
```

- [ ] **Step 3: Run the tests, verify they fail or pass as written**

Run: `cargo test -p dashc --test figma_lowering partial_still_refuses 2>&1 | tail -20`
Expected: Both PASS if the Task 1 severity change was correctly scoped to `unsupported_at` only. If either FAILS (e.g. the no-content or noise path went through `unsupported_at` and got downgraded), that is a real defect — fix the minting site so only genuine `figma.unsupported` omissions are policy-sensitive, then re-run. (This is the guard that proves the omission-vs-approximation split.)

- [ ] **Step 4: Commit**

```bash
git add crates/dashc/tests/figma_lowering.rs
git commit -m "test(dashc): pin never-approximate and no-content refusal under Partial"
```

---

### Task 3: The ABI wire `policy` field (wire v1 compatible)

**Files:**

- Modify: `crates/dashc/src/abi/wire.rs` (`CompileRequest`, `decode_compile_request`, test encoder)
- Modify: `crates/dashc/src/abi/mod.rs` (`compile_figma_response`)
- Test: `crates/dashc/src/abi/wire.rs` `#[cfg(test)]`

**Interfaces:**

- Produces: `CompileRequest.policy: EmitPolicy`.
- Consumes: `EmitPolicy` (Task 1), `compile_figma_with_bindings_and_policy` (Task 1).

- [ ] **Step 1: Write the failing test — the trailing flag decodes; absent ⇒ Strict**

In `crates/dashc/src/abi/wire.rs` tests, add two cases mirroring the existing `encode` test helper (extend the helper with a trailing optional `strict: Option<u32>`, appending `to_le_bytes()` only when `Some`, so the "absent" case produces the pre-existing wire bytes):

```rust
#[test]
fn a_request_without_a_policy_flag_decodes_as_strict() {
    // The pre-existing wire shape (no trailing u32) must still decode.
    let bytes = encode(0, "{}", &[], &[], None);
    let request = decode_compile_request(&bytes).expect("decodes");
    assert_eq!(request.policy, crate::EmitPolicy::Strict);
}

#[test]
fn a_trailing_zero_flag_decodes_as_partial() {
    let bytes = encode(0, "{}", &[], &[], Some(0));
    let request = decode_compile_request(&bytes).expect("decodes");
    assert_eq!(request.policy, crate::EmitPolicy::Partial);
    let bytes = encode(0, "{}", &[], &[], Some(1));
    let request = decode_compile_request(&bytes).expect("decodes");
    assert_eq!(request.policy, crate::EmitPolicy::Strict);
}
```

Convention: `strict` flag `1 ⇒ Strict`, `0 ⇒ Partial`, **absent ⇒ Strict** (an old caller refuses-hard).

- [ ] **Step 2: Run, verify it fails**

Run: `cargo test -p dashc --lib abi::wire 2>&1 | tail -20`
Expected: FAIL — `no field policy` / `encode` arity mismatch.

- [ ] **Step 3: Add the field and the tolerant decode**

- Add `pub policy: EmitPolicy` to `CompileRequest`.
- At the end of `decode_compile_request` (after bindings), read a trailing flag only if bytes remain. Use whatever "bytes remaining" check the `reader` exposes (grep the reader impl in this file; if it has no such method, add a small `fn remaining(&self) -> usize`). Map `Some(1) | None ⇒ Strict`, `Some(0) ⇒ Partial`:

```rust
let policy = match reader.optional_u32()? {
    Some(0) => crate::EmitPolicy::Partial,
    _ => crate::EmitPolicy::Strict, // Some(1) or absent
};
// ...
Ok(CompileRequest { profile, json, images, bindings, policy })
```

Implement `optional_u32` (or inline the remaining-bytes check) to return `Ok(None)` at end-of-buffer and `Ok(Some(v))` otherwise. Extend the test `encode` helper to append the flag when `Some`.

- In `crates/dashc/src/abi/mod.rs`, change `compile_figma_response` to call `crate::compile_figma_with_bindings_and_policy(&request.json, request.profile, &request.images, &request.bindings, request.policy)`.

- [ ] **Step 4: Run, verify it passes**

Run: `cargo test -p dashc --lib abi::wire 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/dashc/src/abi/wire.rs crates/dashc/src/abi/mod.rs
git commit -m "feat(dashc): carry the emit policy across the wasm ABI (v1 compatible)"
```

---

### Task 4: The importer defaults to Partial; `--strict` opts out

**Files:**

- Modify: `importers/figma/src/wasm.ts` (`compileFigma` writes the trailing flag)
- Modify: `importers/figma/src/import.ts` (parse `--strict`, thread it)
- Test: the existing import test (grep for `runImportCli` in `importers/figma/src/*_test.ts`)

**Interfaces:**

- Consumes: the wire flag convention from Task 3 (`1 ⇒ Strict`, `0 ⇒ Partial`, trailing u32).

- [ ] **Step 1: Write the failing Deno test — default Partial, `--strict` opts out**

Mirror the existing import test's dependency-injection pattern (a stub `dashc` whose `compileFigma` records its arguments). Add:

```ts
Deno.test("import defaults to partial-emit", async () => {
  const seen: { strict?: boolean } = {};
  const dashc = makeStubDashc((_, __, ___, ____, strict) => {
    seen.strict = strict;
    return { bytes: new Uint8Array([1]), diagnostics: [] };
  });
  await runImportCli(["FILEKEY", "--root", "1:2"], stubDeps({ dashc }));
  assertEquals(seen.strict, false);
});

Deno.test("import --strict opts into all-or-nothing", async () => {
  const seen: { strict?: boolean } = {};
  const dashc = makeStubDashc((_, __, ___, ____, strict) => {
    seen.strict = strict;
    return { bytes: new Uint8Array([1]), diagnostics: [] };
  });
  await runImportCli(["FILEKEY", "--root", "1:2", "--strict"], stubDeps({ dashc }));
  assertEquals(seen.strict, true);
});
```

Adapt `makeStubDashc`/`stubDeps` to the ACTUAL helpers the existing test uses (read the file first — it already stubs the compile boundary; reuse its harness, do not invent a new one). The point is: assert the `strict` value reaching `compileFigma`.

- [ ] **Step 2: Run, verify it fails**

Run: `cd importers/figma && deno task test 2>&1 | tail -20`
Expected: FAIL — `compileFigma` has no `strict` param / `runImportCli` ignores `--strict`.

- [ ] **Step 3: Implement**

- In `importers/figma/src/wasm.ts`, add a `strict: boolean` parameter to `compileFigma` and, after writing the bindings, `writer.u32(strict ? 1 : 0);`.
- In `importers/figma/src/import.ts`:
  - In the arg parser (near the `--root`/`--manifest` handling), detect and remove a boolean `--strict` flag: `const strict = args.includes("--strict"); if (strict) args.splice(args.indexOf("--strict"), 1);`.
  - Thread `strict` (default `false`) through `runImportCli` to the `dashc.compileFigma(...)` call at the current call site.
  - Add `--strict` to the usage string.

- [ ] **Step 4: Run, verify it passes**

Run: `cd importers/figma && deno task test 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Deno lint + fmt**

Run: `cd importers/figma && deno task lint && deno task fmt`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add importers/figma/src/wasm.ts importers/figma/src/import.ts importers/figma/src/*_test.ts
git commit -m "feat(importers): default to partial-emit, add --strict for all-or-nothing"
```

---

### Task 5: Revise the decision record and the spec prose

**Files:**

- Modify: `docs/decisions/unsupported-figma-constructs-refuse-the-compile.md`
- Modify: `docs/specification/06-dashc-figma-lowering.md`
- Modify (only if it asserts all-or-nothing): `docs/specification/05-qualification.md`

- [ ] **Step 1: Revise the decision record**

Add a section (dated 2026-07-18, story S0-impl of the real-file-import epic) recording:

- The verdict is now policy-dependent: Strict keeps "correct or refused"; Partial skips an unsupported node with a **warning** and still emits.
- The omission-vs-approximation line: `figma.unsupported` omissions downgrade to warnings under Partial; REJECT-band approximation-if-shipped constructs and `figma.no-content` stay fatal in both modes.
- The importer defaults to Partial; `--strict` restores all-or-nothing. The Rust API default stays Strict.
- Why this is within R6 (R6 permits a warning) and preserves P1/P4.
- The accepted consequence: a REJECT-band-only file still refuses under Partial (per-construct node-omission is a follow-up).

Keep the historical "Choice: Option 3" and "Revised at #140" sections; append the new revision rather than deleting the record's history (match the record's existing revision style).

- [ ] **Step 2: Update the lowering spec**

In `docs/specification/06-dashc-figma-lowering.md`, update "Compilation" §2 and "Refusal" §1 so the "an error withholds the bytes / the emitted bytes shall be discarded" language is qualified by the emit policy: under Strict an omission-class gap is an error and withholds the bytes; under Partial it is a warning and the bytes ship with the node omitted. Approximation-if-shipped constructs and no-content still withhold the bytes in both modes.

- [ ] **Step 3: Check the qualification narrative**

Read `docs/specification/05-qualification.md` E4 and E7. If either asserts all-or-nothing as the behavior, add one clause noting Strict/Partial. If they only assert "a dirty file produces a report" (E4) or render fidelity (E7), leave them unchanged.

- [ ] **Step 4: Lint the docs**

Run: `dprint check && markdownlint docs/decisions/unsupported-figma-constructs-refuse-the-compile.md docs/specification/06-dashc-figma-lowering.md`
Expected: clean (fix any wrapping/heading issues).

- [ ] **Step 5: Commit**

```bash
git add docs/decisions/unsupported-figma-constructs-refuse-the-compile.md docs/specification/
git commit -m "docs(decisions): record partial-emit mode; qualify the refusal spec"
```

---

## Final verification (whole story)

- [ ] `just build` green (assemble + full check).
- [ ] `cargo test -p dashc` green.
- [ ] `cd importers/figma && deno task check && deno task test && deno task lint` green.
- [ ] Manual empirical check: rebuild wasm, run the first-light probe under Partial and confirm it now EMITS: `cd importers/figma && FIGMA_TOKEN=$(security find-generic-password -a "$USER" -s figma-pat -w) deno task import MRk9I5cYY6yJa8JhljzkBn --root 2411:10795 -o /tmp/first-light.dsb` — expect a written `.dsb` plus warning lines for the skipped text/VECTOR nodes (never echo the token). Under `--strict` the same command must still refuse.

## Self-review notes

- Spec coverage: EmitPolicy (T1) · Partial emit+warn (T1) · never-approximate (T2) · no-content pin (T2) · ABI (T3) · importer default+flag (T4) · decision record + spec (T5). All design sections covered.
- Type consistency: `compile_figma_with_bindings_and_policy` and `lower_with_bindings_and_policy` used identically in T1/T3; `EmitPolicy` variants `Strict`/`Partial` used verbatim throughout; wire flag `1⇒Strict, 0⇒Partial, absent⇒Strict` consistent across T3/T4.
- The one place to verify against source before coding: the exact current return type/signature of `lower_with_bindings` and the `reader`'s remaining-bytes API in `wire.rs`. Both are flagged inline.
