# The dashc wasm ABI + the minimal Deno importer — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `dashc` a callable wasm ABI, and build the Deno importer that
drives it: Figma fixture in, `.dsb` out, byte-identical to what native
`dashc` produces from the same input.

**Architecture:** Five hand-written `extern "C"` exports on the `dashc_wasm`
cdylib, with a length-prefixed binary envelope carrying the byte payloads and
JSON carrying the structured report and error. The framing codec is a pure
function over `&[u8]`, so `cargo test` pins the wire format natively —
calling the real exports, with no wasm runtime in the loop. The Deno side owns
everything `dashc` cannot do: HTTP, auth, and resolving an `imageRef` into
bytes.

**Tech Stack:** Rust (edition 2024, `wasm32-unknown-unknown`, `serde_json`),
Deno 2.x / TypeScript, `@figma/rest-api-spec`.

Design: `docs/wip/2026-07-14-figma-importer-minimal-design.md`. Read it first
— it records why this ABI is hand-written rather than wasm-bindgen, and every
task below assumes that decision.

## Global Constraints

- Rust edition 2024: a `no_mangle` attribute is written `#[unsafe(no_mangle)]`.
- `cargo clippy --workspace --all-targets -- -D warnings` must pass. A public
  `unsafe fn` needs a `/// # Safety` section or clippy fails the build.
- `dashc` does no network and no filesystem I/O, and must keep building for
  `wasm32-unknown-unknown`. CI enforces this.
- `dashscene-validator` and `dashpaint` carry no `serde` and must not gain it.
  The serializable mirrors live in `dashc`, which already depends on `serde`
  and `serde_json`.
- **Nothing in the ABI may panic on bad input.** A Rust panic on
  `wasm32-unknown-unknown` traps and kills the module instance, turning a bad
  request into an unrecoverable importer. Malformed input returns `status: 2`.
- The IR is DSB (`Dsb` in memory, `.dsb` on disk). Never SCD.
- Conventional commits, and the scope must be one of: `dashc`, `importers`,
  `corpus`, `goldens`, `ci`, `repo`, `docs`, `deps` (`.git-std.toml` is
  `strict = true` — an unlisted scope fails the commit hook and CI).
- Markdown must pass `dprint check` and `markdownlint`. TypeScript must pass
  `deno fmt --check` and `deno lint`.

## File structure

| File                                   | Responsibility                                                                 |
| -------------------------------------- | ------------------------------------------------------------------------------ |
| `crates/dashc/src/abi/wire.rs`         | The framing codec: decode a request, encode a response. Pure, no wasm, no I/O. |
| `crates/dashc/src/abi/json.rs`         | `serde` mirrors of `Report` / `Diagnostic` / `Location` / `CompileError`.      |
| `crates/dashc/src/abi/mod.rs`          | The five `extern "C"` shims, and the dispatch each one wraps.                  |
| `crates/dashc/src/figma/mod.rs`        | Gains `image_refs()` — the refs the lowering will demand.                      |
| `crates/dashc/tests/abi.rs`            | Drives the real exports natively: alloc, write, call, decode, free.            |
| `crates/dashc/tests/figma_lowering.rs` | Gains the golden assertion; drops its invented PNG constant.                   |
| `goldens/dsb/v03-paint.dsb`            | The one artifact both languages pin.                                           |
| `importers/figma/src/wasm.ts`          | Module loading, the ABI codec, `compileFigma` / `figmaImageRefs`.              |
| `importers/figma/src/images.ts`        | `imageRef` → bytes: the URL map, the downloads, the PNG check.                 |
| `importers/figma/src/import.ts`        | The five-step orchestration, plus the `deno task import` CLI.                  |
| `importers/figma/src/fetch.ts`         | Gains `imageFills()` on the existing serialized limiter.                       |
| `importers/figma/src/capture.ts`       | Gains image-fill capture, writing bytes into the corpus.                       |

---

### Task 1: The wire codec

**Files:**

- Create: `crates/dashc/src/abi/wire.rs`
- Create: `crates/dashc/src/abi/mod.rs` (module declarations only in this task)
- Modify: `crates/dashc/src/lib.rs` (add `mod abi;`)

**Interfaces:**

- Consumes: `dashpaint::{ImageAsset, ImageFormat}`, `dashscene_validator::Profile`.
- Produces: `wire::ABI_VERSION`, `wire::CompileRequest`,
  `wire::decode_compile_request(&[u8]) -> Result<CompileRequest, String>`,
  `wire::encode_response(status: u32, blob: &[u8], json: &str) -> Vec<u8>`,
  `wire::Status`.

- [ ] **Step 1: Write the failing test**

Append to `crates/dashc/src/abi/wire.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes a request the way the TypeScript codec does
    /// (`importers/figma/src/wasm.ts`). If this helper and that codec ever
    /// disagree, the Deno suite fails on the golden — which is the point.
    fn encode_request(profile: u32, json: &str, images: &[(&str, u32, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&profile.to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(json.as_bytes());
        out.extend_from_slice(&(images.len() as u32).to_le_bytes());
        for (image_ref, format, bytes) in images {
            out.extend_from_slice(&(image_ref.len() as u32).to_le_bytes());
            out.extend_from_slice(image_ref.as_bytes());
            out.extend_from_slice(&format.to_le_bytes());
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        out
    }

    #[test]
    fn a_request_round_trips() {
        let bytes = encode_request(1, "{}", &[("abc", 0, &[1, 2, 3])]);
        let request = decode_compile_request(&bytes).expect("the request decodes");

        assert_eq!(request.profile, Profile::Full);
        assert_eq!(request.json, "{}");
        assert_eq!(request.images.len(), 1);
        let asset = &request.images["abc"];
        assert_eq!(asset.format, ImageFormat::Png);
        assert_eq!(asset.bytes, vec![1, 2, 3]);
    }

    #[test]
    fn a_truncated_request_is_an_error_not_a_panic() {
        let bytes = encode_request(0, "{}", &[("abc", 0, &[1, 2, 3])]);
        for cut in 0..bytes.len() {
            // Every prefix must decode to an error. A panic here would trap
            // the wasm module (see the plan's global constraints).
            assert!(decode_compile_request(&bytes[..cut]).is_err(), "prefix of {cut} bytes");
        }
    }

    #[test]
    fn trailing_bytes_are_an_error() {
        let mut bytes = encode_request(0, "{}", &[]);
        bytes.push(0);
        assert!(decode_compile_request(&bytes).is_err());
    }

    #[test]
    fn an_unknown_profile_is_an_error() {
        let bytes = encode_request(7, "{}", &[]);
        assert!(decode_compile_request(&bytes).is_err());
    }

    #[test]
    fn an_unknown_image_format_is_an_error() {
        let bytes = encode_request(0, "{}", &[("abc", 9, &[1])]);
        assert!(decode_compile_request(&bytes).is_err());
    }

    #[test]
    fn a_response_is_length_prefixed() {
        let response = encode_response(Status::Ok as u32, &[7, 8], "{}");

        let total = u32::from_le_bytes(response[0..4].try_into().unwrap()) as usize;
        assert_eq!(total, response.len() - 4, "the prefix counts everything after itself");
        assert_eq!(u32::from_le_bytes(response[4..8].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(response[8..12].try_into().unwrap()), 2);
        assert_eq!(&response[12..14], &[7, 8]);
        assert_eq!(u32::from_le_bytes(response[14..18].try_into().unwrap()), 2);
        assert_eq!(&response[18..20], b"{}");
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p dashc --lib abi::wire`
Expected: FAIL — `crates/dashc/src/abi/wire.rs` does not exist.

- [ ] **Step 3: Write the codec**

Create `crates/dashc/src/abi/wire.rs` (the test module above stays at the
bottom of the file):

```rust
//! The wasm ABI wire format.
//!
//! Little-endian, `u32` lengths, no padding. The format is deliberately dull:
//! it is read by hand-written code on both sides (see the design note for why
//! this is not wasm-bindgen), so every field is a length followed by exactly
//! that many bytes.
//!
//! Nothing here panics. A wasm trap kills the module instance, so a malformed
//! request has to come back as a value — `Err(String)`, which the caller turns
//! into a `Status::MalformedRequest` response.

use std::collections::BTreeMap;

use dashpaint::{ImageAsset, ImageFormat};
use dashscene_validator::Profile;

/// The version of this wire format. The Deno side reads it at load and
/// refuses a module it does not understand, so a stale `.wasm` fails with a
/// sentence instead of a misdecode.
pub const ABI_VERSION: u32 = 1;

/// The first field of every response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Status {
    Ok = 0,
    CompileError = 1,
    MalformedRequest = 2,
}

/// A decoded `dashc_compile_figma` request.
#[derive(Debug, Clone, PartialEq)]
pub struct CompileRequest {
    pub profile: Profile,
    pub json: String,
    pub images: BTreeMap<String, ImageAsset>,
}

/// A cursor that runs out of bytes instead of panicking.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn u32(&mut self) -> Result<u32, String> {
        let end = self.at.checked_add(4).ok_or("length overflow")?;
        let field = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| format!("want 4 bytes at offset {}, have {}", self.at, self.remaining()))?;
        self.at = end;
        Ok(u32::from_le_bytes(field.try_into().expect("a 4-byte slice is 4 bytes")))
    }

    fn bytes(&mut self) -> Result<&'a [u8], String> {
        let len = self.u32()? as usize;
        let end = self.at.checked_add(len).ok_or("length overflow")?;
        let field = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| format!("want {len} bytes at offset {}, have {}", self.at, self.remaining()))?;
        self.at = end;
        Ok(field)
    }

    fn string(&mut self) -> Result<String, String> {
        let bytes = self.bytes()?;
        String::from_utf8(bytes.to_vec()).map_err(|e| format!("not UTF-8: {e}"))
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.at)
    }

    /// Trailing bytes mean the two sides disagree about the format, which is
    /// exactly the bug this ABI exists to make loud.
    fn finish(&self) -> Result<(), String> {
        match self.remaining() {
            0 => Ok(()),
            n => Err(format!("{n} trailing byte(s) after the request")),
        }
    }
}

pub fn decode_compile_request(bytes: &[u8]) -> Result<CompileRequest, String> {
    let mut reader = Reader::new(bytes);

    let profile = match reader.u32()? {
        0 => Profile::Core,
        1 => Profile::Full,
        other => return Err(format!("unknown profile {other} (0 = core, 1 = full)")),
    };
    let json = reader.string()?;

    let count = reader.u32()?;
    let mut images = BTreeMap::new();
    for _ in 0..count {
        let image_ref = reader.string()?;
        let format = match reader.u32()? {
            0 => ImageFormat::Png,
            other => return Err(format!("unknown image format {other} (0 = png)")),
        };
        let asset = ImageAsset {
            format,
            bytes: reader.bytes()?.to_vec(),
        };
        if images.insert(image_ref.clone(), asset).is_some() {
            return Err(format!("imageRef {image_ref} appears twice"));
        }
    }
    reader.finish()?;

    Ok(CompileRequest {
        profile,
        json,
        images,
    })
}

/// Frames one response: a `u32` byte count, then the envelope.
///
/// The count is what lets the caller free the buffer with a single
/// `dashc_free(ptr, 4 + count)` — see the design note on why the length is a
/// prefix rather than a pointer/length pair packed into a `u64`.
pub fn encode_response(status: u32, blob: &[u8], json: &str) -> Vec<u8> {
    let body_len = 4 + 4 + blob.len() + 4 + json.len();
    let mut out = Vec::with_capacity(4 + body_len);

    out.extend_from_slice(&(body_len as u32).to_le_bytes());
    out.extend_from_slice(&status.to_le_bytes());
    out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    out.extend_from_slice(blob);
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(json.as_bytes());
    out
}
```

Create `crates/dashc/src/abi/mod.rs`:

```rust
//! The wasm ABI: five `extern "C"` exports over a hand-written wire format.

pub mod wire;
```

Add to `crates/dashc/src/lib.rs`, beside the existing `mod dsb;`. It is `pub`
because `tests/abi.rs` calls the exports directly — that native test is what
pins the wire format, so the module cannot be private:

```rust
pub mod abi;
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p dashc --lib abi::wire`
Expected: PASS — 6 tests.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy -p dashc --all-targets -- -D warnings
cargo fmt --all
git add crates/dashc/src/abi crates/dashc/src/lib.rs
git commit -m "feat(dashc): the wasm ABI wire format"
```

---

### Task 2: The JSON mirrors

**Files:**

- Create: `crates/dashc/src/abi/json.rs`
- Modify: `crates/dashc/src/abi/mod.rs`

**Interfaces:**

- Consumes: `dashscene_validator::{Diagnostic, Location, Report, Severity}`,
  `crate::figma::CompileError`.
- Produces: `json::report_json(&Report) -> String`,
  `json::compile_error_json(&CompileError) -> String`,
  `json::image_refs_json(&[String]) -> String`.

- [ ] **Step 1: Write the failing test**

Append to `crates/dashc/src/abi/json.rs`:

```rust
#[cfg(test)]
mod tests {
    use dashscene_validator::{Diagnostic, Location, NodePath, Report, Severity};

    use super::*;

    fn diagnostic(at: Location) -> Diagnostic {
        Diagnostic {
            rule: "paint.effect.noise",
            severity: Severity::Error,
            at,
            message: "noise is not in the v0.3 vocabulary".to_string(),
        }
    }

    #[test]
    fn a_node_location_is_tagged_and_carries_its_path() {
        let report: Report = vec![diagnostic(Location::Node(NodePath::new(3, "/card/badge")))]
            .into_iter()
            .collect();

        assert_eq!(
            report_json(&report),
            r#"{"diagnostics":[{"rule":"paint.effect.noise","severity":"error",
"at":{"kind":"node","index":3,"path":"/card/badge"},
"message":"noise is not in the v0.3 vocabulary"}]}"#
                .replace('\n', "")
        );
    }

    #[test]
    fn a_pool_location_is_tagged_apart_from_a_node() {
        // A paint-pool index and a node index are both integers. The tag is
        // what stops a consumer from resolving one as the other.
        let report: Report = vec![diagnostic(Location::PaintEntry(2))].into_iter().collect();
        assert!(report_json(&report).contains(r#""at":{"kind":"paintEntry","index":2}"#));

        let report: Report = vec![diagnostic(Location::ImageAsset(0))].into_iter().collect();
        assert!(report_json(&report).contains(r#""at":{"kind":"imageAsset","index":0}"#));
    }

    #[test]
    fn every_compile_error_variant_is_tagged() {
        let unsupported = CompileError::Unsupported {
            path: "/card".to_string(),
            what: "an auto-layout frame".to_string(),
        };
        assert_eq!(
            compile_error_json(&unsupported),
            r#"{"kind":"unsupported","path":"/card","what":"an auto-layout frame"}"#
        );

        let unresolved = CompileError::UnresolvedImage {
            path: "/hero".to_string(),
            image_ref: "abc".to_string(),
        };
        assert_eq!(
            compile_error_json(&unresolved),
            r#"{"kind":"unresolvedImage","path":"/hero","imageRef":"abc"}"#
        );

        let parse = CompileError::Parse(serde_json::from_str::<u8>("nope").unwrap_err());
        assert!(compile_error_json(&parse).starts_with(r#"{"kind":"parse","message":"#));

        let report: Report = vec![diagnostic(Location::PaintEntry(0))].into_iter().collect();
        let diagnostics = CompileError::Diagnostics(report);
        assert!(compile_error_json(&diagnostics).starts_with(r#"{"kind":"diagnostics","diagnostics":[{"#));
    }

    #[test]
    fn image_refs_serialize_as_an_array() {
        assert_eq!(image_refs_json(&["a".to_string(), "b".to_string()]), r#"["a","b"]"#);
    }
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p dashc --lib abi::json`
Expected: FAIL — `crates/dashc/src/abi/json.rs` does not exist.

- [ ] **Step 3: Write the mirrors**

Create `crates/dashc/src/abi/json.rs` (the test module above stays at the
bottom):

```rust
//! The JSON half of the ABI: the report and the error.
//!
//! `dashscene-validator` and `dashpaint` carry no `serde`, deliberately — they
//! are dependency-lean, and the ABI is not a good enough reason to change
//! that. So the serializable shapes live here, in the one crate that already
//! depends on `serde`, and they are mirrors: a field added to `Diagnostic`
//! that is not added here simply does not cross, which a reviewer can see.

use serde::Serialize;

use dashscene_validator::{Diagnostic, Location, Report, Severity};

use crate::figma::CompileError;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum WireLocation<'a> {
    Node { index: u32, path: &'a str },
    PaintEntry { index: u32 },
    ImageAsset { index: u32 },
}

impl<'a> From<&'a Location> for WireLocation<'a> {
    fn from(at: &'a Location) -> Self {
        match at {
            Location::Node(node) => Self::Node {
                index: node.index,
                path: &node.path,
            },
            Location::PaintEntry(index) => Self::PaintEntry { index: *index },
            Location::ImageAsset(index) => Self::ImageAsset { index: *index },
        }
    }
}

#[derive(Serialize)]
struct WireDiagnostic<'a> {
    rule: &'a str,
    severity: &'static str,
    at: WireLocation<'a>,
    message: &'a str,
}

impl<'a> From<&'a Diagnostic> for WireDiagnostic<'a> {
    fn from(diagnostic: &'a Diagnostic) -> Self {
        Self {
            rule: diagnostic.rule,
            severity: match diagnostic.severity {
                Severity::Warning => "warning",
                Severity::Error => "error",
            },
            at: (&diagnostic.at).into(),
            message: &diagnostic.message,
        }
    }
}

#[derive(Serialize)]
struct WireReport<'a> {
    diagnostics: Vec<WireDiagnostic<'a>>,
}

impl<'a> From<&'a Report> for WireReport<'a> {
    fn from(report: &'a Report) -> Self {
        Self {
            diagnostics: report.diagnostics().iter().map(WireDiagnostic::from).collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum WireError<'a> {
    Parse {
        message: String,
    },
    Unsupported {
        path: &'a str,
        what: &'a str,
    },
    UnresolvedImage {
        path: &'a str,
        #[serde(rename = "imageRef")]
        image_ref: &'a str,
    },
    Diagnostics {
        diagnostics: Vec<WireDiagnostic<'a>>,
    },
}

/// Serializing cannot fail: every mirror is a plain struct of strings and
/// integers, and `serde_json` only errors on a map with a non-string key or a
/// non-finite float. Neither exists here, so the `expect` is unreachable
/// rather than optimistic — and returning a `Result` would push an
/// unrepresentable failure onto every caller.
fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("a mirror type always serializes")
}

pub fn report_json(report: &Report) -> String {
    to_json(&WireReport::from(report))
}

pub fn compile_error_json(error: &CompileError) -> String {
    let wire = match error {
        CompileError::Parse(e) => WireError::Parse {
            message: e.to_string(),
        },
        CompileError::Unsupported { path, what } => WireError::Unsupported { path, what },
        CompileError::UnresolvedImage { path, image_ref } => {
            WireError::UnresolvedImage { path, image_ref }
        }
        CompileError::Diagnostics(report) => WireError::Diagnostics {
            diagnostics: report.diagnostics().iter().map(WireDiagnostic::from).collect(),
        },
    };
    to_json(&wire)
}

pub fn image_refs_json(refs: &[String]) -> String {
    to_json(&refs)
}
```

Add to `crates/dashc/src/abi/mod.rs`:

```rust
pub mod json;
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p dashc --lib abi::json`
Expected: PASS — 4 tests.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy -p dashc --all-targets -- -D warnings
cargo fmt --all
git add crates/dashc/src/abi
git commit -m "feat(dashc): serializable mirrors of the report and the compile error"
```

---

### Task 3: `figma::image_refs`

**Files:**

- Modify: `crates/dashc/src/figma/mod.rs`
- Modify: `crates/dashc/tests/figma_lowering.rs` (add the test below)

**Interfaces:**

- Produces: `figma::image_refs(&FigmaFile) -> Result<Vec<String>, CompileError>`
  — the `imageRef`s the lowering will demand, sorted and deduplicated.

- [ ] **Step 1: Write the failing test**

Append to `crates/dashc/tests/figma_lowering.rs`:

```rust
/// The Deno importer does not scan the file for `imageRef`s — it asks. This
/// is why: the answer comes from the same module that consumes it, so the
/// resolver and the lowering cannot disagree about where an `imageRef` lives.
#[test]
fn image_refs_names_every_ref_the_lowering_demands() {
    let refs = dashc_wasm::figma::image_refs(&parse(V03_PAINT)).expect("the fixture has a root frame");

    assert_eq!(refs, vec![IMAGE_REF.to_string()]);
}

#[test]
fn image_refs_refuses_a_file_with_no_root_frame() {
    let file: FigmaFile = serde_json::from_value(serde_json::json!({
        "name": "empty",
        "document": { "id": "0:0", "name": "Document", "type": "DOCUMENT", "children": [] }
    }))
    .expect("the synthetic document parses");

    assert!(matches!(
        dashc_wasm::figma::image_refs(&file),
        Err(CompileError::Unsupported { .. })
    ));
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p dashc --test figma_lowering image_refs`
Expected: FAIL — `image_refs` is not a member of `dashc_wasm::figma`.

- [ ] **Step 3: Implement it**

Add to `crates/dashc/src/figma/mod.rs`, below `lower`:

```rust
/// The `imageRef`s the lowering will demand, sorted and deduplicated.
///
/// The Deno importer cannot fetch what it cannot name, and Figma's `GET /file`
/// carries no image bytes — only refs. Rather than have the importer walk the
/// JSON looking for them (a second copy of "where an imageRef lives", free to
/// drift from the walk that actually consumes them), it asks here. The scan
/// covers the same subtree `lower` walks, and both fills and strokes, so a ref
/// this returns is a ref the lowering can resolve.
///
/// Deliberately a superset: a paint this returns may still be refused by the
/// lowering (a stacked fill, an invisible one). Fetching an image that turns
/// out to be unused costs a download; missing one is a failed compile.
pub fn image_refs(file: &FigmaFile) -> Result<Vec<String>, CompileError> {
    fn walk(node: &Node, found: &mut BTreeSet<String>) {
        for paint in node.fills.iter().chain(node.strokes.iter()) {
            if paint.kind == PaintTag::Image
                && let Some(image_ref) = &paint.image_ref
            {
                found.insert(image_ref.clone());
            }
        }
        for child in &node.children {
            walk(child, found);
        }
    }

    let mut found = BTreeSet::new();
    walk(root_frame(&file.document)?, &mut found);
    Ok(found.into_iter().collect())
}
```

Add `BTreeSet` to the existing `std::collections` import at the top of the
file:

```rust
use std::collections::{BTreeMap, BTreeSet};
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p dashc --test figma_lowering`
Expected: PASS — the two new tests, and every existing one still green.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy -p dashc --all-targets -- -D warnings
cargo fmt --all
git add crates/dashc/src/figma/mod.rs crates/dashc/tests/figma_lowering.rs
git commit -m "feat(dashc): name the imageRefs a lowering will demand"
```

---

### Task 4: The five exports

**Files:**

- Modify: `crates/dashc/src/abi/mod.rs`
- Create: `crates/dashc/tests/abi.rs`

**Interfaces:**

- Produces the pinned ABI: `dashc_abi_version`, `dashc_alloc`, `dashc_free`,
  `dashc_compile_figma`, `dashc_figma_image_refs`.

- [ ] **Step 1: Write the failing test**

Create `crates/dashc/tests/abi.rs`:

```rust
//! The wasm ABI, driven exactly as the Deno importer drives it — but natively.
//!
//! These call the real exports: allocate a request in the module's allocator,
//! write it, call, decode the length-prefixed response, free. The response is
//! a length-prefixed buffer rather than a `(ptr, len)` pair packed into a
//! `u64` precisely so this test can exist: a packed pair assumes a 32-bit
//! pointer, which is true on wasm and false here.
//!
//! What this cannot cover is the TypeScript codec on the other side. That is
//! what the shared golden `.dsb` is for (`goldens/dsb/v03-paint.dsb`): both
//! languages pin the same bytes.

use dashc_wasm::abi::{
    dashc_abi_version, dashc_alloc, dashc_compile_figma, dashc_figma_image_refs, dashc_free,
};

const V03_PAINT: &str = include_str!("../../../corpus/figma-fixtures/v03-paint.json");
const IMAGE_REF: &str = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";
const IMAGE_PNG: &[u8] =
    include_bytes!("../../../corpus/figma-fixtures/v03-paint.images/390616a0e7321eddb464388366d9a2a1bcb7f4c3.png");

/// One decoded response envelope.
struct Response {
    status: u32,
    blob: Vec<u8>,
    json: String,
}

fn encode_request(profile: u32, json: &str, images: &[(&str, u32, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&profile.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(json.as_bytes());
    out.extend_from_slice(&(images.len() as u32).to_le_bytes());
    for (image_ref, format, bytes) in images {
        out.extend_from_slice(&(image_ref.len() as u32).to_le_bytes());
        out.extend_from_slice(image_ref.as_bytes());
        out.extend_from_slice(&format.to_le_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"))
}

/// Drives one export the way `importers/figma/src/wasm.ts` does.
fn call(
    export: unsafe extern "C" fn(*const u8, u32) -> *mut u8,
    request: &[u8],
) -> Response {
    unsafe {
        let request_ptr = dashc_alloc(request.len() as u32);
        assert!(!request_ptr.is_null(), "the request allocation succeeds");
        std::ptr::copy_nonoverlapping(request.as_ptr(), request_ptr, request.len());

        let response_ptr = export(request_ptr, request.len() as u32);
        dashc_free(request_ptr, request.len() as u32);
        assert!(!response_ptr.is_null(), "the response allocation succeeds");

        let total = read_u32(std::slice::from_raw_parts(response_ptr, 4), 0) as usize;
        let response = std::slice::from_raw_parts(response_ptr, 4 + total).to_vec();
        dashc_free(response_ptr, (4 + total) as u32);

        let status = read_u32(&response, 4);
        let blob_len = read_u32(&response, 8) as usize;
        let blob = response[12..12 + blob_len].to_vec();
        let json_len = read_u32(&response, 12 + blob_len) as usize;
        let json_at = 12 + blob_len + 4;
        let json = String::from_utf8(response[json_at..json_at + json_len].to_vec())
            .expect("the json field is UTF-8");

        Response { status, blob, json }
    }
}

#[test]
fn the_abi_version_is_pinned() {
    // A bump is a deliberate break: the Deno loader refuses a version it does
    // not know, so this constant and `importers/figma/src/wasm.ts` move together.
    assert_eq!(dashc_abi_version(), 1);
}

#[test]
fn the_fixture_compiles_to_the_golden_dsb() {
    let request = encode_request(0, V03_PAINT, &[(IMAGE_REF, 0, IMAGE_PNG)]);
    let response = call(dashc_compile_figma, &request);

    assert_eq!(response.status, 0, "status 0 = ok; json was {}", response.json);
    assert_eq!(response.json, r#"{"diagnostics":[]}"#);
    assert_eq!(
        response.blob,
        include_bytes!("../../../goldens/dsb/v03-paint.dsb"),
        "the ABI emits the same bytes as the library call",
    );
}

#[test]
fn an_unresolved_image_is_a_tagged_error() {
    let request = encode_request(0, V03_PAINT, &[]);
    let response = call(dashc_compile_figma, &request);

    assert_eq!(response.status, 1);
    assert!(
        response.json.contains(r#""kind":"unresolvedImage""#),
        "json was {}",
        response.json,
    );
    assert!(response.json.contains(IMAGE_REF));
    assert!(response.blob.is_empty(), "a blocked document emits no bytes (R6)");
}

#[test]
fn a_malformed_request_is_a_status_not_a_trap() {
    let response = call(dashc_compile_figma, &[0, 0]);

    assert_eq!(response.status, 2);
    assert!(response.blob.is_empty());
    assert!(!response.json.is_empty(), "a malformed request explains itself");
}

#[test]
fn image_refs_crosses_the_abi() {
    let response = call(dashc_figma_image_refs, V03_PAINT.as_bytes());

    assert_eq!(response.status, 0);
    assert_eq!(response.json, format!(r#"["{IMAGE_REF}"]"#));
    assert!(response.blob.is_empty(), "image_refs carries no blob");
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo test -p dashc --test abi`
Expected: FAIL — `dashc_wasm::abi` is private, and the exports do not exist.
(It will also fail on the two `include_bytes!` paths, which Task 5 creates.
That is expected: run it again at the end of Task 5.)

- [ ] **Step 3: Write the exports**

Replace `crates/dashc/src/abi/mod.rs` with:

```rust
//! The wasm ABI: five `extern "C"` exports over a hand-written wire format.
//!
//! The exports are thin on purpose. Each one converts raw pointers into a
//! slice, calls a `dispatch` function that is ordinary safe Rust, and frames
//! the result — so everything worth testing is reachable from a native
//! `cargo test`, and `tests/abi.rs` drives these very symbols.
//!
//! Why hand-written rather than wasm-bindgen: see
//! `docs/decisions/dashc-wasm-abi.md`. The short version is that core wasm has
//! no string, array, or object type, so *every* option compiles down to
//! "allocate, copy in, pass a pointer and a length" — and wasm-bindgen would
//! not save the mirror types (`abi::json`), which are the actual work.

pub mod json;
pub mod wire;

use std::alloc::{Layout, alloc, dealloc};

use crate::abi::wire::Status;
use crate::figma::{self, CompileError};

/// The version of the wire format this module speaks.
#[unsafe(no_mangle)]
pub extern "C" fn dashc_abi_version() -> u32 {
    wire::ABI_VERSION
}

/// Reserves `len` bytes in the module's linear memory, for the caller to write
/// a request into. Returns null when `len` is 0 or the allocation fails —
/// never panics, because a panic here traps the module.
#[unsafe(no_mangle)]
pub extern "C" fn dashc_alloc(len: u32) -> *mut u8 {
    let Ok(layout) = Layout::from_size_align(len as usize, 1) else {
        return std::ptr::null_mut();
    };
    if layout.size() == 0 {
        return std::ptr::null_mut();
    }
    // SAFETY: the layout is non-zero-sized, checked directly above.
    unsafe { alloc(layout) }
}

/// Releases a buffer obtained from [`dashc_alloc`], or a response buffer
/// returned by one of the compile exports.
///
/// # Safety
///
/// `ptr` must have come from this module, and `len` must be the length it was
/// allocated with — for a response buffer, `4 + total`, where `total` is the
/// `u32` the buffer starts with.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dashc_free(ptr: *mut u8, len: u32) {
    let Ok(layout) = Layout::from_size_align(len as usize, 1) else {
        return;
    };
    if ptr.is_null() || layout.size() == 0 {
        return;
    }
    // SAFETY: the caller guarantees ptr/len came from this module's allocator
    // with this exact layout.
    unsafe { dealloc(ptr, layout) }
}

/// Compiles Figma REST JSON into a `.dsb`.
///
/// The request is the framing in [`wire`]; the return is a length-prefixed
/// response buffer the caller must release with [`dashc_free`].
///
/// # Safety
///
/// `ptr` must point at `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dashc_compile_figma(ptr: *const u8, len: u32) -> *mut u8 {
    // SAFETY: the caller guarantees ptr/len describe a readable region.
    let request = unsafe { as_slice(ptr, len) };
    leak(compile_figma_response(request))
}

/// Names the `imageRef`s a lowering of this file will demand.
///
/// The request is the raw UTF-8 JSON, unframed. The return is the same
/// response buffer as [`dashc_compile_figma`], with the refs as a JSON array
/// and no blob.
///
/// # Safety
///
/// `ptr` must point at `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dashc_figma_image_refs(ptr: *const u8, len: u32) -> *mut u8 {
    // SAFETY: the caller guarantees ptr/len describe a readable region.
    let request = unsafe { as_slice(ptr, len) };
    leak(image_refs_response(request))
}

/// # Safety
///
/// `ptr` must point at `len` readable bytes, or be null with `len` 0.
unsafe fn as_slice<'a>(ptr: *const u8, len: u32) -> &'a [u8] {
    if ptr.is_null() || len == 0 {
        return &[];
    }
    // SAFETY: the caller guarantees the region is readable for `len` bytes.
    unsafe { std::slice::from_raw_parts(ptr, len as usize) }
}

/// Hands a response buffer to the caller, who now owns it.
///
/// `into_boxed_slice` shrinks capacity to length, which is what makes the
/// caller's `dashc_free(ptr, 4 + total)` the exact inverse of this allocation.
fn leak(response: Vec<u8>) -> *mut u8 {
    Box::into_raw(response.into_boxed_slice()).cast::<u8>()
}

/// Everything above the raw-pointer layer. Safe Rust, and the whole reason the
/// exports are three lines each.
fn compile_figma_response(request: &[u8]) -> Vec<u8> {
    let request = match wire::decode_compile_request(request) {
        Ok(request) => request,
        Err(message) => {
            return wire::encode_response(Status::MalformedRequest as u32, &[], &message);
        }
    };

    match crate::compile_figma(&request.json, request.profile, &request.images) {
        Ok((bytes, report)) => {
            wire::encode_response(Status::Ok as u32, &bytes, &json::report_json(&report))
        }
        Err(error) => wire::encode_response(
            Status::CompileError as u32,
            &[],
            &json::compile_error_json(&error),
        ),
    }
}

fn image_refs_response(request: &[u8]) -> Vec<u8> {
    let json = match std::str::from_utf8(request) {
        Ok(json) => json,
        Err(e) => {
            return wire::encode_response(
                Status::MalformedRequest as u32,
                &[],
                &format!("the file JSON is not UTF-8: {e}"),
            );
        }
    };

    let file = match serde_json::from_str(json) {
        Ok(file) => file,
        Err(e) => {
            let error = CompileError::Parse(e);
            return wire::encode_response(
                Status::CompileError as u32,
                &[],
                &json::compile_error_json(&error),
            );
        }
    };

    match figma::image_refs(&file) {
        Ok(refs) => wire::encode_response(Status::Ok as u32, &[], &json::image_refs_json(&refs)),
        Err(error) => wire::encode_response(
            Status::CompileError as u32,
            &[],
            &json::compile_error_json(&error),
        ),
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p dashc --test abi the_abi_version_is_pinned`
Expected: PASS. The other four tests in the file still fail — they need the
corpus image and the golden, which is Task 5.

- [ ] **Step 5: Confirm the exports actually exist in the wasm**

```bash
just wasm
```

Then verify the cdylib exports the five symbols (this is the failure the whole
story exists to fix — before this task, the module exported nothing):

```bash
cargo install wasm-tools --locked 2>/dev/null || true
wasm-tools print target/wasm32-unknown-unknown/release/dashc_wasm.wasm | grep -c '(export "dashc_'
```

Expected: `5`. If `wasm-tools` is unavailable, this works too:

```bash
grep -c dashc_compile_figma target/wasm32-unknown-unknown/release/dashc_wasm.wasm
```

Expected: at least `1` (the export name appears in the export section).

- [ ] **Step 6: Lint and commit**

```bash
cargo clippy -p dashc --all-targets -- -D warnings
cargo fmt --all
git add crates/dashc/src crates/dashc/tests/abi.rs
git commit -m "feat(dashc): the wasm ABI exports"
```

---

### Task 5: The image fixture and the golden `.dsb`

The golden is the artifact both languages pin, so it has to exist before either
side can assert on it. It is generated from a **stand-in** PNG here; the real
bytes arrive at Task 10, and the golden is regenerated then. Doing it in this
order means the checkpoint is a data swap, not a blocked dependency.

**Files:**

- Create: `corpus/figma-fixtures/v03-paint.images/390616a0e7321eddb464388366d9a2a1bcb7f4c3.png`
- Create: `corpus/figma-fixtures/v03-paint.images/README.md`
- Create: `goldens/dsb/README.md`
- Create: `goldens/dsb/v03-paint.dsb` (generated, not hand-written)
- Modify: `crates/dashc/tests/figma_lowering.rs`

**Interfaces:**

- Produces: `goldens/dsb/v03-paint.dsb` — the bytes `crates/dashc/tests/abi.rs`
  and `importers/figma/src/wasm_test.ts` both assert against.

- [ ] **Step 1: Write the stand-in PNG into the corpus**

The bytes are the ones `figma_lowering.rs` invents today. Write them as a file
instead, so both languages can read the same input:

```bash
mkdir -p corpus/figma-fixtures/v03-paint.images
printf '\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\x0cIDAT\x08\xd7c\xf8\xcf\xc0\x00\x00\x03\x01\x01\x00\x18\xdd\x8d\xb0\x00\x00\x00\x00IEND\xaeB`\x82' \
  > corpus/figma-fixtures/v03-paint.images/390616a0e7321eddb464388366d9a2a1bcb7f4c3.png
```

Verify it is a valid 1×1 PNG:

```bash
file corpus/figma-fixtures/v03-paint.images/390616a0e7321eddb464388366d9a2a1bcb7f4c3.png
```

Expected: `PNG image data, 1 x 1, 8-bit/color RGB, non-interlaced`.

- [ ] **Step 2: Document what it is**

Create `corpus/figma-fixtures/v03-paint.images/README.md`:

```markdown
# v03-paint image fills

The bytes behind `v03-paint.json`'s image fills, one file per `imageRef`.

Figma's `GET /file` carries no image bytes — an image fill is a bare
`imageRef`. The bytes live behind `GET /v1/files/:key/images`, which returns a
presigned URL per ref. The **bytes** are committed here rather than that URL:
the URL is regenerated on every fetch, so committing it would rewrite this
fixture on every capture (issue #141).

Captured by `deno task capture`, which resolves each ref the lowering demands
(`dashc_figma_image_refs`) and writes the downloaded bytes here.

`crates/dashc/tests/figma_lowering.rs` and the Deno importer's tests both read
these files, so both halves of the byte-identity check compile from identical
input.

## 390616a0e7321eddb464388366d9a2a1bcb7f4c3.png

**A stand-in, not the captured asset** — a 1×1 opaque PNG, the smallest thing
that decodes. Replace it by running `just deno-capture` with `FIGMA_TOKEN` set,
then regenerate the golden:

    UPDATE_GOLDENS=1 cargo test -p dashc --test figma_lowering

Review the new `goldens/dsb/v03-paint.dsb` and commit both.
```

- [ ] **Step 3: Write the failing golden test**

In `crates/dashc/tests/figma_lowering.rs`, delete the `png_pixel()` function
entirely (the existing `IMAGE_REF` const stays as it is), add `IMAGE_PNG` next
to it, and point `images()` at the corpus file:

```rust
/// The fixture's image fill is an `imageRef` with no bytes anywhere in the
/// JSON, so the caller supplies them (design D1). In production that is the
/// Deno importer resolving `GET /images`; here it is the same corpus file the
/// importer's own tests read — which is what makes the golden below a
/// cross-language contract rather than two unrelated assertions.
const IMAGE_PNG: &[u8] = include_bytes!(
    "../../../corpus/figma-fixtures/v03-paint.images/390616a0e7321eddb464388366d9a2a1bcb7f4c3.png"
);

fn images() -> BTreeMap<String, ImageAsset> {
    BTreeMap::from([(
        IMAGE_REF.to_string(),
        ImageAsset {
            format: ImageFormat::Png,
            bytes: IMAGE_PNG.to_vec(),
        },
    )])
}
```

Then append the golden test and its helper:

```rust
/// The `.dsb` the Deno importer must reproduce byte for byte.
///
/// This is one half of the story-#17 acceptance criterion. The other half is
/// `importers/figma/src/wasm_test.ts`, which asserts the same bytes come back
/// through the wasm ABI. Neither test can see the other's toolchain, so the
/// golden is what makes "byte-identical to dashc-native output" checkable in
/// two CI jobs that never meet.
///
/// Regenerate with `UPDATE_GOLDENS=1` after a deliberate change to emission or
/// to the captured fixture, review the diff, and commit. A missing golden is a
/// failure, never an auto-create: CI on a clean checkout must fail loudly
/// rather than mint its own truth (`goldens/README.md`).
#[test]
fn the_fixture_emits_the_golden_dsb() {
    let (bytes, report) = compile_figma(V03_PAINT, Profile::Core, &images())
        .expect("the paint fixture compiles");
    assert!(report.is_empty(), "v03-paint emits clean");

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../goldens/dsb/v03-paint.dsb");

    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(path.parent().expect("the golden has a parent"))
            .expect("the goldens directory is writable");
        std::fs::write(&path, &bytes).expect("the golden is writable");
        return;
    }

    let golden = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\nrun `UPDATE_GOLDENS=1 cargo test -p dashc --test figma_lowering` to create it",
            path.display(),
        )
    });

    assert_eq!(
        bytes,
        golden,
        "emission drifted from the golden ({} bytes vs {}). If this is intended, \
         regenerate with UPDATE_GOLDENS=1, review the diff, and commit.",
        bytes.len(),
        golden.len(),
    );
}
```

- [ ] **Step 4: Run it and watch it fail**

Run: `cargo test -p dashc --test figma_lowering the_fixture_emits_the_golden_dsb`
Expected: FAIL — "cannot read .../goldens/dsb/v03-paint.dsb", with the
`UPDATE_GOLDENS=1` hint.

- [ ] **Step 5: Generate the golden**

```bash
UPDATE_GOLDENS=1 cargo test -p dashc --test figma_lowering the_fixture_emits_the_golden_dsb
cargo test -p dashc
```

Expected: the second command passes every test in the crate — including the four
in `tests/abi.rs` that were waiting on the golden and the corpus image.

Confirm the golden is a real `.dsb` and not an empty file:

```bash
cargo run -p dashc -- check goldens/dsb/v03-paint.dsb
```

Expected: `dashc: goldens/dsb/v03-paint.dsb is valid`.

- [ ] **Step 6: Document the golden**

Create `goldens/dsb/README.md`:

```markdown
# goldens/dsb

Golden `.dsb` documents: the compiler's output, frozen.

`images/` holds goldens that are _pictures_ — what a scene looks like once
painted, compared with a pixel tolerance. These are goldens that are _bytes_ —
what the compiler emits, compared exactly. Emission is byte-reproducible for a
given input (R7), so there is no tolerance to allow.

## v03-paint.dsb

`corpus/figma-fixtures/v03-paint.json` plus the image bytes in
`corpus/figma-fixtures/v03-paint.images/`, compiled through
`dashc::compile_figma`.

Two suites pin it, in two CI jobs that never meet:

- `crates/dashc/tests/figma_lowering.rs` — the native library call.
- `importers/figma/src/wasm_test.ts` — the same compile through the wasm ABI,
  from Deno.

That is what makes story #17's "byte-identical to dashc-native output"
checkable: each side asserts against the same committed bytes, so identity is
transitive.

## Regenerating

    UPDATE_GOLDENS=1 cargo test -p dashc --test figma_lowering

A golden is reviewed truth: inspect the change before committing it. A missing
golden never auto-creates on a normal run, so CI on a clean checkout fails
loudly instead of minting its own.
```

- [ ] **Step 7: Lint and commit**

```bash
cargo clippy -p dashc --all-targets -- -D warnings
cargo fmt --all
dprint fmt
markdownlint 'corpus/**/*.md' 'goldens/**/*.md'
git add corpus/figma-fixtures/v03-paint.images goldens/dsb crates/dashc/tests
git commit -m "test(dashc): pin the Figma compile with a golden .dsb"
```

---

### Task 6: `just` and CI

**Files:**

- Modify: `justfile`
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Build only the library for wasm**

In `justfile`, replace the `wasm` recipe:

```just
# Build dashc's cdylib for wasm32 — the module the Deno importer loads.
#
# --lib on purpose. Without it cargo also builds the `dashc` bin for wasm,
# producing a second artifact (dashc.wasm) that is the CLI — it reads files and
# reads the environment, and it exports none of the ABI. Two .wasm files where
# one is a decoy is a trap; the importer loads dashc_wasm.wasm.
wasm:
    cargo build -p dashc --lib --release --target wasm32-unknown-unknown
```

Add a `wasm` dependency to `deno-test`, and a capture recipe:

```just
# Run the Deno importer's test suite. Depends on `wasm`: the suite loads
# dashc_wasm.wasm and asserts its output against the golden .dsb.
deno-test: wasm
    cd importers/figma && deno task test

# Capture the Figma fixture corpus, including image-fill bytes. Needs
# FIGMA_TOKEN (SCOPE_DECISIONS.md §11). Never commit the token.
deno-capture:
    cd importers/figma && deno task capture
```

- [ ] **Step 2: Verify the decoy is gone**

```bash
rm -rf target/wasm32-unknown-unknown/release
just wasm
ls target/wasm32-unknown-unknown/release/*.wasm
```

Expected: exactly one file, `dashc_wasm.wasm`.

- [ ] **Step 3: Publish the module to the deno job**

In `.github/workflows/ci.yml`, in the `wasm-build` job, replace the build step
and add an upload:

```yaml
- run: cargo build -p dashc --lib --release --target wasm32-unknown-unknown
# The deno job loads this module and asserts its output against the
# golden .dsb. Handing it over as an artifact keeps the Rust toolchain
# (and flatc) out of that job.
- uses: actions/upload-artifact@v4
  with:
    name: dashc-wasm
    path: target/wasm32-unknown-unknown/release/dashc_wasm.wasm
    if-no-files-found: error
```

- [ ] **Step 4: Make the deno job run when the ABI changes**

Still in `.github/workflows/ci.yml`, widen the `figma` filter in the `changes`
job. Without this, a `dashc` change that breaks the ABI, with no edit under
`importers/figma/`, skips the deno job and merges green against a boundary
nothing checked:

```yaml
figma:
  - 'importers/figma/**'
  # The committed fixture manifest is validated by the deno job
  # (manifest_test.ts), so editing it must trigger that job —
  # otherwise a malformed manifest merges with a green CI run
  # (issue #90).
  - 'corpus/figma-fixtures/**'
  # The deno suite calls dashc's wasm ABI and pins its output
  # against the golden .dsb. Both live on the Rust side, so a
  # Rust-only change can break the importer — the job has to run
  # for those too (story #17).
  - 'crates/**'
  - 'goldens/dsb/**'
  - 'Cargo.toml'
  - 'Cargo.lock'
```

Then make the deno job consume the artifact:

```yaml
deno:
  name: deno
  needs: [changes, wasm-build]
  runs-on: ubuntu-latest
  # Path-filtered: only runs when the importer, the corpus, or the Rust side
  # it calls through the wasm ABI changes (SCOPE_DECISIONS.md §4/§7).
  if: needs.changes.outputs.figma == 'true'
  steps:
    - uses: actions/checkout@v4
    - uses: denoland/setup-deno@v2
      with:
        deno-version: v2.x
    # The suite loads this module. It is built once, in wasm-build.
    - uses: actions/download-artifact@v4
      with:
        name: dashc-wasm
        path: target/wasm32-unknown-unknown/release
    - working-directory: importers/figma
      run: |
        deno task check
        deno task lint
        deno task fmt --check
        deno task test
```

- [ ] **Step 5: Check the workflow parses**

```bash
python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml')); print('ci.yml parses')"
```

Expected: `ci.yml parses`.

- [ ] **Step 6: Commit**

```bash
git add justfile .github/workflows/ci.yml
git commit -m "ci(ci): hand dashc's wasm module to the deno job, and run it when the ABI moves"
```

---

### Task 7: The Deno ABI codec

**Files:**

- Modify: `importers/figma/src/wasm.ts` (replaces the stub entirely)
- Create: `importers/figma/src/wasm_test.ts`
- Modify: `importers/figma/deno.json` (test permissions)

**Interfaces:**

- Produces:
  - `type Profile = "core" | "full"`
  - `interface ImageAsset { readonly format: "png"; readonly bytes: Uint8Array }`
  - `interface Diagnostic { rule, severity, at, message }`
  - `interface CompileOk { readonly bytes: Uint8Array; readonly diagnostics: readonly Diagnostic[] }`
  - `class CompileFailed extends Error { readonly detail: CompileErrorDetail }`
  - `loadDashc(url?: URL): Promise<Dashc>`
  - `Dashc.compileFigma(json: string, profile: Profile, images: ReadonlyMap<string, ImageAsset>): CompileOk`
  - `Dashc.figmaImageRefs(json: string): string[]`

- [ ] **Step 1: Write the failing test**

Create `importers/figma/src/wasm_test.ts`:

```ts
/**
 * The wasm ABI, from the side that consumes it.
 *
 * The golden assertion is one half of story #17's acceptance criterion:
 * `crates/dashc/tests/figma_lowering.rs` asserts the native library call emits
 * `goldens/dsb/v03-paint.dsb`, and this asserts the wasm ABI emits the same
 * bytes. Neither suite can see the other's toolchain, so the committed golden
 * is what makes byte-identity checkable.
 */

import { assertEquals, assertRejects, assertThrows } from "@std/assert";

import { CompileFailed, type ImageAsset, loadDashc } from "./wasm.ts";

const CORPUS = new URL("../../../corpus/figma-fixtures/", import.meta.url);
const GOLDEN = new URL("../../../goldens/dsb/v03-paint.dsb", import.meta.url);
const IMAGE_REF = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";

const dashc = await loadDashc();

function fixture(name: string): string {
  return Deno.readTextFileSync(new URL(`${name}.json`, CORPUS));
}

function images(): Map<string, ImageAsset> {
  const bytes = Deno.readFileSync(
    new URL(`v03-paint.images/${IMAGE_REF}.png`, CORPUS),
  );
  return new Map([[IMAGE_REF, { format: "png", bytes }] as const]);
}

Deno.test("compileFigma emits the golden .dsb, byte for byte", () => {
  const result = dashc.compileFigma(fixture("v03-paint"), "core", images());

  assertEquals(result.diagnostics, []);
  assertEquals(result.bytes, Deno.readFileSync(GOLDEN));
});

Deno.test("figmaImageRefs names the refs the lowering demands", () => {
  assertEquals(dashc.figmaImageRefs(fixture("v03-paint")), [IMAGE_REF]);
});

Deno.test("an unsupported construct throws a tagged failure", () => {
  // effects-2025's root frame is auto-layout, which the lowering refuses
  // before it ever reaches the three REJECT-band effects the fixture carries.
  const error = assertThrows(
    () => dashc.compileFigma(fixture("effects-2025"), "core", new Map()),
    CompileFailed,
  );

  assertEquals((error as CompileFailed).detail.kind, "unsupported");
});

Deno.test("REJECT-band constructs come back as diagnostics, not bytes", () => {
  // Drop the auto-layout that stops the compile earlier, so the effects are
  // reached: what comes back must name each one, never silently drop it (P4).
  const file = JSON.parse(fixture("effects-2025"));
  const root = file.document.children[0].children[0];
  delete root.layoutMode;

  const error = assertThrows(
    () => dashc.compileFigma(JSON.stringify(file), "core", new Map()),
    CompileFailed,
  );

  const detail = (error as CompileFailed).detail;
  assertEquals(detail.kind, "diagnostics");
  if (detail.kind !== "diagnostics") throw new Error("unreachable");
  assertEquals(detail.diagnostics.length > 0, true);
  assertEquals(detail.diagnostics.every((d) => d.severity === "error"), true);
});

Deno.test("an unresolved imageRef is a named failure", () => {
  const error = assertThrows(
    () => dashc.compileFigma(fixture("v03-paint"), "core", new Map()),
    CompileFailed,
  );

  const detail = (error as CompileFailed).detail;
  assertEquals(detail.kind, "unresolvedImage");
  if (detail.kind !== "unresolvedImage") throw new Error("unreachable");
  assertEquals(detail.imageRef, IMAGE_REF);
});

Deno.test("a module that is not dashc is refused by name", async () => {
  await assertRejects(
    () => loadDashc(new URL("./wasm.ts", import.meta.url)),
    Error,
    "just wasm",
  );
});
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cd importers/figma && deno task test src/wasm_test.ts`
Expected: FAIL — `loadDashc` is not exported from `./wasm.ts`.

- [ ] **Step 3: Write the codec**

Replace `importers/figma/src/wasm.ts` entirely:

```ts
/**
 * Boundary with `dashc_wasm.wasm` (crates/dashc, built via `just wasm`).
 *
 * Deno hands Figma REST JSON and the image bytes it resolved to the same Rust
 * code path the native `dashc` library call runs — Figma lowering,
 * `dashscene-validator` validation, `.dsb` emission — and gets back `.dsb`
 * bytes plus a diagnostics report, or a tagged failure. Same R6 rule either
 * way: an error blocks emission, never a silent drop (SCOPE_DECISIONS.md §4).
 *
 * The ABI is hand-written rather than generated by wasm-bindgen; the reasoning
 * is in `docs/decisions/dashc-wasm-abi.md`. Core WebAssembly has no string,
 * array, or object type — only numbers and one linear memory — so a boundary
 * like this one is always "allocate, copy in, pass a pointer and a length".
 * The only question is who writes that, and here it is this file.
 *
 * Wire format (little-endian, u32 lengths), mirrored in `crates/dashc/src/abi/wire.rs`:
 *
 *   request   u32 profile | u32 json_len | json | u32 image_count
 *                 | per image: u32 ref_len | ref | u32 format | u32 bytes_len | bytes
 *   response  u32 total | u32 status | u32 blob_len | blob | u32 json_len | json
 */

/** The paint-vocabulary subset the target honors (DESIGN §5, R6). */
export type Profile = "core" | "full";

/** One encoded image asset. v0.3 knows exactly one container format. */
export interface ImageAsset {
  readonly format: "png";
  readonly bytes: Uint8Array;
}

/** What a diagnostic points at — a node, or a pooled entry by its own index. */
export type Location =
  | { readonly kind: "node"; readonly index: number; readonly path: string }
  | { readonly kind: "paintEntry"; readonly index: number }
  | { readonly kind: "imageAsset"; readonly index: number };

export interface Diagnostic {
  readonly rule: string;
  readonly severity: "warning" | "error";
  readonly at: Location;
  readonly message: string;
}

/** A compile that produced a document. Warnings do not block, so they ride along. */
export interface CompileOk {
  readonly bytes: Uint8Array;
  readonly diagnostics: readonly Diagnostic[];
}

/** Why a file could not be compiled at all — the four `CompileError` variants. */
export type CompileErrorDetail =
  | { readonly kind: "parse"; readonly message: string }
  | { readonly kind: "unsupported"; readonly path: string; readonly what: string }
  | {
    readonly kind: "unresolvedImage";
    readonly path: string;
    readonly imageRef: string;
  }
  | {
    readonly kind: "diagnostics";
    readonly diagnostics: readonly Diagnostic[];
  };

/** A compile that emitted nothing. R6: an error blocks the document. */
export class CompileFailed extends Error {
  readonly detail: CompileErrorDetail;

  constructor(detail: CompileErrorDetail) {
    super(describe(detail));
    this.name = "CompileFailed";
    this.detail = detail;
  }
}

function describe(detail: CompileErrorDetail): string {
  switch (detail.kind) {
    case "parse":
      return `not valid Figma REST JSON: ${detail.message}`;
    case "unsupported":
      return `${detail.path}: ${detail.what} is not in the v0.3 vocabulary`;
    case "unresolvedImage":
      return `${detail.path}: no image supplied for imageRef ${detail.imageRef}`;
    case "diagnostics":
      return detail.diagnostics
        .map((d) => `${d.severity}[${d.rule}]: ${d.message}`)
        .join("\n");
  }
}

/** The wire format this file speaks. `dashc_abi_version` must agree. */
const ABI_VERSION = 1;

const STATUS_OK = 0;
const STATUS_COMPILE_ERROR = 1;
const STATUS_MALFORMED_REQUEST = 2;

const PROFILE: Record<Profile, number> = { core: 0, full: 1 };
const FORMAT: Record<ImageAsset["format"], number> = { png: 0 };

/** Where `just wasm` puts the module. */
const DEFAULT_MODULE = new URL(
  "../../../target/wasm32-unknown-unknown/release/dashc_wasm.wasm",
  import.meta.url,
);

interface Exports {
  readonly memory: WebAssembly.Memory;
  readonly dashc_abi_version: () => number;
  readonly dashc_alloc: (len: number) => number;
  readonly dashc_free: (ptr: number, len: number) => void;
  readonly dashc_compile_figma: (ptr: number, len: number) => number;
  readonly dashc_figma_image_refs: (ptr: number, len: number) => number;
}

/** A framed response, decoded. */
interface Response {
  readonly status: number;
  readonly blob: Uint8Array;
  readonly json: string;
}

class Writer {
  #bytes: number[] = [];

  u32(value: number): void {
    this.#bytes.push(value & 0xff, (value >>> 8) & 0xff, (value >>> 16) & 0xff, (value >>> 24) & 0xff);
  }

  bytes(value: Uint8Array): void {
    this.u32(value.length);
    for (const byte of value) this.#bytes.push(byte);
  }

  text(value: string): void {
    this.bytes(new TextEncoder().encode(value));
  }

  finish(): Uint8Array {
    return new Uint8Array(this.#bytes);
  }
}

export class Dashc {
  readonly #exports: Exports;

  constructor(exports: Exports) {
    this.#exports = exports;
  }

  /**
   * Compiles Figma REST JSON into a `.dsb`.
   *
   * `images` maps every `imageRef` the lowering demands (ask
   * {@link Dashc.figmaImageRefs}) to its bytes. `dashc` does no I/O — it
   * compiles to wasm — so resolving a ref is the caller's job by construction.
   *
   * @throws {CompileFailed} when the document is blocked (R6).
   */
  compileFigma(
    json: string,
    profile: Profile,
    images: ReadonlyMap<string, ImageAsset>,
  ): CompileOk {
    const writer = new Writer();
    writer.u32(PROFILE[profile]);
    writer.text(json);
    writer.u32(images.size);
    for (const [imageRef, asset] of images) {
      writer.text(imageRef);
      writer.u32(FORMAT[asset.format]);
      writer.bytes(asset.bytes);
    }

    const response = this.#call(
      this.#exports.dashc_compile_figma,
      writer.finish(),
    );

    switch (response.status) {
      case STATUS_OK:
        return {
          bytes: response.blob,
          diagnostics: (JSON.parse(response.json) as {
            diagnostics: Diagnostic[];
          }).diagnostics,
        };
      case STATUS_COMPILE_ERROR:
        throw new CompileFailed(JSON.parse(response.json) as CompileErrorDetail);
      default:
        throw new Error(malformed(response));
    }
  }

  /**
   * The `imageRef`s a lowering of this file will demand.
   *
   * Asked rather than scanned: a second copy of "where an imageRef lives in
   * Figma's shape", written here, could drift from the walk in `dashc` that
   * actually consumes them (P5).
   */
  figmaImageRefs(json: string): string[] {
    const response = this.#call(
      this.#exports.dashc_figma_image_refs,
      new TextEncoder().encode(json),
    );

    switch (response.status) {
      case STATUS_OK:
        return JSON.parse(response.json) as string[];
      case STATUS_COMPILE_ERROR:
        throw new CompileFailed(JSON.parse(response.json) as CompileErrorDetail);
      default:
        throw new Error(malformed(response));
    }
  }

  /**
   * One round trip: reserve, write, call, read, release.
   *
   * `memory.buffer` is re-read after every call into the module. An allocation
   * can grow the memory, and growth detaches the previous ArrayBuffer — a view
   * held across a call would read zeroes or throw.
   */
  #call(
    exported: (ptr: number, len: number) => number,
    request: Uint8Array,
  ): Response {
    const { dashc_alloc, dashc_free, memory } = this.#exports;

    const requestPtr = dashc_alloc(request.length);
    if (requestPtr === 0 && request.length > 0) {
      throw new Error("dashc.wasm: the request allocation failed");
    }
    let responsePtr = 0;
    let total = 0;
    try {
      new Uint8Array(memory.buffer, requestPtr, request.length).set(request);

      responsePtr = exported(requestPtr, request.length);
      if (responsePtr === 0) {
        throw new Error("dashc.wasm: the response allocation failed");
      }

      const header = new DataView(memory.buffer, responsePtr, 4);
      total = header.getUint32(0, true);

      // Copied out of linear memory (slice, not subarray) before anything can
      // grow it: the caller keeps these bytes long after the buffer is freed.
      const envelope = new Uint8Array(memory.buffer, responsePtr + 4, total).slice();
      return decode(envelope);
    } finally {
      if (requestPtr !== 0) dashc_free(requestPtr, request.length);
      if (responsePtr !== 0) dashc_free(responsePtr, 4 + total);
    }
  }
}

function decode(envelope: Uint8Array): Response {
  const view = new DataView(envelope.buffer, envelope.byteOffset, envelope.byteLength);

  const status = view.getUint32(0, true);
  const blobLen = view.getUint32(4, true);
  const blob = envelope.slice(8, 8 + blobLen);
  const jsonLen = view.getUint32(8 + blobLen, true);
  const jsonAt = 8 + blobLen + 4;
  const json = new TextDecoder().decode(envelope.subarray(jsonAt, jsonAt + jsonLen));

  return { status, blob, json };
}

function malformed(response: Response): string {
  if (response.status === STATUS_MALFORMED_REQUEST) {
    // The two codecs disagree about the wire format. That is a bug in this
    // file or in crates/dashc/src/abi/wire.rs, never bad user input.
    return `dashc.wasm rejected the request as malformed: ${response.json}`;
  }
  return `dashc.wasm returned an unknown status ${response.status}`;
}

/**
 * Instantiates the module and checks it speaks this wire format.
 *
 * A version mismatch, or a `.wasm` that exports nothing at all (which is what
 * the cdylib was before story #17), fails here with a sentence naming
 * `just wasm` — rather than misdecoding somewhere deep in a test.
 */
export async function loadDashc(url: URL = DEFAULT_MODULE): Promise<Dashc> {
  let instance: WebAssembly.Instance;
  try {
    const bytes = await Deno.readFile(url);
    ({ instance } = await WebAssembly.instantiate(bytes, {}));
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(
      `cannot load ${url.pathname}: ${message} — run \`just wasm\` to build it`,
    );
  }

  const exports = instance.exports as unknown as Exports;
  if (typeof exports.dashc_abi_version !== "function") {
    throw new Error(
      `${url.pathname} exports no dashc ABI — run \`just wasm\` to rebuild it`,
    );
  }

  const version = exports.dashc_abi_version();
  if (version !== ABI_VERSION) {
    throw new Error(
      `${url.pathname} speaks ABI version ${version}, this importer speaks ` +
        `${ABI_VERSION} — run \`just wasm\` to rebuild it`,
    );
  }

  return new Dashc(exports);
}
```

- [ ] **Step 4: Widen the test permissions**

In `importers/figma/deno.json`, the `test` task must be able to read the wasm
module and the golden:

```json
"test": "deno test --allow-net=api.figma.com --allow-read=.,../../corpus/figma-fixtures,../../goldens/dsb,../../target/wasm32-unknown-unknown/release --allow-env=FIGMA_TOKEN",
```

- [ ] **Step 5: Run the tests**

```bash
just wasm
cd importers/figma && deno task test src/wasm_test.ts
```

Expected: PASS — 6 tests. The golden assertion is the one that matters: it is
the Deno half of the acceptance criterion.

- [ ] **Step 6: Lint and commit**

```bash
cd importers/figma && deno task fmt && deno task lint && deno task check
cd ../.. && git add importers/figma
git commit -m "feat(importers): call dashc's wasm ABI from Deno"
```

---

### Task 8: Resolving an `imageRef`

**Files:**

- Modify: `importers/figma/src/fetch.ts`
- Create: `importers/figma/src/images.ts`
- Create: `importers/figma/src/images_test.ts`

**Interfaces:**

- Consumes: `FigmaClient`, `ImageAsset` from `./wasm.ts`.
- Produces: `FigmaClient.imageFills(fileKey): Promise<Readonly<Record<string, string>>>`,
  `resolveImages(options): Promise<Map<string, ImageAsset>>`.

- [ ] **Step 1: Write the failing test**

Create `importers/figma/src/images_test.ts`:

```ts
/**
 * imageRef resolution: the seam that exists so `dashc` never fetches.
 *
 * Scripted fetch throughout — the suite never touches the network.
 */

import { assertEquals, assertRejects } from "@std/assert";

import { createFigmaClient } from "./fetch.ts";
import { resolveImages } from "./images.ts";

const PNG = Uint8Array.from([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d,
]);

/** Answers by URL, not by call order: a resolver may fetch in any order. */
function scripted(routes: Record<string, () => Response>): typeof fetch {
  return (input: string | URL | Request) => {
    const url = input instanceof Request ? input.url : String(input);
    const route = routes[url];
    if (!route) return Promise.resolve(new Response("not found", { status: 404 }));
    return Promise.resolve(route());
  };
}

const FILE_KEY = "abc123";
const REF = "390616a0";
const IMAGES_URL = `https://api.figma.com/v1/files/${FILE_KEY}/images`;
const ASSET_URL = "https://s3-alpha-sig.figma.com/img/390616a0?signed=yes";

Deno.test("resolveImages downloads exactly the refs it was asked for", async () => {
  const fetchFn = scripted({
    [IMAGES_URL]: () =>
      Response.json({
        error: false,
        status: 200,
        meta: { images: { [REF]: ASSET_URL, unused: "https://example.invalid/x" } },
      }),
    [ASSET_URL]: () => new Response(PNG),
  });

  const images = await resolveImages({
    client: createFigmaClient({ token: "x", fetchFn }),
    fileKey: FILE_KEY,
    refs: [REF],
    fetchFn,
  });

  assertEquals(images.size, 1, "the unused ref in the map is not downloaded");
  assertEquals(images.get(REF)?.format, "png");
  assertEquals(images.get(REF)?.bytes, PNG);
});

Deno.test("a ref missing from the map is a named error", async () => {
  const fetchFn = scripted({
    [IMAGES_URL]: () =>
      Response.json({ error: false, status: 200, meta: { images: {} } }),
  });

  await assertRejects(
    () =>
      resolveImages({
        client: createFigmaClient({ token: "x", fetchFn }),
        fileKey: FILE_KEY,
        refs: [REF],
        fetchFn,
      }),
    Error,
    REF,
  );
});

Deno.test("a non-PNG asset is refused, never guessed", async () => {
  // The .dsb image table has exactly one container format in v0.3. Handing a
  // JPEG's bytes over as a PNG would fail in the painter, far from the cause.
  const fetchFn = scripted({
    [IMAGES_URL]: () =>
      Response.json({
        error: false,
        status: 200,
        meta: { images: { [REF]: ASSET_URL } },
      }),
    [ASSET_URL]: () => new Response(Uint8Array.from([0xff, 0xd8, 0xff, 0xe0])),
  });

  await assertRejects(
    () =>
      resolveImages({
        client: createFigmaClient({ token: "x", fetchFn }),
        fileKey: FILE_KEY,
        refs: [REF],
        fetchFn,
      }),
    Error,
    "not a PNG",
  );
});

Deno.test("a failed download names the ref and the status", async () => {
  const fetchFn = scripted({
    [IMAGES_URL]: () =>
      Response.json({
        error: false,
        status: 200,
        meta: { images: { [REF]: ASSET_URL } },
      }),
    [ASSET_URL]: () => new Response("gone", { status: 403 }),
  });

  await assertRejects(
    () =>
      resolveImages({
        client: createFigmaClient({ token: "x", fetchFn }),
        fileKey: FILE_KEY,
        refs: [REF],
        fetchFn,
      }),
    Error,
    "403",
  );
});

Deno.test("no refs means no requests at all", async () => {
  const images = await resolveImages({
    client: createFigmaClient({
      token: "x",
      fetchFn: () => {
        throw new Error("the resolver must not fetch when there is nothing to resolve");
      },
    }),
    fileKey: FILE_KEY,
    refs: [],
    fetchFn: () => {
      throw new Error("the resolver must not fetch when there is nothing to resolve");
    },
  });

  assertEquals(images.size, 0);
});
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cd importers/figma && deno task test src/images_test.ts`
Expected: FAIL — `./images.ts` does not exist.

- [ ] **Step 3: Add the endpoint to the client**

In `importers/figma/src/fetch.ts`, add the import:

```ts
import type {
  GetFileMetaResponse,
  GetFileResponse,
  GetImageFillsResponse,
} from "@figma/rest-api-spec";
```

and the method, next to `file()`:

```ts
/**
 * The `imageRef` → presigned-URL map for every image fill in the file.
 *
 * Figma's `GET /file` carries no image bytes, only refs; the bytes live
 * behind these URLs. They are presigned and short-lived, which is why the
 * capture tool commits the downloaded *bytes* and never the URL (issue #141).
 */
imageFills(fileKey: string): Promise<Readonly<Record<string, string>>> {
  return this.#request(`/v1/files/${fileKey}/images`).then((body) => {
    const images = (body as GetImageFillsResponse | null)?.meta?.images;
    return images ?? {};
  });
}
```

- [ ] **Step 4: Write the resolver**

Create `importers/figma/src/images.ts`:

```ts
/**
 * imageRef → bytes.
 *
 * This is the seam story #139 pinned: `dashc` compiles to
 * wasm32-unknown-unknown, so it does no network and no filesystem I/O — and
 * Figma serializes an image fill as a bare `imageRef` with no bytes anywhere
 * in the file JSON. Whoever *can* fetch resolves the refs and hands the bytes
 * across the ABI. That is this file.
 *
 * Which refs to resolve is not decided here either: `dashc` is asked
 * (`figmaImageRefs`), because it is the module that consumes them.
 */

import type { FigmaClient } from "./fetch.ts";
import type { ImageAsset } from "./wasm.ts";

export interface ResolveImagesOptions {
  readonly client: FigmaClient;
  readonly fileKey: string;
  /** The refs the lowering demands — from `Dashc.figmaImageRefs`. */
  readonly refs: readonly string[];
  /**
   * Injectable for tests. Defaults to the global fetch.
   *
   * The asset download does not go to `api.figma.com` — the URLs are presigned
   * and point at Figma's asset host — so it does not run through the REST
   * client's limiter, which exists for the rate-limited API (§11).
   */
  readonly fetchFn?: typeof fetch;
}

/** The eight bytes that open every PNG (RFC 2083 §3.1). */
const PNG_SIGNATURE = Uint8Array.from([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
]);

function isPng(bytes: Uint8Array): boolean {
  return bytes.length >= PNG_SIGNATURE.length &&
    PNG_SIGNATURE.every((byte, at) => bytes[at] === byte);
}

/**
 * Downloads the bytes behind each ref.
 *
 * @throws when a ref has no URL, a download fails, or an asset is not a PNG —
 * the `.dsb` image table knows exactly one container format in v0.3, and
 * guessing is what P4 forbids.
 */
export async function resolveImages(
  options: ResolveImagesOptions,
): Promise<Map<string, ImageAsset>> {
  const { client, fileKey, refs } = options;
  const images = new Map<string, ImageAsset>();
  if (refs.length === 0) return images;

  const fetchFn = options.fetchFn ?? fetch;
  const urls = await client.imageFills(fileKey);

  for (const ref of refs) {
    const url = urls[ref];
    if (!url) {
      throw new Error(
        `figma-image-unresolved: the file's image map has no URL for imageRef ` +
          `${ref} — the fill references an asset the file does not carry`,
      );
    }

    const response = await fetchFn(url);
    if (!response.ok) {
      await response.body?.cancel();
      throw new Error(
        `figma-image-download: GET the asset for imageRef ${ref} returned ` +
          `${response.status}`,
      );
    }

    const bytes = new Uint8Array(await response.arrayBuffer());
    if (!isPng(bytes)) {
      throw new Error(
        `figma-image-format: the asset for imageRef ${ref} is not a PNG — ` +
          "the v0.3 image table carries PNG only (dashpaint::ImageFormat)",
      );
    }

    images.set(ref, { format: "png", bytes });
  }

  return images;
}
```

- [ ] **Step 5: Run the tests**

Run: `cd importers/figma && deno task test src/images_test.ts`
Expected: PASS — 5 tests.

- [ ] **Step 6: Lint and commit**

```bash
cd importers/figma && deno task fmt && deno task lint && deno task check
cd ../.. && git add importers/figma
git commit -m "feat(importers): resolve an imageRef into bytes"
```

---

### Task 9: The importer, end to end

**Files:**

- Create: `importers/figma/src/import.ts`
- Create: `importers/figma/src/import_test.ts`
- Modify: `importers/figma/src/mod.ts`
- Modify: `importers/figma/src/mod_test.ts`
- Modify: `importers/figma/deno.json`

**Interfaces:**

- Produces: `importFigmaFile(options): Promise<CompileOk>`.

- [ ] **Step 1: Write the failing test**

Create `importers/figma/src/import_test.ts`:

```ts
/**
 * The five steps, with a scripted Figma: fetch the file, ask which refs the
 * lowering needs, resolve them, compile.
 */

import { assertEquals } from "@std/assert";

import { createFigmaClient } from "./fetch.ts";
import { importFigmaFile } from "./import.ts";
import { loadDashc } from "./wasm.ts";

const CORPUS = new URL("../../../corpus/figma-fixtures/", import.meta.url);
const GOLDEN = new URL("../../../goldens/dsb/v03-paint.dsb", import.meta.url);
const FILE_KEY = "abc123";
const REF = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";
const ASSET_URL = "https://s3-alpha-sig.figma.com/img/390616a0?signed=yes";

const dashc = await loadDashc();

Deno.test("importFigmaFile compiles a file into the golden .dsb", async () => {
  const file = Deno.readTextFileSync(new URL("v03-paint.json", CORPUS));
  const png = Deno.readFileSync(new URL(`v03-paint.images/${REF}.png`, CORPUS));

  const requested: string[] = [];
  const fetchFn = (input: string | URL | Request) => {
    const url = input instanceof Request ? input.url : String(input);
    requested.push(url);
    if (url === `https://api.figma.com/v1/files/${FILE_KEY}?plugin_data=shared`) {
      return Promise.resolve(new Response(file));
    }
    if (url === `https://api.figma.com/v1/files/${FILE_KEY}/images`) {
      return Promise.resolve(
        Response.json({ error: false, status: 200, meta: { images: { [REF]: ASSET_URL } } }),
      );
    }
    if (url === ASSET_URL) return Promise.resolve(new Response(png));
    return Promise.resolve(new Response("not found", { status: 404 }));
  };

  const result = await importFigmaFile({
    client: createFigmaClient({ token: "x", fetchFn }),
    dashc,
    fileKey: FILE_KEY,
    profile: "core",
    fetchFn,
  });

  assertEquals(result.bytes, Deno.readFileSync(GOLDEN));
  assertEquals(result.diagnostics, []);
  assertEquals(requested.length, 3, "one file fetch, one image map, one download");
});
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cd importers/figma && deno task test src/import_test.ts`
Expected: FAIL — `./import.ts` does not exist.

- [ ] **Step 3: Write the orchestration and its CLI**

Create `importers/figma/src/import.ts`:

```ts
/**
 * Figma file → `.dsb`.
 *
 * The Deno half owns HTTP, auth, and resolving an `imageRef` into bytes. Every
 * decision about what the document *means* — lowering, validation, emission —
 * belongs to `dashc`, reached through the wasm ABI, so the importer and the
 * native compiler cannot disagree (SCOPE_DECISIONS.md §4).
 *
 *   1. GET /files/:key         the file JSON
 *   2. figmaImageRefs          the refs the lowering demands
 *   3. GET /files/:key/images  the ref → URL map
 *   4. download those refs     the bytes
 *   5. compileFigma            the .dsb
 *
 * Run as `deno task import <fileKey> -o out.dsb`.
 */

import { createFigmaClient, type FigmaClient, REQUIRED_SCOPES } from "./fetch.ts";
import { resolveImages } from "./images.ts";
import { type CompileOk, type Dashc, loadDashc, type Profile } from "./wasm.ts";

export interface ImportFigmaFileOptions {
  readonly client: FigmaClient;
  readonly dashc: Dashc;
  readonly fileKey: string;
  readonly profile: Profile;
  /** Injectable for tests; used for the presigned asset downloads. */
  readonly fetchFn?: typeof fetch;
}

/**
 * @throws {CompileFailed} when the document is blocked (R6) — no `.dsb` is
 * emitted, and the diagnostics say why.
 */
export async function importFigmaFile(
  options: ImportFigmaFileOptions,
): Promise<CompileOk> {
  const { client, dashc, fileKey, profile, fetchFn } = options;

  const json = JSON.stringify(await client.file(fileKey));
  const refs = dashc.figmaImageRefs(json);
  const images = await resolveImages({ client, fileKey, refs, fetchFn });

  return dashc.compileFigma(json, profile, images);
}

if (import.meta.main) {
  const args = [...Deno.args];
  const output = (() => {
    const at = args.findIndex((arg) => arg === "-o" || arg === "--output");
    if (at === -1) return null;
    const [, path] = args.splice(at, 2);
    return path ?? null;
  })();
  const [fileKey] = args;

  if (!fileKey || !output) {
    console.error("usage: deno task import <fileKey> -o <out.dsb>");
    Deno.exit(2);
  }

  const token = Deno.env.get("FIGMA_TOKEN");
  if (!token) {
    console.error(
      "FIGMA_TOKEN is not set. Create a Figma PAT with the scopes " +
        REQUIRED_SCOPES + " (SCOPE_DECISIONS.md §11) and export it. " +
        "Never commit it.",
    );
    Deno.exit(1);
  }

  const result = await importFigmaFile({
    client: createFigmaClient({ token, log: (line) => console.log(line) }),
    dashc: await loadDashc(),
    fileKey,
    profile: "core",
  });

  await Deno.writeFile(output, result.bytes);
  // A warning does not block, so it would otherwise leave with the bytes and
  // never be seen. P4: never a silent drop.
  for (const diagnostic of result.diagnostics) {
    console.warn(`${diagnostic.severity}[${diagnostic.rule}]: ${diagnostic.message}`);
  }
  console.log(`wrote ${output} (${result.bytes.length} bytes)`);
}
```

- [ ] **Step 4: Export it, and retire the stub assertion**

In `importers/figma/src/mod.ts`, update the module doc and the exports:

```ts
/**
 * @driftsys/dashscene-figma — public entry point.
 *
 * Deno-side half of the Figma importer (SCOPE_DECISIONS.md §4). Owns HTTP,
 * auth, and resolving an `imageRef` into bytes; hands the file JSON and those
 * bytes to `dashc.wasm` for lowering, validation, and `.dsb` emission — the
 * same Rust code path as the native `dashc` library call (crates/dashc).
 *
 * The REST client (fetch.ts), the fixture capture tool (capture.ts), the wasm
 * boundary (wasm.ts), image resolution (images.ts), and the import flow
 * (import.ts) are implemented. Closure, trim, and tokens remain stubs whose
 * implementation begins alongside v0.7 ("importer catch-up",
 * DESIGN_1.md §11).
 */

export * from "./fetch.ts";
export * from "./closure.ts";
export * from "./trim.ts";
export * from "./tokens.ts";
export * from "./wasm.ts";
export * from "./images.ts";
export * from "./import.ts";
```

In `importers/figma/src/mod_test.ts`, delete the `compileViaWasm` test and its
import (`compileViaWasm` no longer exists), leaving the other three stub tests
untouched. Update the file's doc comment to match:

```ts
/**
 * Smoke test: confirms `deno test` is wired up correctly, that
 * `createFigmaClient` is reachable through the public entry point, and that
 * the remaining importer stubs throw their documented "not yet implemented"
 * error (real implementation begins alongside v0.7, DESIGN_1.md §11).
 *
 * The wasm boundary is no longer a stub — see wasm_test.ts.
 */

import { assertEquals, assertThrows } from "@std/assert";
import { computeClosure, createFigmaClient, joinTokens, trim } from "./mod.ts";
```

- [ ] **Step 5: Wire the task**

In `importers/figma/deno.json`, add `src/import.ts` to the `check` task and add
the `import` task:

```json
"check": "deno check src/mod.ts src/capture.ts src/import.ts plugin/code.ts",
"import": "deno run --allow-env=FIGMA_TOKEN --allow-net=api.figma.com,s3-alpha-sig.figma.com --allow-read=../../target/wasm32-unknown-unknown/release --allow-write=. src/import.ts",
```

- [ ] **Step 6: Run everything**

```bash
cd importers/figma && deno task test
```

Expected: PASS — every suite, including the untouched fetch/capture/manifest
tests.

- [ ] **Step 7: Lint and commit**

```bash
cd importers/figma && deno task fmt && deno task lint && deno task check
cd ../.. && git add importers/figma
git commit -m "feat(importers): import a Figma file into a .dsb"
```

---

### Task 10: Capture the image bytes

**Files:**

- Modify: `importers/figma/src/capture.ts`
- Modify: `importers/figma/src/capture_test.ts`
- Modify: `importers/figma/deno.json`

**Interfaces:**

- Consumes: `resolveImages`, `Dashc.figmaImageRefs`.
- Produces: `captureFixtures` writes image bytes alongside each fixture.

- [ ] **Step 1: Write the failing test**

Append to `importers/figma/src/capture_test.ts`. It reuses the file's existing
`scriptedClient` and `jsonResponse` helpers.

Note `scriptedClient` answers **by queue position, not by URL** (that is open
debt #92), so the two client requests must be scripted in the order the capture
makes them: the file, then the image map. The asset download does not go
through the client at all — it is a presigned URL on Figma's asset host — so it
arrives through the separate `fetchFn`:

```ts
const V03_PAINT = Deno.readTextFileSync(
  new URL("../../../corpus/figma-fixtures/v03-paint.json", import.meta.url),
);
const V03_PAINT_REF = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";
const ASSET_URL = "https://s3-alpha-sig.figma.com/img/390616a0?signed=yes";

Deno.test("a captured fixture also captures its image-fill bytes", async () => {
  const png = Deno.readFileSync(
    new URL(
      `../../../corpus/figma-fixtures/v03-paint.images/${V03_PAINT_REF}.png`,
      import.meta.url,
    ),
  );

  // In queue order: GET /files/:key, then GET /files/:key/images. There is no
  // fileMeta call — readCapturedVersion returns null, so there is no captured
  // version to compare against.
  const { client } = scriptedClient([
    () => new Response(V03_PAINT, { status: 200 }),
    () =>
      jsonResponse({
        error: false,
        status: 200,
        meta: { images: { [V03_PAINT_REF]: ASSET_URL } },
      }),
  ]);

  const written = new Map<string, Uint8Array>();
  const results = await captureFixtures({
    manifest: { fixtures: [{ name: "v03-paint", fileKey: "KEYA" }] },
    client,
    dashc: await loadDashc(),
    readCapturedVersion: () => Promise.resolve(null),
    writeCapture: () => Promise.resolve(),
    writeImage: (name, imageRef, bytes) => {
      written.set(`${name}/${imageRef}`, bytes);
      return Promise.resolve();
    },
    fetchFn: (input) => {
      assertEquals(String(input), ASSET_URL);
      return Promise.resolve(new Response(png));
    },
  });

  assertEquals(results[0].action, "captured");
  assertEquals(written.size, 1, "the fixture's one image fill is captured");
  assertEquals(written.get(`v03-paint/${V03_PAINT_REF}`), png);
});
```

Add the import this test needs to the top of the file:

```ts
import { loadDashc } from "./wasm.ts";
```

Reading the corpus means `capture_test.ts` is no longer permission-free (its
header says every effect is injected). Update that header comment: the corpus
fixture and the wasm module are read from disk now, and the `test` task already
grants both.

- [ ] **Step 2: Run it and watch it fail**

Run: `cd importers/figma && deno task test src/capture_test.ts`
Expected: FAIL — `captureFixtures` takes no `dashc` or `writeImage` option.

- [ ] **Step 3: Capture the bytes**

In `importers/figma/src/capture.ts`, add to `CaptureFixturesOptions`:

```ts
/** Asked which refs the lowering demands, so capture and compile agree. */
readonly dashc: Dashc;
/** Writes one image fill's bytes into the corpus. */
readonly writeImage: (
  name: string,
  imageRef: string,
  bytes: Uint8Array,
) => Promise<void>;
/** Injectable for tests; used for the presigned asset downloads. */
readonly fetchFn?: typeof fetch;
```

and, in `captureFixtures`, immediately after the successful `writeCapture`:

```ts
      // The fixture's image fills, resolved to bytes. The presigned URL in the
      // map is regenerated per fetch, so committing it would rewrite the
      // fixture on every capture (issue #141) — the bytes are what is stable.
      const text = JSON.stringify(file, null, 2) + "\n";
      await writeCapture(name, text);

      const refs = dashc.figmaImageRefs(text);
      const images = await resolveImages({ client, fileKey, refs, fetchFn });
      for (const [imageRef, asset] of images) {
        await writeImage(name, imageRef, asset.bytes);
      }
      log(`${name}: captured version ${file.version}, ${images.size} image(s)`);
```

replacing the existing `writeCapture` + `log` pair. A fixture with no root
frame (a diagnostic fixture) has no refs to resolve: `figmaImageRefs` throws
`CompileFailed` for it, and the existing `catch` around the loop body already
turns that into `action: "failed"` — which would be wrong. Guard it:

```ts
const refs = (() => {
  try {
    return dashc.figmaImageRefs(text);
  } catch {
    // A fixture the lowering refuses outright (a diagnostic fixture, or
    // one whose vocabulary is not v0.3) still captures its JSON — the
    // suite needs it precisely because it does not compile. It just has
    // no image bytes to resolve.
    log(`${name}: no image fills resolved (the lowering refuses this file)`);
    return [];
  }
})();
```

In the `import.meta.main` block, pass the two new options:

```ts
dashc: await loadDashc(),
writeImage: async (name, imageRef, bytes) => {
  const dir = new URL(`${name}.images/`, corpusDir);
  await Deno.mkdir(dir, { recursive: true });
  await Deno.writeFile(new URL(`${imageRef}.png`, dir), bytes);
},
```

and add the imports:

```ts
import { resolveImages } from "./images.ts";
import { type Dashc, loadDashc } from "./wasm.ts";
```

- [ ] **Step 4: Widen the capture task's permissions**

In `importers/figma/deno.json`:

```json
"capture": "deno run --allow-env=FIGMA_TOKEN --allow-net=api.figma.com,s3-alpha-sig.figma.com --allow-read=../../corpus/figma-fixtures,../../target/wasm32-unknown-unknown/release --allow-write=../../corpus/figma-fixtures src/capture.ts"
```

The asset host is not `api.figma.com`. If Figma serves a presigned URL from a
different host, the capture fails on a Deno permission error naming it — loud,
and one line to fix here.

- [ ] **Step 5: Run the tests**

Run: `cd importers/figma && deno task test`
Expected: PASS — every suite.

- [ ] **Step 6: Lint and commit**

```bash
cd importers/figma && deno task fmt && deno task lint && deno task check
cd ../.. && git add importers/figma
git commit -m "feat(importers): capture a fixture's image-fill bytes"
```

---

### Task 11: CHECKPOINT — capture the real asset

This is the one step an agent cannot do: it needs a Figma token.

- [ ] **Step 1: Ask the human to capture**

```bash
export FIGMA_TOKEN=<a PAT with file_content:read, file_metadata:read, library_content:read>
just deno-capture
```

Expected: `v03-paint: captured version <n>, 1 image(s)`, and a modified
`corpus/figma-fixtures/v03-paint.images/390616a0e7321eddb464388366d9a2a1bcb7f4c3.png`.

- [ ] **Step 2: Regenerate the golden from the real bytes**

```bash
UPDATE_GOLDENS=1 cargo test -p dashc --test figma_lowering the_fixture_emits_the_golden_dsb
cargo test -p dashc
cd importers/figma && deno task test
```

Expected: both suites pass with no code change. The golden is the only thing
that moved — which is the whole point of generating it from a stand-in first.

- [ ] **Step 3: Drop the stand-in note**

In `corpus/figma-fixtures/v03-paint.images/README.md`, replace the
"A stand-in, not the captured asset" section with what the real asset is:

```markdown
## 390616a0e7321eddb464388366d9a2a1bcb7f4c3.png

The image fill on `v03-paint.json`'s image-fill node, captured from Figma.
```

- [ ] **Step 4: Commit**

```bash
git add corpus/figma-fixtures/v03-paint.images goldens/dsb/v03-paint.dsb
git commit -m "corpus(corpus): capture the v03-paint image fill"
```

**If the capture cannot be run** (no token available): stop and say so. Do not
fake the bytes. The stand-in stays, the README keeps saying it is a stand-in, a
`debt`-labeled issue records that the fixture's asset is synthetic, and the PR
description says the same. An honest gap beats a fabricated fixture.

---

### Task 12: Documentation

**Files:**

- Modify: `AGENTS.md`
- Modify: `specs/SCOPE_DECISIONS.md`
- Modify: `importers/figma/README.md` if one exists; otherwise skip

- [ ] **Step 1: Record the new recipes**

In `AGENTS.md`, under "Commands", the `deno-*` line becomes:

```text
just deno-check   just deno-test   just deno-fmt   just deno-capture
                  — scoped to importers/figma/
```

- [ ] **Step 2: Record the ABI in SCOPE_DECISIONS**

Append `## 21. The dashc wasm ABI is hand-written and pinned` to
`specs/SCOPE_DECISIONS.md` (§20 is the last one today). It records: the ABI is
hand-written rather than wasm-bindgen, and why in one paragraph; the five
exports and the wire version; that `dashc_wasm.wasm`, not `dashc.wasm`, is the
module the importer loads, and that `just wasm` builds `--lib` so the decoy is
not produced; that story #17 owns `imageRef` resolution, and `dashc` is _asked_
which refs it needs rather than the importer scanning for them; and that the
`deno` CI job now runs on Rust changes, because it is what checks the ABI.
Cross-reference `docs/decisions/dashc-wasm-abi.md`, which the gardening step
(Task 13) writes.

- [ ] **Step 3: Lint and commit**

```bash
dprint fmt && dprint check
markdownlint '**/*.md' --ignore target --ignore node_modules
git add AGENTS.md specs/SCOPE_DECISIONS.md
git commit -m "docs(docs): record the dashc wasm ABI"
```

---

### Task 13: The full gate

- [ ] **Step 1: Run what CI runs**

```bash
just build
just wasm
cd importers/figma && deno task check && deno task lint && deno task fmt --check && deno task test
```

Expected: all green. `just build` is `assemble + test + lint + audit`.

- [ ] **Step 2: Prove the acceptance criterion from a clean slate**

The story's criterion is "fixture → `.dsb` byte-identical to dashc-native
output for the same input". Verify it end to end, not by trusting the tests:

```bash
rm -rf target/wasm32-unknown-unknown
just wasm
cd importers/figma
deno eval '
  import { loadDashc } from "./src/wasm.ts";
  const dashc = await loadDashc();
  const json = Deno.readTextFileSync("../../corpus/figma-fixtures/v03-paint.json");
  const ref = "390616a0e7321eddb464388366d9a2a1bcb7f4c3";
  const bytes = Deno.readFileSync(`../../corpus/figma-fixtures/v03-paint.images/${ref}.png`);
  const out = dashc.compileFigma(json, "core", new Map([[ref, { format: "png", bytes }]]));
  const golden = Deno.readFileSync("../../goldens/dsb/v03-paint.dsb");
  console.log("wasm bytes:", out.bytes.length, "golden:", golden.length);
  console.log("identical:", out.bytes.every((b, i) => b === golden[i]) && out.bytes.length === golden.length);
'
```

Expected: `identical: true`.

- [ ] **Step 3: Garden, review, and open the PR**

Per `AGENTS.md`: garden `docs/wip/` into durable records with the
`sdd-gardening` skill (the ABI is a `docs/decisions/` record — it is normative
and binds #37), open the PR as a **draft**, run `/code-review` on it, capture
every finding as a checklist in the PR description, fix the critical ones, file
one `debt` issue per minor one, and mark it ready only when CI is green.
