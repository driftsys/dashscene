// Where a `StreamingAssets` file's bytes or text actually come from, on
// whichever platform the player is running — the reader both showcase
// samples share.

using System.IO;
using System.Text;
using Driftsys.Dashscene;

namespace Driftsys.Dashscene.Samples
{
    /// Reads a `StreamingAssets` file's bytes or text, on whichever platform
    /// this is running on.
    ///
    /// **`File` cannot read one directly on Android**, and the failure is
    /// silent in the worst way: `Application.streamingAssetsPath` is
    /// `jar:file:///data/app/<pkg>/base.apk!/assets`, so `File.Exists` answers
    /// false for a file that is present and the reader concludes it was never
    /// staged. Measured on a Pixel 5 on 2026-08-29: the player reported the
    /// manifest missing, `Awake` ended, and the SCENES — which need no
    /// manifest at all — never loaded either. Issue #1469 measured the same
    /// failure against `DashsceneCanvasBaseline`, which read the manifest and
    /// the font cascade the same broken way rather than through this reader.
    ///
    /// **Both methods go through `StreamingAssetDocument.Resolve`** rather
    /// than carrying a second answer to the same question. That resolver asks
    /// the APK's own `AssetManager` where the entry is and hands back a
    /// container path with a byte range, which is what the mapped document
    /// loader already uses — so this reads the asset where it is packed, and
    /// there is one place that knows how an APK stores one.
    ///
    /// **`ReadStreamingAssetText` is `ReadBytes` decoded, not a second
    /// reader.** A manifest is a few hundred bytes, so reading it is not the
    /// cost `Resolve` exists to avoid for a document; the point is that it is
    /// the same LOOKUP, and now the same READ.
    internal static class StreamingAssetText
    {
        internal static byte[] ReadBytes(string relative)
        {
            var range = StreamingAssetDocument.Resolve(relative);
            if (range.IsWholeFile)
            {
                return File.ReadAllBytes(range.ContainerPath);
            }

            using var stream = new FileStream(
                range.ContainerPath, FileMode.Open, FileAccess.Read);
            stream.Seek((long)range.Offset, SeekOrigin.Begin);
            var bytes = new byte[range.Length];
            var read = 0;
            // **Looped, because one `Read` is not obliged to fill the buffer.**
            while (read < bytes.Length)
            {
                var got = stream.Read(bytes, read, bytes.Length - read);
                if (got <= 0)
                {
                    break;
                }

                read += got;
            }

            // **A short read is refused, not returned.** Breaking out of the
            // loop and handing back the partial bytes is exactly the outcome
            // the loop exists to prevent: a caller decoding JSON or a font
            // would then report the content malformed where the entry is fine
            // and the READ was partial — and a truncation landing after a
            // syntactically complete prefix parses, silently, with a short
            // result.
            if (read != bytes.Length)
            {
                throw new IOException(
                    $"{relative}: read {read} of {bytes.Length} byte(s) from "
                    + $"{range.ContainerPath} at offset {range.Offset}. The entry is "
                    + "shorter than the asset manager reported, so the content is "
                    + "partial rather than malformed.");
            }

            return bytes;
        }

        /// **A leading UTF-8 BOM is stripped, matching `File.ReadAllText`.**
        /// `Encoding.UTF8.GetString` decodes the three BOM bytes to `U+FEFF`
        /// rather than detecting and dropping them, and `JsonUtility` does not
        /// accept that character before a manifest's `{` — so a manifest saved
        /// with a BOM would fail to parse here where the `File.ReadAllText`
        /// this replaces would have read it.
        internal static string ReadStreamingAssetText(string relative)
        {
            var text = Encoding.UTF8.GetString(ReadBytes(relative));
            return text.Length > 0 && text[0] == '\uFEFF' ? text.Substring(1) : text;
        }
    }
}
