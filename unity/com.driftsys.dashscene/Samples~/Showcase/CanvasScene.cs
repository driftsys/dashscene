// The faithful Canvas: a uGUI hierarchy built at load from the runtime's own
// resolved rect table, and driven per frame from the lease's dirty set.
//
// Story #1444, to the eight fairness rules of
// `docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`
// D2. What each rule costs is where it is implemented:
//
//  1. **Geometry is identical by construction.** `Build` walks `frame.Rects` —
//     the same `DsFrame` the painter packs — and places one `RectTransform` per
//     rect. No `LayoutGroup` runs anywhere in this file, so uGUI's own box model
//     never re-solves anything; a layout that differed would make the reading a
//     comparison of two scenes rather than of two renderers.
//  2. **One 9-sliced sprite per distinct shape**, from `CanvasSprites`, tinted
//     through `Image.color`. A rect whose fill and stroke are both static solid
//     colours is one `Image`; otherwise the fill and the stroke are two.
//  3. **Gradients are baked** at the node's pixel size, EVERY kind — where the
//     rule's letter bakes a linear one as a 256-texel strip stretched across the
//     `Image`. `CanvasSprites.BakeGradient` carries the argument: a stretched
//     strip cannot hold the node's rounded corners or its anti-aliased edge, so
//     a node with either would draw a hard square boundary, which is a different
//     picture and rule 1 forbids that. Baked at load, which is the PNG a team
//     ships moved to run time, and re-baked only when a row's stops differ from
//     the ones the element cached.
//  5. **`Apply` walks the dirty set and nothing else.** A Canvas that rewrote
//     every element each frame would be the painter's advantage, and rule 5 says
//     that advantage is not taken. `unity/package-gate`'s `canvas_baseline.rs`
//     holds this method to exactly one loop over `frame.Dirty`.
//  6. **The constructs the painter refuses are omitted here too**, and the count
//     reported beside the picture is the painter's OWN: `Build` runs a
//     `FramePacker` over the same frame and keeps its `PackDiagnostics`, rather
//     than counting refusals a second way that could disagree with the painter's
//     while both looked right.
//  7. **Nothing is culled.** Every rect the shown root holds becomes an element.
//  8. **`Isolate`** moves a rect the pulses dirty onto its own child `Canvas`,
//     once, from the first pulse that touches it.
//
// Rule 4, text, is `CanvasText` — and it places one object per text NODE where
// the rule says one per glyph RUN, because the producer reaches the text through
// the run's anchor rect and so answers the whole node's string. `PlaceText`
// carries that argument.
//
// Both departures are recorded in the decision record itself, under D2, so a
// reader of the rules meets them there rather than only here.
//
// **This file contains no `unsafe`.** A sample is copied into a consumer's
// `Assets/` and compiles into `Assembly-CSharp`, which does not allow unsafe
// code; `Runtime/FrameRows.cs` is the seam, and it is inside the package whose
// assembly definition does.

using System;
using System.Collections.Generic;
using Driftsys.Dashscene.BoundaryB;
using TMPro;
using UnityEngine;
using UnityEngine.UI;

// `UnityEngine` declares a `Gradient` and a `Color` of its own, so both names are
// ambiguous in a file that opens boundary B. The gradient is aliased because
// every use of it here is boundary B's; `Color` is not, because both are used
// and `Tinted` is the one place that converts between them.
using Gradient = Driftsys.Dashscene.BoundaryB.Gradient;

namespace Driftsys.Dashscene.Samples
{
    /// Where a document's coordinates land on the Canvas.
    ///
    /// The document's y runs down and `BrgPainter.DocumentToWorld` is the
    /// showcase's `Scale(1, -1, 1)`, so a document point `(x, y)` is the world
    /// point `(x, -y)`. The camera is orthographic, so one world unit is
    /// `pixelHeight / (2 * orthographicSize)` canvas units — the same scalar
    /// `DashsceneShowcase.UpdateEdgeWidth` inverts to get the painter's
    /// `EdgeWidth`. Taking it from the camera rather than assuming a document is
    /// built at the drawable's extent is what lets the same code place a scene
    /// (built at `Screen.width` by `Screen.height`, so the scalar is one) and a
    /// committed document (framed by `DemoBuild`'s own orthographic size).
    public readonly struct DocumentToCanvas
    {
        /// Canvas units per document unit.
        public readonly float Scale;

        /// The camera's world position, which the Canvas centres on.
        public readonly Vector2 CameraWorld;

        /// **`canvasHeight` of zero is refused, not scaled by.** Unity sizes a
        /// `ScreenSpaceCamera` canvas's own `RectTransform` during the canvas
        /// update rather than on `AddComponent`, so a caller reading `rect.height`
        /// in the same breath can read 0 — and a zero scale draws every element
        /// at zero size while `ElementCount`, `IsolatedCount`, the dirty-count
        /// pin and the `drew` line all read exactly right. That is the fail-open
        /// shape this repository keeps meeting, so it throws instead.
        ///
        /// # Exceptions
        ///
        /// [`ArgumentOutOfRangeException`] when `canvasHeight` is not above zero.
        public DocumentToCanvas(Camera camera, float canvasHeight)
        {
            if (!(canvasHeight > 0.0f))
            {
                throw new ArgumentOutOfRangeException(
                    nameof(canvasHeight),
                    canvasHeight,
                    "the canvas has no height yet, so every element would be placed and "
                    + "sized at zero while every count this scene reports read correct.");
            }

            Scale = camera.orthographicSize > 0.0f
                ? canvasHeight / (2.0f * camera.orthographicSize)
                : 1.0f;
            var position = camera.transform.position;
            CameraWorld = new Vector2(position.x, position.y);
        }

        /// The canvas position, relative to the Canvas's centre, of a document
        /// point.
        public Vector2 Of(float x, float y)
        {
            return (new Vector2(x, -y) - CameraWorld) * Scale;
        }
    }

    /// One document's rects as a uGUI hierarchy.
    public sealed class CanvasScene : IDisposable
    {
        /// Indexed by rect index, so the dirty set indexes it directly.
        private readonly List<Element> _elements = new List<Element>();

        private readonly Transform _root;
        private readonly CanvasSprites _sprites;
        private readonly DocumentToCanvas _map;

        /// Rule 4's half: the font assets and the run text.
        private CanvasText _text;

        /// The runs and their anchor rects, copied out of the frame so the text
        /// can be fetched once the lease has ended.
        private GlyphRun[] _runs = System.Array.Empty<GlyphRun>();
        private RectEntry[] _runAnchors = System.Array.Empty<RectEntry>();

        /// `_elements.Count`, for the readers outside `Apply`.
        ///
        /// `canvas_baseline.rs` refuses `_elements.Count`, `ElementCount` and
        /// this field alike inside `Apply`'s body, so the cache buys that method
        /// nothing — it exists for `ElementCount`, `PlaceText`'s bound and
        /// `Dispose`, which are the three readers that have a reason to ask.
        private int _count;

        /// How many times `Apply` has run. Read by `Isolate`, which the first
        /// commit's dirty set must not reach.
        private int _applies;

        /// How many rows the last `Apply` wrote.
        ///
        /// **Counted inside the loop, not assigned from the span's length before
        /// it.** Assigning `dirty.Length` up front made the pin that compares
        /// this against `frame.Dirty.CountAsLong` a tautology: the two sides were
        /// the same number however the loop behaved, so emptying the loop body
        /// left the pin green — which is the regression the pin is named for, a
        /// Canvas that draws the right picture for every frame after the first
        /// and costs nothing to do it.
        ///
        /// The `compare` action asserts this equals `frame.Dirty.CountAsLong` on
        /// a pulse frame. At rest the loop skips the frame entirely, so this is
        /// the previous commit's count and the SKIP is the rest signal — an idle
        /// tick commits nothing, so the last commit's dirty rows stay on the
        /// frame and an empty set would say nothing.
        public int AppliedLastFrame { get; private set; }

        /// How many elements the scene holds. One per rect, rule 7.
        public int ElementCount => _count;

        /// How many rects were isolated onto their own child `Canvas`, rule 8.
        public int IsolatedCount { get; private set; }

        /// How many glyph runs the Canvas could not draw. Rule 4's own refusals,
        /// reported beside the painter's.
        public int TextRefused => _text == null ? 0 : _text.Refused;

        /// What the PAINTER refused on this same frame.
        ///
        /// Read from a `FramePacker` over the same tables rather than counted
        /// again here, so the `drew` line's refusal set is the painter's own and
        /// cannot drift from it — which is what issue #1444's second "Done when"
        /// asks for.
        public PackDiagnostics Diagnostics { get; private set; }

        private CanvasScene(Transform root, CanvasSprites sprites, DocumentToCanvas map)
        {
            _root = root;
            _sprites = sprites;
            _map = map;
        }

        /// Builds the hierarchy for one committed frame. Runs at load.
        public static CanvasScene Build(
            DsFrame frame,
            Transform root,
            CanvasSprites sprites,
            DocumentToCanvas map,
            TextAtlasSet atlases)
        {
            var scene = new CanvasScene(root, sprites, map);

            var probe = new FramePacker();
            probe.Pack(frame, MaterialClass.UnlitOverlay, atlases);
            scene.Diagnostics = probe.Diagnostics;

            var rects = FrameRows.Of<RectEntry>(frame.Rects);
            var entries = FrameRows.Of<PaintEntry>(frame.PaintEntries);
            for (var i = 0; i < rects.Length; i++)
            {
                // **A rect naming a row past the table is skipped, not thrown
                // on.** `FramePacker.PackRect` refuses exactly this and keeps
                // packing the rest, reporting one corrupt row — so a Canvas that
                // threw here would abort the whole entry where the painter draws
                // the frame minus one rect, and rule 6's "omitted on both sides"
                // would be an abort on one of them.
                if (rects[i].Paint >= (uint)entries.Length)
                {
                    scene._elements.Add(new Element { Index = i });
                    continue;
                }

                scene._elements.Add(scene.Make(i, rects[i], entries[(int)rects[i].Paint], frame));
            }
            scene._count = scene._elements.Count;
            scene.CopyGlyphRuns(frame);
            return scene;
        }

        /// The glyph runs, copied out of the borrowed tables.
        ///
        /// **Copied rather than read where they are used, and the reason is the
        /// lease.** `ds_demo_run_text` reaches the arena through
        /// `dashscene-ffi`'s demonstration seam, and every call on that seam is
        /// refused while a frame lease is outstanding — the views the lease
        /// handed out would stop being valid under it. So the rows this needs are
        /// taken inside the lease and the text is fetched after it ends, which is
        /// what `PlaceText` is separate for.
        private void CopyGlyphRuns(in DsFrame frame)
        {
            var runs = FrameRows.Of<GlyphRun>(frame.GlyphRuns);
            var rects = FrameRows.Of<RectEntry>(frame.Rects);
            _runs = runs.ToArray();
            _runAnchors = new RectEntry[_runs.Length];
            for (var r = 0; r < _runs.Length; r++)
            {
                var anchor = (int)_runs[r].Rect;
                _runAnchors[r] = anchor < rects.Length ? rects[anchor] : default;
            }
        }

        /// Rule 4: one TextMeshPro object per text node, at the node's own box,
        /// typeset by TextMeshPro. **Called after the lease has ended**, because
        /// the producer's seam is refused while one is outstanding.
        ///
        /// **Per NODE, where the rule says per RUN, and the difference is what
        /// the producer can answer.** `ds_demo_run_text` reaches the text
        /// through the run's anchor rect and the arena, so what it returns is the
        /// whole node's string — a node the shaper split into two runs, one per
        /// face, would otherwise get that whole string twice, once per run,
        /// drawn over itself. Placing one object per node and letting
        /// TextMeshPro's own cascade handle the split is both the correct picture
        /// and what a Unity team ships; the run is still what names the face and
        /// the size.
        ///
        /// **The node's box, not the run's baseline.** The solver already placed
        /// the text node, so handing TextMeshPro that box and its own alignment
        /// is rule 4's "TextMeshPro's own typesetting" — where reconstructing a
        /// baseline from the first quad would be this host typesetting instead,
        /// and would make the reading a comparison of two placements rather than
        /// of two typesetters.
        public void PlaceText(CanvasText text)
        {
            _text = text;
            for (var r = 0; r < _runs.Length; r++)
            {
                var run = _runs[r];
                var anchor = (int)run.Rect;
                if (anchor >= _count || _elements[anchor].Text != null)
                {
                    continue;
                }

                var source = _text.TextOf(r);
                if (source == null)
                {
                    continue;
                }

                var font = _text.FontFor((int)run.Atlas);
                if (font == null)
                {
                    continue;
                }

                _elements[anchor].Text =
                    MakeText(_elements[anchor], run, _runAnchors[r], source, font);
            }

            _runs = System.Array.Empty<GlyphRun>();
            _runAnchors = System.Array.Empty<RectEntry>();
        }

        private TMP_Text MakeText(
            Element element,
            in GlyphRun run,
            in RectEntry rect,
            string text,
            TMP_FontAsset font)
        {
            var host = new GameObject("text", typeof(RectTransform), typeof(TextMeshProUGUI));
            var transform = (RectTransform)host.transform;
            transform.SetParent(element.Rect != null ? element.Rect : _root, false);

            if (element.Rect != null)
            {
                transform.anchorMin = Vector2.zero;
                transform.anchorMax = Vector2.one;
                transform.offsetMin = Vector2.zero;
                transform.offsetMax = Vector2.zero;
            }
            else
            {
                // A rect this baseline built no element for — a refused
                // construct — still anchors a run, so the object is placed from
                // the rect table directly rather than dropped.
                transform.anchorMin = Centre;
                transform.anchorMax = Centre;
                transform.pivot = Centre;
                transform.sizeDelta = new Vector2(rect.W, rect.H) * _map.Scale;
                transform.anchoredPosition =
                    _map.Of(rect.X + (rect.W * 0.5f), rect.Y + (rect.H * 0.5f));
            }

            var label = host.GetComponent<TextMeshProUGUI>();
            label.font = font;
            label.text = text;
            label.fontSize = run.Size * _map.Scale;
            label.color = Tinted(run.Color, run.Opacity);
            label.alignment = TextAlignmentOptions.TopLeft;
            label.raycastTarget = false;
            label.enableWordWrapping = true;
            label.overflowMode = TextOverflowModes.Overflow;
            return label;
        }

        /// Per frame: the dirty rows only.
        ///
        /// A careful Unity developer diffing their own state would touch exactly
        /// these. Rule 5.
        public void Apply(DsFrame frame)
        {
            var rects = FrameRows.Of<RectEntry>(frame.Rects);
            var entries = FrameRows.Of<PaintEntry>(frame.PaintEntries);
            var dirty = FrameRows.Of<uint>(frame.Dirty);
            AppliedLastFrame = 0;
            _applies++;
            for (var d = 0; d < dirty.Length; d++)
            {
                var i = (int)dirty[d];
                Isolate(_elements[i]);
                Place(_elements[i], rects[i]);
                Tint(_elements[i], rects[i], PaintOf(rects[i], entries), frame);
                AppliedLastFrame++;
            }
        }

        /// The rect's paint entry, or the entry that draws nothing.
        ///
        /// The bound `FramePacker.PackRect` applies before it follows a rect's
        /// paint index, kept out of `Apply`'s own body so the loop there stays
        /// the one the gate reads.
        private static PaintEntry PaintOf(in RectEntry rect, ReadOnlySpan<PaintEntry> entries)
        {
            return rect.Paint < (uint)entries.Length ? entries[(int)rect.Paint] : default;
        }

        /// Rule 8: a rect the pulses move gets its own child `Canvas`, once.
        ///
        /// **`overrideSorting` stays OFF**, which is what keeps the picture's
        /// order. A nested `Canvas` is its own batch either way — that is the
        /// isolation rule 8 asks for — but overriding the sort lifts it out of
        /// the parent's order into one of its own, and the root canvas sits at
        /// zero: giving each isolated element its own rect index as a sorting
        /// order therefore drew every isolated rect ABOVE every rect still
        /// batched in the root, which is the inversion this comment used to
        /// claim it prevented. Left off, the child sorts with its parent and
        /// hierarchy order — which `Build` writes in the document's own order —
        /// decides, as it does for every element that is not isolated.
        ///
        /// **Not on the first `Apply`, and that is the rule rather than an
        /// optimisation.** Rule 8 isolates a rect "from the first PULSE that
        /// dirties it". The first commit a host sees reports every rect dirty —
        /// it is the commit that produced them — so isolating on it would put
        /// every element of the scene on its own child `Canvas`, which is many
        /// batches where a careful team would have one. Measured on the first
        /// judged run: `typography` isolated 16 of its 16 elements. That is not
        /// the advantage rule 8 describes, it is the opposite of it, and rule 8
        /// exists to give the Canvas its advantages rather than to load it with
        /// costs.
        private void Isolate(Element element)
        {
            if (_applies <= 1 || element.Isolated || element.Rect == null)
            {
                return;
            }

            element.Isolated = true;
            IsolatedCount++;
            element.Rect.gameObject.AddComponent<Canvas>().overrideSorting = false;
        }

        private Element Make(int index, in RectEntry rect, in PaintEntry entry, in DsFrame frame)
        {
            var element = new Element { Index = index };

            // Rule 6, and the painter's own order of refusal: a baked vector
            // field's silhouette is not its box, so drawing the box would be a
            // plausible wrong picture — `FramePacker.PackRect` returns there and
            // so does this. A shadow or a blur is reported and the node's own
            // ink still draws, which is also what the packer does.
            if (entry.Shape.Count > 0 || entry.Stroke.Count > 1)
            {
                return element;
            }

            var host = new GameObject($"rect {index}", typeof(RectTransform));
            element.Rect = (RectTransform)host.transform;
            element.Rect.SetParent(_root, false);
            element.Rect.anchorMin = Centre;
            element.Rect.anchorMax = Centre;

            // **Everything a sprite is baked from is converted to canvas units
            // here, once.** `CanvasSprites` works in texels and one texel is one
            // canvas unit, while a rect's radii and its stroke width are
            // document units — equal only when the document is built at the
            // drawable's own extent, which a scene is and a committed document
            // is not.
            var stroke = entry.Stroke.Count > 0
                ? FrameRows.Of<Stroke>(frame.Strokes)[(int)entry.Stroke.Offset]
                : default;
            element.StrokeWidth = entry.Stroke.Count > 0 ? stroke.Width * _map.Scale : 0.0f;
            element.StrokeAlign = entry.Stroke.Count > 0 ? stroke.Align : StrokeAlign.Inside;
            element.Corners = new CornerRadii
            {
                TopLeft = entry.Corners.TopLeft * _map.Scale,
                TopRight = entry.Corners.TopRight * _map.Scale,
                BottomRight = entry.Corners.BottomRight * _map.Scale,
                BottomLeft = entry.Corners.BottomLeft * _map.Scale,
            };

            element.Fill = MakeFill(element, entry, frame);
            if (entry.Stroke.Count > 0)
            {
                element.Ring = MakeImage(element, "stroke");
                element.Ring.sprite =
                    _sprites.Ring(element.Corners, element.StrokeWidth, element.StrokeAlign);
                element.Ring.type = Image.Type.Sliced;
            }

            Place(element, rect);
            Tint(element, rect, entry, frame);
            return element;
        }

        /// The fill `Image`, or null for a node with no fill this baseline draws.
        private Image MakeFill(Element element, in PaintEntry entry, in DsFrame frame)
        {
            switch (entry.Fill.Tag)
            {
                case PaintTag.None:
                    return null;
                case PaintTag.Image:
                    // Rule 6: the painter refuses an image fill, so this omits it.
                    return null;
                case PaintTag.Gradient:
                    element.GradientRow = (int)entry.Fill.Index;
                    var gradient = MakeImage(element, "gradient");
                    gradient.type = Image.Type.Simple;
                    return gradient;
                default:
                    element.GradientRow = -1;
                    var fill = MakeImage(element, "fill");
                    fill.sprite = _sprites.Fill(
                        element.Corners, element.StrokeWidth, element.StrokeAlign);
                    fill.type = Image.Type.Sliced;
                    return fill;
            }
        }

        /// One `Image` under the element, grown past the node box by the sprite's
        /// own padding — the stroke's outset plus the anti-aliasing band.
        ///
        /// Stretched to its parent rather than sized, so `Place` writes the node
        /// box once and both images follow it.
        private Image MakeImage(Element element, string name)
        {
            var host = new GameObject(name, typeof(RectTransform), typeof(Image));
            var transform = (RectTransform)host.transform;
            transform.SetParent(element.Rect, false);
            transform.anchorMin = Vector2.zero;
            transform.anchorMax = Vector2.one;
            transform.pivot = Centre;

            var pad = CanvasSprites.Pad(element.StrokeWidth, element.StrokeAlign);
            transform.offsetMin = new Vector2(-pad, -pad);
            transform.offsetMax = new Vector2(pad, pad);

            var image = host.GetComponent<Image>();
            image.raycastTarget = false;
            return image;
        }

        /// The node's box, its rotation and the point it turns about.
        private void Place(Element element, in RectEntry rect)
        {
            if (element.Rect == null)
            {
                return;
            }

            // `rotation_anchor` is in the rect's own space with `(0, 0)` at the
            // TOP-LEFT, and a uGUI pivot is normalised with `(0, 0)` at the
            // BOTTOM-left — so the y term is inverted rather than merely scaled.
            var pivot = new Vector2(
                rect.W > 0.0f ? rect.RotationAnchor.X / rect.W : 0.5f,
                rect.H > 0.0f ? 1.0f - (rect.RotationAnchor.Y / rect.H) : 0.5f);
            element.Rect.pivot = pivot;
            element.Rect.sizeDelta = new Vector2(rect.W, rect.H) * _map.Scale;
            element.Rect.anchoredPosition =
                _map.Of(rect.X + rect.RotationAnchor.X, rect.Y + rect.RotationAnchor.Y);

            // Negated: the document turns clockwise about a y-down axis, and a
            // uGUI z rotation turns counter-clockwise about a y-up one.
            element.Rect.localRotation =
                Quaternion.Euler(0.0f, 0.0f, -rect.Rotation * Mathf.Rad2Deg);
        }

        /// The node's colour, and a gradient re-baked only when its stops moved.
        private void Tint(Element element, in RectEntry rect, in PaintEntry entry, in DsFrame frame)
        {
            // **The entry decides, not the element.** A rect the scene hides keeps
            // its row and interns to the default paint entry — no fill, no
            // stroke — so branching on what `Make` built would leave the Image
            // on screen and tint it from row 0, which is another node's colour.
            // `layout`'s pulse hides a chip on every phase, so this is a wrong
            // picture on one of the three compared scenes rather than a
            // hypothetical.
            var hasFill = element.Fill != null && entry.Fill.Tag != PaintTag.None;
            if (element.Fill != null)
            {
                element.Fill.enabled = hasFill;
            }

            if (hasFill)
            {
                if (element.GradientRow >= 0)
                {
                    element.Fill.color = new UnityEngine.Color(1.0f, 1.0f, 1.0f, rect.Opacity);
                    RebakeGradientIfChanged(element, rect, frame);
                }
                else
                {
                    var solids = FrameRows.Of<BoundaryB.Color>(frame.Solids);
                    if (entry.Fill.Index < solids.Length)
                    {
                        element.Fill.color = Tinted(solids[(int)entry.Fill.Index], rect.Opacity);
                    }
                }
            }

            // The stroke's own half of the same rule, with the bound `Make`
            // applies and this did not: a row past the table is a corrupt frame,
            // which the painter reports and keeps drawing through.
            var strokes = FrameRows.Of<Stroke>(frame.Strokes);
            var hasRing = element.Ring != null
                && entry.Stroke.Count > 0
                && entry.Stroke.Offset < strokes.Length;
            if (element.Ring != null)
            {
                element.Ring.enabled = hasRing;
            }

            if (hasRing)
            {
                element.Ring.color =
                    Tinted(strokes[(int)entry.Stroke.Offset].Color, rect.Opacity);
            }
        }

        /// Boundary B's colour as `Image.color`, with the rect's own opacity
        /// folded in.
        ///
        /// **`Color` here is `UnityEngine.Color` and the argument is boundary
        /// B's**, and the two are different types with the same name — which is
        /// why this conversion is a named method rather than four field reads at
        /// each call site.
        private static UnityEngine.Color Tinted(BoundaryB.Color colour, float opacity)
        {
            return new UnityEngine.Color(colour.R, colour.G, colour.B, colour.A * opacity);
        }

        /// Rule 3's "re-bake only when the stops differ from the cached ones".
        ///
        /// An animated gradient re-bakes, as a real Canvas would have to; no
        /// showcase scene animates one, so on these scenes this is a comparison
        /// that answers "no" and returns.
        private void RebakeGradientIfChanged(Element element, in RectEntry rect, in DsFrame frame)
        {
            var gradient = FrameRows.Of<Gradient>(frame.Gradients)[element.GradientRow];
            var stops = FrameRows.Of<GradientStop>(frame.GradientStops);
            var count = Mathf.Min((int)gradient.Stops.Count, MaxStops);
            if (Unchanged(element, gradient, stops, count))
            {
                return;
            }

            _sprites.ReleaseGradient(element.Fill.sprite);
            element.Fill.sprite = _sprites.BakeGradient(
                gradient,
                stops,
                element.Corners,
                rect.W * _map.Scale,
                rect.H * _map.Scale,
                CanvasSprites.Pad(element.StrokeWidth, element.StrokeAlign));

            element.BakedKind = gradient.Kind;
            element.BakedStops = new GradientStop[count];
            for (var s = 0; s < count; s++)
            {
                element.BakedStops[s] = stops[(int)gradient.Stops.Offset + s];
            }
        }

        /// Whether the element's baked gradient is still the one this row asks
        /// for.
        ///
        /// **The kind and every stop, not the stop count.** Two ramps of the
        /// same length are the same length however far apart their colours are,
        /// and a signal that moved one stop's colour is exactly what an animated
        /// gradient is.
        private static bool Unchanged(
            Element element,
            in Gradient gradient,
            ReadOnlySpan<GradientStop> stops,
            int count)
        {
            if (element.BakedStops == null
                || element.BakedStops.Length != count
                || element.BakedKind != gradient.Kind)
            {
                return false;
            }

            for (var s = 0; s < count; s++)
            {
                var was = element.BakedStops[s];
                var now = stops[(int)gradient.Stops.Offset + s];
                if (was.Offset != now.Offset
                    || was.Color.R != now.Color.R
                    || was.Color.G != now.Color.G
                    || was.Color.B != now.Color.B
                    || was.Color.A != now.Color.A)
                {
                    return false;
                }
            }

            return true;
        }

        /// Destroys the hierarchy, and **no sprite and no texture**.
        ///
        /// Every sprite an element points at belongs to `CanvasSprites`: a shape
        /// sprite is shared by every rect of that shape, so destroying the ones
        /// an element holds would destroy each shared sprite once per rect and
        /// again when that object is disposed. The one unshared sprite, a
        /// gradient bake, is released through `CanvasSprites.ReleaseGradient`
        /// when it is re-baked and destroyed by that object otherwise.
        public void Dispose()
        {
            foreach (var element in _elements)
            {
                if (element.Rect != null)
                {
                    UnityEngine.Object.Destroy(element.Rect.gameObject);
                }
            }
            _elements.Clear();
            _count = 0;
        }

        private static readonly Vector2 Centre = new Vector2(0.5f, 0.5f);

        /// The bound the shader library clamps to.
        ///
        /// **The package's own constant, not a literal.**
        /// `sdf_hlsl_is_generated.rs` already holds `PaintHeap.MaxGradientStops`
        /// against `MAX_GRADIENT_STOPS` in the generated `Sdf.hlsl` — the
        /// library `SpriteBake.shader` includes — so reading it here puts this
        /// file inside that gate instead of beside it.
        private const int MaxStops = PaintHeap.MaxGradientStops;

        /// One rect's uGUI objects and the bake state they cache.
        private sealed class Element
        {
            internal int Index;
            internal RectTransform Rect;
            internal Image Fill;
            internal Image Ring;
            internal TMP_Text Text;
            internal bool Isolated;
            internal float StrokeWidth;
            internal StrokeAlign StrokeAlign;
            internal CornerRadii Corners;

            /// The gradient row this element's fill is baked from, or -1.
            internal int GradientRow = -1;

            /// The stops the current bake was made from, so an animated gradient
            /// re-bakes and a static one does not.
            internal GradientStop[] BakedStops;

            internal GradientKind BakedKind;
        }
    }
}
