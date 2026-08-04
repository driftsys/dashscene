# The demo names its painter on screen

The showcase host draws the same document with either painter and swaps
between them on a key. Which painter drew a given frame is reported only
to stderr, so a screenshot, a screen recording, or a window someone walked
up to carries no evidence of it. This adds an on-screen label that names
the painter.

## What exists today

`Present::name` already builds the string. `SkiaPresenter` returns a
literal, `dashscene-skia (CPU raster, softbuffer blit)`; `GpuPresenter`
builds its own at construction from the adapter, the wgpu backend and the
swapchain format, because none of the three is known until a device
exists (`demo/src/present.rs`).

The host prints that string twice: once at startup
(`demo/src/shell.rs`, `resumed`) and again whenever the `P` key swaps the
painter (`swap_painter`). The window title is a fixed `&'static str`,
`dashscene — showcase` or `dashscene — document`, chosen in
`demo/src/main.rs` and never changed after the window opens.

## Decisions taken

**The label is drawn into the frame, not written into the window title.**
A title-bar label costs less and stays outside the picture, but it does
not survive a screenshot, which is the case this exists for.

Two consequences follow from that choice and are accepted:

- The label is drawn **by the painter under test**. It is therefore part
  of the picture the two painters are being compared on, and it renders
  through each painter's own text path rather than through a neutral one.
- Content is authored for the demonstration that is not part of any
  scene's subject matter.

**The label carries the painter's crate name only** — `dashscene-skia` or
`dashscene-gpu`. The adapter, the wgpu backend and the swapchain format
stay in the stderr line, where they already are. Carrying them on screen
would mean handing a runtime string from the host to the scene through
process-wide state, because the scene is built before those values are
known.

**The `--dsb` run carries no label.** `demo/src/document.rs` injects a
bare `TaffySolver`, which holds no typesetter, so a text node in that
arena stages no glyph runs. That run also replays a compiled document,
and authoring a host-owned root into its arena would mix host content
into the document under test. The signal lookup finds nothing there and
the host writes nothing, so this needs no special case. The stderr line
still names the painter for that run.

## Design

### The badge is a second root, owned by the scenes

`Scene::roots` takes a list (`crates/dashlang/src/reactive.rs`). The badge
is appended as a second root rather than added to the scene's tree, so it
never participates in the scene's layout and it paints after the content.

No scene or test in the repository used more than one root before this, so
the behaviour was measured rather than assumed. Two roots stage as two
arena roots in list order, each solved independently — a second root
honours its own `Node::at` offset and overlaps the first rather than
stacking below it — and the second root's rect is last in the committed
table, which is what puts it above the content.

It is owned by `corpus/showcase/`, not by `demo/`, because the glyphs are
staged by the solver the scene injects, and only `ShowcaseSolver` carries
a typesetter and the atlas list. All three showcase scenes — `surfaces`,
`typography` and `layout` — already inject one, so the badge shapes in
every one of them. The label is printable ASCII, which the committed
`corpus/atlas/inter-ascii` bundle already covers, so no atlas bake is
added.

A new module, `corpus/showcase/src/badge.rs`, exposes one function that
declares the signal and returns the positioned node:

```rust
pub fn badge(scene: &mut Scene, width: f32, height: f32) -> Node
```

Each scene calls it and passes both roots:

```rust
let badge = badge::badge(&mut scene, width, height);
scene.roots([root, badge]);
```

### One named scalar signal drives it

`showcase::BACKEND` is a named scalar signal with initial value `0.0`.

| Value | Text             | Badge opacity |
| ----- | ---------------- | ------------- |
| `0.0` | empty string     | `0.0`         |
| `1.0` | `dashscene-skia` | `1.0`         |
| `2.0` | `dashscene-gpu`  | `1.0`         |

The badge root carries two bindings, both on that one signal, and both
built with `Signal::map`:

- `bind_text` over a `Mapped<String>` closure, which is the single place
  the two names are written.
- `bind(Channel::Opacity, ...)` over a `Mapped<f32>` closure returning
  `1.0` for any positive value and `0.0` otherwise. `Channel::Opacity` is
  group opacity and paint-only
  (`crates/dashscene-core/src/bindings.rs`), so raising or lowering the
  badge reflows nothing.

Both are closures rather than the declarative transforms because
`Signal::map_range` and `Signal::clamp` are methods on `Signal<f32>` and
return a `ScalarExpr` that carries neither, so they do not compose into a
clamped remap.

A scalar signal is used rather than a boolean because
`LiveScene::signal_named` returns `Signal<f32>` only, and the host looks
the signal up by name in a scene it did not build.

`Scene::build_live` seeds every bound prop by evaluating its transform
against the signal's initial value, so the badge is committed empty and at
zero opacity by the build itself — no first-frame flash, and nothing to
reset.

**The `0.0` default is what keeps the badge out of the committed still.**
`docs/images/showcase-surfaces.png` is the image in the repository README,
produced by `cargo run -p showcase --example still`. That example never
writes the signal, so the badge stays at zero opacity and the committed
PNG is unchanged.

### Appearance

A pill in the top-left corner: `Node::at` for the offset, a
`palette::PANEL` fill, a corner radius, padding, and near-white Inter
SemiBold text vertically centred. Both the offset and the pill's extent
are derived from the drawable extent the function is given, as every other
showcase measurement is, rather than from any scene's own margin — the
badge is the same size in every scene.

### Host wiring

Two additions to `demo/`:

- `Choice::badge_value(self) -> f32` in `demo/src/painter.rs`, returning
  `1.0` for `Choice::Skia` and `2.0` for `Choice::Gpu`. It sits beside
  `Choice` so the two painters cannot be given values in two places.
- `Host::announce_painter` in `demo/src/shell.rs`, which looks up
  `showcase::BACKEND` through `LiveScene::signal_named` and writes the
  current painter's value. A scene that declares no such signal produces
  no write, which is what makes the `--dsb` run need no special case.

It is called from `Host::rebuild`, which covers the first frame, a resize
and a scene advance, and at the end of `Host::swap_painter`.

Writing the signal on a swap is also what makes that frame re-solve: the
write marks a binding dirty, so `LiveScene::tick` commits through the
scene's solver and stages the new label's glyph run. No rebuild is
required, so the property `swap_painter` was built for — that what changes
on screen between the two painters is the painters and not two separate
runs — is kept.

## Verification

The seam that can rot is between two crates: `demo` decides `1.0` and
`2.0`, and `corpus/showcase` decides what those values mean. A test
asserts the two agree, and is mutation-tested by changing one value and
confirming the assertion fails.

- The text closure maps `0.0`, `1.0` and `2.0` to the empty string,
  `dashscene-skia` and `dashscene-gpu`.
- The opacity expression maps `0.0` to `0.0`, and both `1.0` and `2.0` to
  `1.0`.
- The badge is the last root of each scene, which is what puts it above
  the content.
- `Choice::Skia` and `Choice::Gpu` produce distinct values, and each
  matches the text the showcase closure returns for it.

Checked by hand, because neither is a property a test asserts:

- `cargo run -p demo -- surfaces`, then `P`, shows the label changing
  from one painter's name to the other's without the scene restarting.
- `cargo run -p showcase --example still -- surfaces <path>` produces a
  PNG with no badge in it.

The diff touches `demo/` and `corpus/showcase/` only, neither of which is
in the `packer` path filter, so the tiers are `just test` while editing
and `just build` before opening the pull request.

## Alternatives considered

**A window-title label.** Compose the title from the scene name and the
painter name and call `Window::set_title` on startup, on a swap and on a
scene advance. Smaller, confined to `demo/src/shell.rs`, painter-independent,
and legible whatever subset the GPU painter has reached. Rejected because
the label does not appear in a screenshot, which is the case this work
exists to serve.

**Passing the painter name into `SceneBuilder`.** Changing the builder
signature to take the name would put the real runtime string on screen
with no shared state. Rejected on two counts: the signature is consumed by
`corpus/showcase`, the `still` example, `demo/src/document.rs`,
`demo/src/shell.rs` and `demo/src/main.rs`, and a painter swap would have
to rebuild the scene to refresh the label — which is exactly what
`swap_painter` avoids so that a swap shows a difference between painters
rather than between runs.

**The host authoring the badge through the producer API.** The host owns
the arena and `Txn::add_node` exists, so it could author the root itself.
Rejected because staging the glyph run needs `Txn::commit_with` and the
scene's solver, and `LiveScene` does not expose the solver it holds. A
commit without it stages no glyph runs at all, and the label would be
blank.

**A process-wide table holding the full presenter string.** Would put the
adapter and backend on screen without a signature change. Rejected as
shared mutable state bought for detail that the stderr line already
carries.
