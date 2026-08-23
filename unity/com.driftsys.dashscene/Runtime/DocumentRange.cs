// Where a `.dsb` lives when it does not begin a file.

using System;

namespace Driftsys.Dashscene
{
    /// A `.dsb` held as a byte range inside a larger file.
    ///
    /// **The shape an Android APK forces.** Unity puts everything under
    /// `Assets/StreamingAssets/` into `assets/` in the APK and its own gradle
    /// template marks those entries `noCompress`, so a `.dsb` is stored whole
    /// and uncompressed — but it is stored *inside* `base.apk`, at an offset,
    /// and `Application.streamingAssetsPath` resolves to a `jar:file://…!/assets`
    /// URI rather than to a path. `AssetManager.openFd` is what reports the
    /// offset and the length, and the container is the APK.
    ///
    /// Measured on `6000.3.22f1`, an arm64 Android 14 emulator, one `.dsb` in
    /// `StreamingAssets`: the entry was `Stored`, `openFd` gave a start offset
    /// of 24073616 and a length of 4189, and the process could open the
    /// container by path and read the document's magic at that offset.
    ///
    /// **This type is engine-independent on purpose.** Producing one on Android
    /// needs `UnityEngine` and `AndroidJavaObject`, which
    /// `docs/specification/07-embedding-and-distribution.md` R-E10 keeps out of
    /// `Runtime/` until issue #1286 settles it — so that half is the
    /// `Samples~/FrameLoop` sample's, and everything a gate can execute is
    /// here.
    public readonly struct DocumentRange : IEquatable<DocumentRange>
    {
        private DocumentRange(string containerPath, ulong offset, ulong length, bool wholeFile)
        {
            ContainerPath = containerPath;
            Offset = offset;
            Length = length;
            IsWholeFile = wholeFile;
        }

        /// The file the document is in. For a whole-file range this is the
        /// document itself; on Android it is `base.apk`.
        public string ContainerPath { get; }

        /// The document's first byte, as an offset into `ContainerPath`.
        /// Always 0 when `IsWholeFile`.
        public ulong Offset { get; }

        /// The document's length in bytes. Always 0 when `IsWholeFile`, where
        /// the library reads the file's own length instead.
        public ulong Length { get; }

        /// Whether this names a whole file rather than a window inside one.
        ///
        /// The two are separate calls in the C ABI rather than one with a
        /// sentinel length, so this is what `DashsceneRuntime` branches on. A
        /// length of 0 is a refused range there, not "to the end of the file".
        public bool IsWholeFile { get; }

        /// A file that is itself a `.dsb` — a desktop path, or a document
        /// already extracted to `Application.persistentDataPath`.
        public static DocumentRange WholeFile(string path)
        {
            RefuseAnEmptyPath(path, nameof(path));
            return new DocumentRange(path, 0, 0, true);
        }

        /// `length` bytes at `offset` inside `containerPath`.
        ///
        /// **A length of 0 is refused here rather than passed on.** The library
        /// answers `DsStatus.Map` for it, and a host that arrived at 0 got it
        /// from a container query that failed — `AssetFileDescriptor` reports
        /// `UNKNOWN_LENGTH` as -1, which becomes a very large `ulong` rather
        /// than 0, so both ends of that mistake are worth naming here where the
        /// message can say which argument was wrong.
        public static DocumentRange Window(string containerPath, ulong offset, ulong length)
        {
            RefuseAnEmptyPath(containerPath, nameof(containerPath));

            if (length == 0)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(length),
                    "a document range of 0 bytes names no document. A container query that could "
                    + "not find the entry is the usual cause.");
            }

            if (offset > ulong.MaxValue - length)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(offset),
                    $"an offset of {offset} plus a length of {length} overflows, so this range "
                    + "cannot name bytes in any file.");
            }

            return new DocumentRange(containerPath, offset, length, false);
        }

        /// **Empty is refused as well as null**, for the reason `Window` gives
        /// for a length of 0: a host that arrived at `""` got it from a config
        /// value or a container query that produced nothing, and the library
        /// would report it as `File::open("")` — `DsStatus.Map` with a message
        /// reading ": No such file or directory", naming neither the argument
        /// nor the caller's mistake.
        ///
        /// **An `ArgumentException` here and a `DashsceneException` from
        /// `DashsceneRuntime.LoadDocumentMapped(string, uint)`, deliberately.**
        /// These are factories: nothing has been called, there is no status to
        /// report, and a host building a range is at the point where
        /// fail-fast is right. That overload is a call on a live runtime whose
        /// documented failure type has shipped since story #1121, and moving it
        /// would step around the `catch (DashsceneException)` every host is
        /// told to write.
        private static void RefuseAnEmptyPath(string path, string parameter)
        {
            if (path == null)
            {
                throw new ArgumentNullException(parameter);
            }

            if (path.Length == 0)
            {
                throw new ArgumentException(
                    "a document range names no file. An empty path is what a config value or a "
                    + "container query that found nothing produces.",
                    parameter);
            }
        }

        public bool Equals(DocumentRange other) =>
            IsWholeFile == other.IsWholeFile
            && Offset == other.Offset
            && Length == other.Length
            && ContainerPath == other.ContainerPath;

        public override bool Equals(object obj) => obj is DocumentRange other && Equals(other);

        public override int GetHashCode()
        {
            // Written out rather than `HashCode.Combine`, which netstandard2.1
            // does have — but the package targets it through Unity's compiler
            // and this keeps the type free of any BCL surface beyond what
            // `unity/package-compat` compiles.
            var hash = ContainerPath == null ? 0 : ContainerPath.GetHashCode();
            hash = (hash * 397) ^ Offset.GetHashCode();
            hash = (hash * 397) ^ Length.GetHashCode();
            return (hash * 397) ^ IsWholeFile.GetHashCode();
        }

        public override string ToString() =>
            IsWholeFile
                ? ContainerPath
                : $"{ContainerPath}[{Offset}..{Offset + Length}]";
    }
}
