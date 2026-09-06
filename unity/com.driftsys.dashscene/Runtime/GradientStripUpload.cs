// Whether the gradient strip's rows are already in the texture, kept where a
// gate can execute it.
//
// `HeapUpload.cs`'s argument, applied to the one table that is a texture rather
// than a `GraphicsBuffer`. This comparison decides whether `BrgPainter` copies
// the strip at all, so getting it wrong in the "already there" direction
// freezes the document's gradients for the life of the painter — a transition
// that recolours a ramp draws the colours of the commit before it, with a
// correct-looking first frame and no diagnostic. Left inside `Runtime/Engine/`
// it would be compiled by no CI job and executed by nothing: `unity/ffi-check`
// excludes that directory, and `unity/render-gate` draws one static document,
// so a run of it cannot tell "skip when unchanged" from "skip always".
//
// Here, `unity/ffi-check` runs it on every pull request.

namespace Driftsys.Dashscene
{
    /// The gradient strip's upload skip, as three booleans.
    public static class GradientStripUpload
    {
        /// Whether the texture already holds this commit's rows.
        ///
        /// `uploaded` is whether anything was ever copied into the texture the
        /// painter holds now — false before the first copy, and false again
        /// whenever a changed row count made a new texture, because a
        /// `Texture2D` cannot be resized.
        ///
        /// **`documentReplaced` is not redundant with the generation.** The
        /// generation is counted within one arena's commit chain, so a replaced
        /// document starts again and its 1 can follow the previous document's 1
        /// while naming different colours. There is no value of the generation
        /// that means "a different document", which is why `DsFrame`'s own
        /// replacement flag is the second trigger.
        ///
        /// **The generation is compared, not ordered.** A host that asked
        /// whether it had grown would never re-copy after a replacement reset
        /// it to a lower number.
        public static bool AlreadyUploaded(
            bool uploaded, bool documentReplaced, ulong generation, ulong lastGeneration)
        {
            return uploaded && !documentReplaced && generation == lastGeneration;
        }
    }
}
