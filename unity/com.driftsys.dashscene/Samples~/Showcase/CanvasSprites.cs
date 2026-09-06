// The faithful Canvas's sprite cache: one 9-sliced sprite per distinct shape,
// baked once at load on the GPU.
//
// Story #1444. Rule 2 of the fairness rules
// (`docs/decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`
// D2): solid fills, rounded corners and static strokes are one 9-sliced sprite
// per distinct shape — corner radius, stroke width, stroke alignment —
// rasterised once at load at the shape's pixel size and tinted through
// `Image.color`. A team shipping this design by hand would export those PNGs
// from Figma; baking them at load is that build step moved to run time, and rule
// 3 says the same of the gradients. **Nothing in this file runs per frame.**
//
// **The distance function is not re-implemented here.** `Dashscene/Samples/
// SpriteBake` includes the package's generated `Sdf.hlsl`, which `just sdf-hlsl`
// compiles from the one WGSL both painters evaluate (R-T5). That shader carries
// the argument for its own placement outside the package.
//
// **What a key quantises, and why the bake uses the quantised value.** Radii and
// stroke widths are keyed to a quarter of a texel. Baking from the ORIGINAL
// value instead would make a sprite depend on which shape reached the cache
// first — two nodes a sixteenth of a texel apart would share whichever was
// baked, so the picture would depend on document order. Quantising both the key
// and the bake removes that: a key names exactly one sprite.

using System;
using System.Collections.Generic;
using Driftsys.Dashscene.BoundaryB;
using UnityEngine;

// `UnityEngine.Gradient` is a different type with the same name, and every
// gradient this file bakes is boundary B's.
using Gradient = Driftsys.Dashscene.BoundaryB.Gradient;

namespace Driftsys.Dashscene.Samples
{
    /// The shapes a scene's rects need, baked once and shared.
    public sealed class CanvasSprites : IDisposable
    {
        /// The width, in texels, over which a baked edge ramps.
        ///
        /// One texel, which is the painter's `EdgeWidth` under the demo's own
        /// camera placement — `DashsceneShowcase.UpdateEdgeWidth` sets it to
        /// `orthographicSize * 2 / pixelHeight`, one device pixel in document
        /// units, and a scene is built at the drawable's own extent so that is
        /// one. A sprite baked with a different band is a different shape, which
        /// is why story #1444's mutation moves this and watches the comparison.
        public const float Aa = 1.0f;

        /// Quarter-texel steps: enough that two shapes a reader would call the
        /// same share a sprite, fine enough that two a reader would call
        /// different do not.
        private const float KeyStep = 4.0f;

        private readonly Material _bake;
        private readonly Dictionary<Key, Sprite> _cache = new Dictionary<Key, Sprite>();
        private readonly List<Texture2D> _textures = new List<Texture2D>();

        /// The per-node gradient bakes, which are not shared and so are the only
        /// sprites a caller may release before this object is disposed.
        private readonly List<Sprite> _gradients = new List<Sprite>();

        /// How many distinct shapes were baked. Reported beside the `drew` line,
        /// because rule 2's cost is a property of the scene rather than of the
        /// frame.
        public int Count => _cache.Count;

        public CanvasSprites()
        {
            var shader = Shader.Find(ShaderName);
            if (shader == null)
            {
                throw new InvalidOperationException(
                    $"the shader '{ShaderName}' is not in this player. The "
                    + "`unity-demo` recipes stage unity/demo/SpriteBake.shader into "
                    + "Assets/Resources/, which is what keeps a player build from "
                    + "stripping a shader nothing references from a scene.");
            }
            _bake = new Material(shader) { hideFlags = HideFlags.HideAndDontSave };
        }

        private const string ShaderName = "Dashscene/Samples/SpriteBake";

        /// The sprite for a node's fill: the rounded box, with no stroke ink.
        public Sprite Fill(CornerRadii corners, float strokeWidth, StrokeAlign align)
        {
            return Cached(new Key(corners, strokeWidth, align, ring: false));
        }

        /// The sprite for a node's stroke alone, for a rect whose stroke colour
        /// differs from its fill — rule 2's "otherwise the fill and the stroke
        /// are two".
        public Sprite Ring(CornerRadii corners, float strokeWidth, StrokeAlign align)
        {
            return Cached(new Key(corners, strokeWidth, align, ring: true));
        }

        /// Rule 3's gradient, baked once at the node's pixel size.
        ///
        /// **Every kind is baked at node size, where the rule's letter says a
        /// linear one is a 256-texel strip stretched.** A stretched strip cannot
        /// carry the node's rounded corners or its anti-aliased edge, so a node
        /// with either would draw a hard square boundary — a DIFFERENT picture,
        /// which rule 1 forbids. On a node with neither the two are the same
        /// texels, so what the departure costs is load-time texture memory and
        /// nothing in any per-frame figure, which is the term rule 3 is written
        /// about.
        ///
        /// **`pad` is the caller's, not this method's own.** The `Image` a gradient
        /// is drawn on is the node box grown by the same padding a shape sprite
        /// carries — the stroke's outset past the box plus the anti-aliasing band
        /// — and a texture baked with a different one is stretched to fit, which
        /// grows the shape and softens its edge. Measured: a gradient-filled node
        /// with an outside stroke drew half again its size.
        ///
        /// Not cached: a gradient is a property of one node's extent rather than
        /// of a shape shared between nodes, and an animated one re-bakes.
        public Sprite BakeGradient(
            Gradient gradient,
            ReadOnlySpan<GradientStop> stops,
            CornerRadii corners,
            float width,
            float height,
            float pad)
        {
            var sizeX = Mathf.Max(Mathf.CeilToInt(width + (pad * 2.0f)), 1);
            var sizeY = Mathf.Max(Mathf.CeilToInt(height + (pad * 2.0f)), 1);

            _bake.SetVector(
                CornersId,
                new Vector4(corners.TopLeft, corners.TopRight, corners.BottomRight, corners.BottomLeft));
            _bake.SetVector(StrokeId, new Vector4(0.0f, 0.0f, 0.0f, Aa));
            _bake.SetVector(
                BoxId,
                new Vector4((sizeX * 0.5f) - pad, (sizeY * 0.5f) - pad, sizeX, sizeY));
            _bake.SetVector(UnitId, new Vector4(width, height, pad, 0.0f));
            _bake.SetVector(
                HandlesId,
                new Vector4(
                    gradient.HandleOrigin.X,
                    gradient.HandleOrigin.Y,
                    gradient.HandlePrimary.X,
                    gradient.HandlePrimary.Y));

            var count = Mathf.Min((int)gradient.Stops.Count, MaxStops);
            _bake.SetVector(
                FrameId,
                new Vector4(
                    gradient.HandleSecondary.X,
                    gradient.HandleSecondary.Y,
                    (int)gradient.Kind,
                    count));

            var offsets = new Vector4[2];
            for (var s = 0; s < count; s++)
            {
                var stop = stops[(int)gradient.Stops.Offset + s];
                offsets[s / 4][s % 4] = stop.Offset;
                _stops[s] = new Vector4(stop.Color.R, stop.Color.G, stop.Color.B, stop.Color.A);
            }
            for (var s = count; s < MaxStops; s++)
            {
                _stops[s] = Vector4.zero;
            }
            _bake.SetVector(OffsetsLoId, offsets[0]);
            _bake.SetVector(OffsetsHiId, offsets[1]);
            _bake.SetVectorArray(StopsId, _stops);

            var texture = Rasterise(sizeX, sizeY, GradientPass, $"DashsceneGradient {sizeX}x{sizeY}");
            var sprite = Sprite.Create(
                texture,
                new Rect(0, 0, sizeX, sizeY),
                new Vector2(0.5f, 0.5f),
                pixelsPerUnit: 1.0f,
                extrude: 0,
                meshType: SpriteMeshType.FullRect,
                border: Vector4.zero);
            _gradients.Add(sprite);
            return sprite;
        }

        /// Destroys a gradient sprite this object baked, and its texture.
        ///
        /// **The one sprite a caller may release, and the reason ownership sits
        /// here at all.** A cached shape sprite is shared by every rect of that
        /// shape, so a scene that destroyed the sprites its elements point at
        /// would destroy each shared one once per rect and then again when this
        /// object is disposed. A gradient is baked per node and is not shared, so
        /// it is the only one with a single owner — and it still goes through
        /// this object, because the texture behind it is tracked here.
        public void ReleaseGradient(Sprite sprite)
        {
            if (sprite == null || !_gradients.Remove(sprite))
            {
                return;
            }

            var texture = sprite.texture;
            _textures.Remove(texture);
            UnityEngine.Object.Destroy(sprite);
            UnityEngine.Object.Destroy(texture);
        }

        /// How far a stroke reaches past the node's own box, in texels.
        ///
        /// `StrokeAlign` is boundary B's, and the three cases are the ones
        /// `stroke_coverage` in the shader library branches on.
        public static float Outset(float strokeWidth, StrokeAlign align)
        {
            switch (align)
            {
                case StrokeAlign.Outside:
                    return Mathf.Max(strokeWidth, 0.0f);
                case StrokeAlign.Center:
                    return Mathf.Max(strokeWidth, 0.0f) * 0.5f;
                default:
                    return 0.0f;
            }
        }

        /// How much larger than its node box a rect's `RectTransform` must be,
        /// on every side, for the baked sprite to hold the stroke's outset and
        /// its anti-aliased edge.
        public static float Pad(float strokeWidth, StrokeAlign align)
        {
            return Outset(strokeWidth, align) + Aa;
        }

        /// The 9-slice border, in texels: the corner the slice must keep whole.
        public static int Border(CornerRadii corners, float strokeWidth, StrokeAlign align)
        {
            var radius = Mathf.Max(
                Mathf.Max(corners.TopLeft, corners.TopRight),
                Mathf.Max(corners.BottomRight, corners.BottomLeft));
            return Mathf.CeilToInt(Mathf.Max(radius, 0.0f) + Pad(strokeWidth, align));
        }

        private Sprite Cached(Key key)
        {
            if (_cache.TryGetValue(key, out var sprite))
            {
                return sprite;
            }

            sprite = Bake(key);
            _cache[key] = sprite;
            return sprite;
        }

        private Sprite Bake(Key key)
        {
            var corners = key.Corners;
            var border = Border(corners, key.Width, key.Align);

            // A one-texel stretchable centre, so the 9-slice reproduces the
            // shape at any node size at or above `2 * border` on each axis.
            var size = border * 2 + 1;
            var pad = Pad(key.Width, key.Align);
            var half = size * 0.5f - pad;

            _bake.SetVector(
                CornersId,
                new Vector4(corners.TopLeft, corners.TopRight, corners.BottomRight, corners.BottomLeft));
            _bake.SetVector(StrokeId, new Vector4(key.Width, (int)key.Align, key.Ring ? 1.0f : 0.0f, Aa));
            _bake.SetVector(BoxId, new Vector4(half, half, size, size));

            // Linear, because the alpha this writes is coverage rather than a
            // colour: a sRGB target would encode it and `Image` would multiply
            // the encoded value by the tint.
            var texture = Rasterise(size, size, CoveragePass, $"DashsceneSprite {key}");
            return Sprite.Create(
                texture,
                new Rect(0, 0, size, size),
                new Vector2(0.5f, 0.5f),
                pixelsPerUnit: 1.0f,
                extrude: 0,
                meshType: SpriteMeshType.FullRect,
                border: new Vector4(border, border, border, border));
        }

        /// One blit into a temporary target, read back into a texture this object
        /// owns.
        ///
        /// **`ReadPixels` off an explicit `RenderTexture.active`**, which is the
        /// one readback family measured to work in this repository's Unity
        /// players: `ScreenCapture.CaptureScreenshotAsTexture` returned once and
        /// then hung on macOS/Metal, which `unity/render-gate`'s own header
        /// records.
        private Texture2D Rasterise(int sizeX, int sizeY, int pass, string name)
        {
            var target = RenderTexture.GetTemporary(
                sizeX, sizeY, 0, RenderTextureFormat.ARGB32, RenderTextureReadWrite.Linear);
            var texture = new Texture2D(sizeX, sizeY, TextureFormat.RGBA32, false, true)
            {
                name = name,
                wrapMode = TextureWrapMode.Clamp,

                // **Point, and this is what makes a 9-slice flat.** The Canvas
                // draws one sprite texel on one canvas unit, so a slice's
                // corners land 1:1 and filtering cannot improve them — while the
                // stretchable centre is ONE texel pulled across the whole
                // element, and bilinear sampling of one texel ramps from it
                // towards its neighbours across that whole span. Measured: every
                // rect with no corner radius rendered as a soft blob, and the
                // rects with a large radius — whose slice borders are wide — did
                // not.
                filterMode = FilterMode.Point,
                hideFlags = HideFlags.HideAndDontSave,
            };

            var previous = RenderTexture.active;
            try
            {
                Graphics.Blit(null, target, _bake, pass);
                RenderTexture.active = target;
                texture.ReadPixels(new Rect(0, 0, sizeX, sizeY), 0, 0);
                texture.Apply(false, true);
            }
            finally
            {
                RenderTexture.active = previous;
                RenderTexture.ReleaseTemporary(target);
            }

            _textures.Add(texture);
            return texture;
        }

        public void Dispose()
        {
            foreach (var sprite in _cache.Values)
            {
                UnityEngine.Object.Destroy(sprite);
            }
            _cache.Clear();

            foreach (var gradient in _gradients)
            {
                UnityEngine.Object.Destroy(gradient);
            }
            _gradients.Clear();

            foreach (var texture in _textures)
            {
                UnityEngine.Object.Destroy(texture);
            }
            _textures.Clear();

            UnityEngine.Object.Destroy(_bake);
        }

        /// `Dashscene/Samples/SpriteBake`'s two passes, in declaration order.
        private const int CoveragePass = 0;
        private const int GradientPass = 1;

        /// The bound the generated shader library clamps to.
        ///
        /// The package's own constant: `sdf_hlsl_is_generated.rs` holds
        /// `PaintHeap.MaxGradientStops` against `MAX_GRADIENT_STOPS` in
        /// `Sdf.hlsl`, which is the library `SpriteBake.shader` includes and
        /// whose clamp this bake must agree with.
        private const int MaxStops = PaintHeap.MaxGradientStops;

        private readonly Vector4[] _stops = new Vector4[MaxStops];

        private static readonly int CornersId = Shader.PropertyToID("_DsCorners");
        private static readonly int StrokeId = Shader.PropertyToID("_DsStroke");
        private static readonly int BoxId = Shader.PropertyToID("_DsBox");
        private static readonly int UnitId = Shader.PropertyToID("_DsUnit");
        private static readonly int HandlesId = Shader.PropertyToID("_DsHandles");
        private static readonly int FrameId = Shader.PropertyToID("_DsFrame");
        private static readonly int OffsetsLoId = Shader.PropertyToID("_DsOffsetsLo");
        private static readonly int OffsetsHiId = Shader.PropertyToID("_DsOffsetsHi");
        private static readonly int StopsId = Shader.PropertyToID("_DsStops");

        /// One distinct shape. Quantised, so it is comparable and hashable
        /// without a float equality anywhere.
        private readonly struct Key : IEquatable<Key>
        {
            private readonly int _tl, _tr, _br, _bl, _width;
            private readonly StrokeAlign _align;
            private readonly bool _ring;

            internal Key(CornerRadii corners, float width, StrokeAlign align, bool ring)
            {
                _tl = Quantise(corners.TopLeft);
                _tr = Quantise(corners.TopRight);
                _br = Quantise(corners.BottomRight);
                _bl = Quantise(corners.BottomLeft);
                _width = Quantise(width);
                _align = align;
                _ring = ring;
            }

            internal CornerRadii Corners => new CornerRadii
            {
                TopLeft = _tl / KeyStep,
                TopRight = _tr / KeyStep,
                BottomRight = _br / KeyStep,
                BottomLeft = _bl / KeyStep,
            };

            internal float Width => _width / KeyStep;

            internal StrokeAlign Align => _align;

            internal bool Ring => _ring;

            /// **Clamped at zero, not merely rounded.** A negative radius or
            /// width is not a shape, and a negative `border` below would make a
            /// zero-or-negative texture extent, which `Texture2D` refuses with
            /// an exception from inside the load rather than a diagnostic.
            private static int Quantise(float value)
            {
                return Mathf.RoundToInt(Mathf.Max(value, 0.0f) * KeyStep);
            }

            public bool Equals(Key other)
            {
                return _tl == other._tl
                    && _tr == other._tr
                    && _br == other._br
                    && _bl == other._bl
                    && _width == other._width
                    && _align == other._align
                    && _ring == other._ring;
            }

            public override bool Equals(object obj) => obj is Key other && Equals(other);

            public override int GetHashCode()
            {
                var hash = 17;
                hash = (hash * 31) + _tl;
                hash = (hash * 31) + _tr;
                hash = (hash * 31) + _br;
                hash = (hash * 31) + _bl;
                hash = (hash * 31) + _width;
                hash = (hash * 31) + (int)_align;
                hash = (hash * 31) + (_ring ? 1 : 0);
                return hash;
            }

            public override string ToString()
            {
                return $"{_tl},{_tr},{_br},{_bl} w{_width} {_align}{(_ring ? " ring" : string.Empty)}";
            }
        }
    }
}
