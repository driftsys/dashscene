// What the painter was handed and did not draw, by name.
//
// **P4 is the whole reason this type exists.** "Every out-of-profile construct
// is a named diagnostic, never a silent drop" — and a painter is where that is
// easiest to break, because a node it cannot draw costs nothing to skip and
// produces a picture that looks finished. Every skip this painter makes lands
// here with the rect that caused it.
//
// Engine-independent, so `unity/package-compat` compiles it and a host that is
// not Unity could report the same set.

using System;
using System.Collections.Generic;
using System.Text;

namespace Driftsys.Dashscene
{
    /// One reason the painter drew less than the document asked for.
    ///
    /// A `[Flags]` enum rather than a list of strings, because the set is
    /// closed and a host branching on it should not be matching prose.
    [Flags]
    public enum PackDiagnostic
    {
        /// Nothing was skipped.
        None = 0,

        /// A node carries a drop or inner shadow. This painter emits no shadow
        /// instance: `blurred_rounded_box` is in the shader library it already
        /// includes, but a shadow needs the paint heap's shadow rows and a
        /// second pass ordering that is not built here.
        Shadow = 1 << 0,

        /// A node carries a layer blur.
        LayerBlur = 1 << 1,

        /// A node carries a backdrop blur.
        ///
        /// **This one cannot be fixed by adding a pass**, and the reason is
        /// worth the separate flag. A backdrop blur reads what the painter
        /// itself composited into the target
        /// (`docs/decisions/a-backdrop-blur-snapshots-the-target-it-draws-into.md`
        /// D3), and a Unity host's target holds the engine's own scene as well.
        /// Frosted glass over a Unity 3D scene is a host material effect
        /// outside boundary B; the vocabulary accepts the node either way,
        /// which is exactly why it has to be reported.
        BackdropBlur = 1 << 2,

        /// A node's fill is an image. The payload crosses the ABI, and nothing
        /// here uploads it: a texture per image, resident across commits and
        /// invalidated by `DocumentReplaced`, is its own piece of work.
        ImageFill = 1 << 3,

        /// A node is a baked vector: its outline is a coverage field rather
        /// than the parametric rounded box this painter's SDF evaluates.
        VectorField = 1 << 4,

        /// The document carries glyph runs and the painter was given no
        /// atlas set to shade them from.
        ///
        /// **A host step, not a missing capability.** The sheets cross the C
        /// ABI on their own call rather than in the frame, because they belong
        /// to the load: read them with `DashsceneRuntime.ReadAtlases` whenever
        /// a frame reports `DocumentReplaced`, and hand them to the painter.
        /// A painter with none draws every other node and reports this.
        GlyphRun = 1 << 5,

        /// The document carries render-target groups. A translucent group's
        /// overlapping children composite twice without one.
        RenderTargetGroup = 1 << 6,

        /// A gradient carries more stops than a row has slots, and the extra
        /// ones were not uploaded.
        GradientStopsTruncated = 1 << 7,

        /// A node needs coverage or alpha the selected material class cannot
        /// express.
        ///
        /// [`MaterialClass.LitOpaque`] only, and it covers **five** things: a
        /// corner radius, a clip, a stroke, a per-node opacity below one, and a
        /// fill whose colour — or any gradient stop — is not fully opaque. That
        /// class does not blend and its fragment stage returns `1.0` for alpha,
        /// so each of those would be drawn away silently: a square corner for
        /// the first three, and a fully opaque node for the last two. Reported
        /// rather than drawn. A first version considered only the geometric
        /// three.
        CoverageNotExpressible = 1 << 8,

        /// A row named a table entry that does not exist, or a range ran past
        /// its table.
        ///
        /// A rect whose paint or clip index is past its table; a clip-box,
        /// blur or extra-fill range past its own; a stroke row past the stroke
        /// table or a stroke range of arity above one; a solid or gradient row
        /// index past its table; a gradient stop range past the stop table; a
        /// stacked layer tagged `None`; a tag outside the four boundary B
        /// declares; a glyph run naming an atlas the set does not hold, a quad
        /// range past the quad table, or an anchor the rect walk has already
        /// passed.
        ///
        /// The producers are not enumerated by count here: the list has been
        /// wrong twice, once as "three" and once as "nine", and the number is
        /// not what a reader of a diagnostic needs.
        ///
        /// **Not a document defect this painter can repair**, and not one it
        /// can ignore either: the tables are raw pointers, so following the
        /// index would be undefined behaviour rather than an exception. The
        /// node is skipped and counted.
        CorruptRow = 1 << 9,
    }

    /// The diagnostics one packed frame produced.
    ///
    /// **Compared between frames rather than logged from inside one.** A
    /// document that carries a shadow carries it on every commit, so reporting
    /// per frame would put one line per skipped node into the log sixty times a
    /// second. The painter logs when this value changes.
    public readonly struct PackDiagnostics : IEquatable<PackDiagnostics>
    {
        /// Everything that was skipped.
        public PackDiagnostic Flags { get; }

        /// How many rects contributed at least one skip.
        public int AffectedRects { get; }

        /// The first rect index that contributed one, or -1 when nothing did.
        ///
        /// A number a reader can act on: it indexes the same `rects` array the
        /// document's own tooling reports over.
        public int FirstRect { get; }

        internal PackDiagnostics(PackDiagnostic flags, int affectedRects, int firstRect)
        {
            Flags = flags;
            AffectedRects = affectedRects;
            FirstRect = firstRect;
        }

        /// Nothing was skipped.
        public bool IsClean => Flags == PackDiagnostic.None;

        /// One line per flag, naming what was dropped and how much of it.
        ///
        /// Empty when [`IsClean`]. The caller decides the severity: a host
        /// drawing a document it authored for this painter wants a warning, and
        /// a host bringing up a Figma import wants the list once.
        public IReadOnlyList<string> Describe()
        {
            var lines = new List<string>();
            if (IsClean)
            {
                return lines;
            }

            foreach (var pair in Descriptions)
            {
                if ((Flags & pair.Key) != 0)
                {
                    lines.Add(pair.Value);
                }
            }

            // **Not every diagnostic is attributed to a rect.** A truncated
            // gradient is a property of a heap row, and the nodes that use it
            // are not walked to find it — so the count can legitimately be
            // zero while a flag is set, and a line reading "0 rect(s)
            // affected, first at index -1" would read as a defect in the
            // report rather than as a document-level finding.
            lines.Add(
                AffectedRects > 0
                    ? $"{AffectedRects} rect(s) affected, first at index {FirstRect}. "
                      + "The document asked for these and this painter did not draw them."
                    : "no individual rect was implicated: the finding above is a property "
                      + "of the document's paint tables rather than of one node.");
            return lines;
        }

        /// The sentence each flag reports, in reporting order.
        ///
        /// A table rather than a `switch` per call site, so a flag added
        /// without a sentence is one missing entry rather than a silent gap in
        /// a formatter. **Read in this array's order, which is the order the
        /// lines are reported in and is not the enum's** — `CorruptRow` is
        /// declared last and listed before `CoverageNotExpressible`, because a
        /// corrupt row explains the others when both fire.
        private static readonly KeyValuePair<PackDiagnostic, string>[] Descriptions =
        {
            new KeyValuePair<PackDiagnostic, string>(
                PackDiagnostic.Shadow,
                "shadows: not drawn — this painter emits no shadow instance."),
            new KeyValuePair<PackDiagnostic, string>(
                PackDiagnostic.LayerBlur,
                "layer blurs: not drawn."),
            new KeyValuePair<PackDiagnostic, string>(
                PackDiagnostic.BackdropBlur,
                "backdrop blurs: not drawn, and not fixable by adding a pass — a "
                + "backdrop reads what the painter itself composited, and this "
                + "painter's target also holds the engine's scene."),
            new KeyValuePair<PackDiagnostic, string>(
                PackDiagnostic.ImageFill,
                "image fills: not drawn — no texture is uploaded from the payload."),
            new KeyValuePair<PackDiagnostic, string>(
                PackDiagnostic.VectorField,
                "baked vector nodes: not drawn — their outline is a coverage "
                + "field rather than the parametric rounded box shaded here."),
            new KeyValuePair<PackDiagnostic, string>(
                PackDiagnostic.GlyphRun,
                "glyph runs: not drawn — no atlas set was installed. Read the "
                + "sheets with DashsceneRuntime.ReadAtlases after each load and "
                + "hand them to the painter."),
            new KeyValuePair<PackDiagnostic, string>(
                PackDiagnostic.RenderTargetGroup,
                "render-target groups: not composited — a translucent group's "
                + "overlapping children are drawn twice."),
            new KeyValuePair<PackDiagnostic, string>(
                PackDiagnostic.GradientStopsTruncated,
                "gradient stops beyond the eighth: not uploaded."),
            new KeyValuePair<PackDiagnostic, string>(
                PackDiagnostic.CorruptRow,
                "rows naming a table entry that does not exist: skipped — the "
                + "committed frame is not one this package can read."),
            new KeyValuePair<PackDiagnostic, string>(
                PackDiagnostic.CoverageNotExpressible,
                "corner radii, clips, strokes, per-node opacity below one, and "
                + "translucent fills: not drawn as authored — the lit-opaque "
                + "material class does not blend, so neither partial coverage "
                + "nor partial alpha can be expressed. Use the unlit-overlay or "
                + "lit-cutout class for these nodes."),
        };

        /// One line, for a log that wants a summary rather than a list.
        public override string ToString()
        {
            if (IsClean)
            {
                return "no construct was skipped";
            }

            var text = new StringBuilder();
            foreach (var line in Describe())
            {
                if (text.Length > 0)
                {
                    text.Append(' ');
                }
                text.Append(line);
            }
            return text.ToString();
        }

        /// Value equality, so a painter can ask whether the set changed.
        public bool Equals(PackDiagnostics other)
        {
            return Flags == other.Flags
                   && AffectedRects == other.AffectedRects
                   && FirstRect == other.FirstRect;
        }

        /// <inheritdoc />
        public override bool Equals(object obj)
        {
            return obj is PackDiagnostics other && Equals(other);
        }

        /// <inheritdoc />
        public override int GetHashCode()
        {
            // Written out rather than `HashCode.Combine`, which netstandard2.1
            // does have — measured, it compiles clean against that target. The
            // reason is stability: `HashCode` is randomly seeded per process,
            // and this value is only ever compared within one run. An earlier
            // comment here said the API was unavailable, which was false.
            // The three fields are small and independent; a shift each keeps
            // them from cancelling.
            unchecked
            {
                var hash = (int)Flags;
                hash = (hash * 397) ^ AffectedRects;
                hash = (hash * 397) ^ FirstRect;
                return hash;
            }
        }

        /// Equality operator, paired with [`Equals`].
        public static bool operator ==(PackDiagnostics left, PackDiagnostics right)
        {
            return left.Equals(right);
        }

        /// Inequality operator, paired with [`Equals`].
        public static bool operator !=(PackDiagnostics left, PackDiagnostics right)
        {
            return !left.Equals(right);
        }
    }
}
