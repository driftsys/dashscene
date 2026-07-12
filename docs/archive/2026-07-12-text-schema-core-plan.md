# Text Schema + Core (#26) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** text vocabulary in the document schema (strings pool + text
style pool + sentinel-indexed node fields) and in the arena
(`Prop::Text` / `Prop::TextStyle` + intent-side accessors).

**Architecture:** additive `dashbuf.fbs` evolution mirroring the paint
pool precedent; `dashscene-core` mirrors the shapes without linking
dashbuf, commit pipeline untouched. See
`docs/wip/2026-07-12-text-schema-core-design.md`.

**Tech Stack:** FlatBuffers (flatc via build.rs), plain Rust in
dashscene-core.

## Global Constraints

- Schema evolution is append-only: new fields only, after existing ones
  (R7); sentinel `uint32::MAX` = absent, matching `parent`/`paint_entry`.
- P1: no glyph data, no measured sizes anywhere in this story.
- Committed output and the commit pipeline stay byte-for-byte unchanged.
- `Prop` loses `Copy` (a `String` variant); it keeps `Clone`. Verify no
  caller depended on `Copy` (grep dashlang + tests).
- House style: doc comments citing DESIGN sections; commits
  conventional with scope `dashbuf` / `dashscene-core`;
  `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` trailer.

---

### Task 1: dashbuf — TextStyle table, pools, node fields, round-trips

**Files:**

- Modify: `crates/dashbuf/schema/dashbuf.fbs`
- Create: `crates/dashbuf/tests/text_roundtrip.rs`

**Interfaces:**

- Consumes: existing generated types (`Color`, `Node`, `Document`).
- Produces (generated): `TextStyle` table (`family: string (required)`,
  `size_px: f32`, `weight: u16 = 400`, `color: Color`);
  `Document.strings: [string]`, `Document.text_styles: [TextStyle]`;
  `Node.text: u32 = MAX`, `Node.text_style: u32 = MAX`.

- [ ] **Step 1: Write the failing round-trip tests**

`crates/dashbuf/tests/text_roundtrip.rs`:

```rust
//! Round-trips for the v0.5 text vocabulary (#26): strings pool, text
//! style pool, sentinel-indexed node fields. One focused test per
//! construct, matching paint_roundtrip.rs's style.

use dashbuf::generated::dashbuf as fb;
use flatbuffers::FlatBufferBuilder;

const NO_TEXT: u32 = u32::MAX;

/// Builds a document with `strings`, `text_styles`, and one node per
/// (text, text_style) pair.
fn build_doc(
    strings: &[&str],
    styles: &[(&str, f32, u16)],
    nodes: &[(u32, u32)],
) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let strings: Vec<_> = strings.iter().map(|s| b.create_string(s)).collect();
    let strings = b.create_vector(&strings);
    let styles: Vec<_> = styles
        .iter()
        .map(|(family, size, weight)| {
            let family = b.create_string(family);
            fb::TextStyle::create(
                &mut b,
                &fb::TextStyleArgs {
                    family: Some(family),
                    size_px: *size,
                    weight: *weight,
                    color: Some(&fb::Color::new(0.1, 0.2, 0.3, 1.0)),
                },
            )
        })
        .collect();
    let styles = b.create_vector(&styles);
    let nodes: Vec<_> = nodes
        .iter()
        .map(|(text, style)| {
            fb::Node::create(
                &mut b,
                &fb::NodeArgs {
                    text: *text,
                    text_style: *style,
                    ..Default::default()
                },
            )
        })
        .collect();
    let nodes = b.create_vector(&nodes);
    let doc = fb::Document::create(
        &mut b,
        &fb::DocumentArgs {
            nodes: Some(nodes),
            strings: Some(strings),
            text_styles: Some(styles),
            ..Default::default()
        },
    );
    b.finish(doc, None);
    b.finished_data().to_vec()
}

#[test]
fn text_node_reads_back_through_the_pools() {
    let bytes = build_doc(
        &["Speed", "km/h"],
        &[("Noto Sans", 16.0, 400)],
        &[(0, 0), (1, 0)],
    );
    let doc = fb::root_as_document(&bytes).expect("verifies");
    let nodes = doc.nodes().unwrap();
    let strings = doc.strings().unwrap();
    let styles = doc.text_styles().unwrap();
    let n0 = nodes.get(0);
    assert_eq!(strings.get(n0.text() as usize), "Speed");
    let style = styles.get(n0.text_style() as usize);
    assert_eq!(style.family(), "Noto Sans");
    assert_eq!(style.size_px(), 16.0);
    assert_eq!(style.weight(), 400);
    assert_eq!(style.color().unwrap().r(), 0.1);
    assert_eq!(strings.get(nodes.get(1).text() as usize), "km/h");
}

#[test]
fn two_nodes_can_share_one_interned_string() {
    let bytes = build_doc(&["OK"], &[("Noto Sans", 12.0, 700)], &[(0, 0), (0, 0)]);
    let doc = fb::root_as_document(&bytes).expect("verifies");
    let nodes = doc.nodes().unwrap();
    assert_eq!(nodes.get(0).text(), nodes.get(1).text());
}

#[test]
fn a_node_without_text_reads_the_sentinels_by_default() {
    let mut b = FlatBufferBuilder::new();
    let node = fb::Node::create(&mut b, &fb::NodeArgs::default());
    let nodes = b.create_vector(&[node]);
    let doc = fb::Document::create(
        &mut b,
        &fb::DocumentArgs {
            nodes: Some(nodes),
            ..Default::default()
        },
    );
    b.finish(doc, None);
    let bytes = b.finished_data().to_vec();
    let doc = fb::root_as_document(&bytes).expect("verifies");
    let n = doc.nodes().unwrap().get(0);
    assert_eq!(n.text(), NO_TEXT);
    assert_eq!(n.text_style(), NO_TEXT);
}

#[test]
fn weight_defaults_to_regular() {
    let mut b = FlatBufferBuilder::new();
    let family = b.create_string("Noto Sans");
    let style = fb::TextStyle::create(
        &mut b,
        &fb::TextStyleArgs {
            family: Some(family),
            ..Default::default()
        },
    );
    let styles = b.create_vector(&[style]);
    let doc = fb::Document::create(
        &mut b,
        &fb::DocumentArgs {
            text_styles: Some(styles),
            ..Default::default()
        },
    );
    b.finish(doc, None);
    let bytes = b.finished_data().to_vec();
    let doc = fb::root_as_document(&bytes).expect("verifies");
    assert_eq!(doc.text_styles().unwrap().get(0).weight(), 400);
}
```

(A family-less `TextStyle` cannot be constructed through the generated
safe API — `TextStyleArgs.family: None` panics in `create` for a
required field — so the "verifier rejects a family-less style" property
is enforced at build time for Rust producers and by the verifier for
foreign bytes; no test constructs invalid bytes by hand.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p dashbuf --test text_roundtrip`
Expected: compile error — `TextStyle`, `strings`, `text` unknown.

- [ ] **Step 3: Extend the schema**

In `crates/dashbuf/schema/dashbuf.fbs`, before `table Node`:

```fbs
// One entry of the document's text style pool (DESIGN §5's "dedup
// style pool" applied to text): how a text node renders. Never glyph
// data (P1) — family/size/weight/color are intent; shaping and
// placement happen in the runtime (DESIGN §7.2).
table TextStyle {
  // Font family name. Required: a family-less style has no meaning,
  // and the verifier rejects it at the load gate (P4) — the same
  // mechanism as Gradient's required fields.
  family: string (required);
  // Em size in document units.
  size_px: float32;
  // CSS-scale weight, 100..900.
  weight: ushort = 400;
  color: Color;
}
```

Append to `table Node` (after `paint_entry`):

```fbs
// Index into Document.strings, or NO_TEXT (uint32::MAX — the same
// sentinel convention as `parent`/`paint_entry`) for a node without
// text content.
text: uint32 = 4294967295;
// Index into Document.text_styles, or uint32::MAX for unstyled text
// (a diagnostic once text validation exists; never a silent default).
text_style: uint32 = 4294967295;
```

Append to `table Document` (after `paints`):

```fbs
// Interned string pool (DESIGN §5) referenced by Node.text. Dedup is
// the producer's job; the pool makes it representable.
strings: [string];
// The text style pool referenced by Node.text_style.
text_styles: [TextStyle];
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p dashbuf`
Expected: existing roundtrip + paint_roundtrip still green; the 4 new
text tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/dashbuf
git commit -m "feat(dashbuf): add the text vocabulary — strings pool, text style pool, node refs"
```

---

### Task 2: dashscene-core — TextStyle, Prop variants, accessors

**Files:**

- Modify: `crates/dashscene-core/src/arena.rs`
- Modify: `crates/dashscene-core/src/lib.rs` (re-export `TextStyle`)
- Modify: `crates/dashscene-core/tests/arena.rs` (append cases)

**Interfaces:**

- Consumes: existing `Arena`/`Txn`/`Prop`/`Color`.
- Produces:
  `pub struct TextStyle { pub family: String, pub size_px: f32, pub weight: u16, pub color: Color }`
  (Clone, Debug, PartialEq);
  `Prop::Text(String)`, `Prop::TextStyle(TextStyle)`;
  `Arena::text_of(NodeId) -> Option<&str>`,
  `Arena::text_style_of(NodeId) -> Option<&TextStyle>`;
  `Prop` derives become `Clone, Debug, PartialEq` (drops `Copy`).

- [ ] **Step 1: Write the failing tests**

Append to `crates/dashscene-core/tests/arena.rs`:

```rust
#[test]
fn text_props_set_and_read_back_through_the_intent_accessors() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let label = txn.add_node(None, Some("label"));
    txn.set_prop(label, Prop::Text("Speed".to_string()));
    txn.set_prop(
        label,
        Prop::TextStyle(TextStyle {
            family: "Noto Sans".to_string(),
            size_px: 16.0,
            weight: 400,
            color: Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 },
        }),
    );
    txn.commit();
    assert_eq!(arena.text_of(label), Some("Speed"));
    let style = arena.text_style_of(label).expect("style set");
    assert_eq!(style.family, "Noto Sans");
    assert_eq!(style.weight, 400);
}

#[test]
fn text_accessors_read_staged_intent_immediately() {
    // Intent-side semantics (the #28/#29 seam): staged values are
    // visible before commit, unlike committed().
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let n = txn.add_node(None, None);
    txn.set_prop(n, Prop::Text("pending".to_string()));
    drop(txn); // staged, never committed
    assert_eq!(arena.text_of(n), Some("pending"));
}

#[test]
fn text_props_replace_previous_values() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let n = txn.add_node(None, None);
    txn.set_prop(n, Prop::Text("old".to_string()));
    txn.set_prop(n, Prop::Text("new".to_string()));
    txn.commit();
    assert_eq!(arena.text_of(n), Some("new"));
}

#[test]
fn nodes_without_text_read_none() {
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let n = txn.add_node(None, None);
    txn.commit();
    assert_eq!(arena.text_of(n), None);
    assert!(arena.text_style_of(n).is_none());
}

#[test]
fn a_text_only_change_does_not_touch_the_rect_table() {
    // P1: text influences no v0.5 committed output; hug sizing is #29.
    let mut arena = Arena::new();
    let mut txn = arena.open();
    let n = txn.add_node(None, None);
    txn.set_prop(n, Prop::Width(10.0));
    txn.commit();
    let mut txn = arena.open();
    txn.set_prop(n, Prop::Text("hello".to_string()));
    txn.commit();
    assert!(arena.committed().dirty().is_empty());
}
```

Also add `TextStyle` to the existing `use dashscene_core::{...}` line.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p dashscene-core --test arena`
Expected: compile error — `TextStyle`, `Prop::Text` unknown.

- [ ] **Step 3: Implement in `arena.rs`**

Add after the `Prop` enum's current variants (and change the `Prop`
derive line from `Clone, Copy, Debug, PartialEq` to
`Clone, Debug, PartialEq` — a `String` variant cannot be `Copy`):

```rust
/// Set/replace the node's text content (DESIGN §5: strings, never
/// glyph positions — P1). v0.5: no effect on committed output;
/// text-driven hug sizing arrives with the measure-callback story.
Text(String),
/// Set/replace the node's text style.
TextStyle(TextStyle),
```

Add the style type (near `Prop`):

```rust
/// Text style intent — mirrors the `dashbuf` `TextStyle` table
/// (family, em size in document units, CSS-scale weight, color)
/// without linking the generated code.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub family: String,
    pub size_px: f32,
    pub weight: u16,
    pub color: Color,
}
```

`NodeData` gains:

```rust
text: Option<String>,
text_style: Option<TextStyle>,
```

(and the `NodeData` construction site in `add_node` gains
`text: None, text_style: None`).

`set_prop`'s match gains:

```rust
Prop::Text(s) => data.text = Some(s),
Prop::TextStyle(ts) => data.text_style = Some(ts),
```

`impl Arena` gains intent-side accessors:

```rust
    /// The node's text content, or `None` for a node without text.
    ///
    /// Reads the intent model: staged (uncommitted) values are visible
    /// immediately, unlike [`Arena::committed`]. This is the seam the
    /// typeset pipeline and the measure callback read from.
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena (same contract
    /// as [`Txn::set_prop`]).
    pub fn text_of(&self, node: NodeId) -> Option<&str> {
        self.node_data(node).text.as_deref()
    }

    /// The node's text style, or `None` when unstyled. Intent-side,
    /// like [`Arena::text_of`].
    ///
    /// # Panics
    ///
    /// Panics if `node` is out of range for this arena.
    pub fn text_style_of(&self, node: NodeId) -> Option<&TextStyle> {
        self.node_data(node).text_style.as_ref()
    }

    fn node_data(&self, node: NodeId) -> &NodeData {
        self.nodes
            .get(node.index())
            .unwrap_or_else(|| panic!("{node:?} is not a node of this arena"))
    }
```

`lib.rs`: extend the `pub use arena::{...}` line with `TextStyle`.

- [ ] **Step 4: Run the tests, clippy, and the dependent crates**

Run: `cargo test -p dashscene-core && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p dashlang`
Expected: all green — dashlang confirms nothing depended on `Prop: Copy`.

- [ ] **Step 5: Commit**

```bash
git add crates/dashscene-core
git commit -m "feat(dashscene-core): add text content + style intent (Prop::Text, Prop::TextStyle)"
```

---

### Task 3: story wrap-up (process)

- [ ] `just build` green.
- [ ] sdd-gardening (subagent): design record update-or-create
      (extend `docs/design/dashbuf.md` + `docs/design/dashscene-core-arena.md`
      in place — this story extends both components; per the sdd rule, edit
      existing records rather than new files), decision record only if a
      genuinely new decision surfaced, archive the wip pair.
- [ ] `/code-review` on the diff; findings → PR checklist; criticals
      fixed; `debt` issues for minors.
- [ ] Rebase on main, PR, CI green (including atlas-repro), merge,
      close #26, tick epic #24.
