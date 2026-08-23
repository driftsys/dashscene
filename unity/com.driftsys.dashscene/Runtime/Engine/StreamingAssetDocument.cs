// Where a `.dsb` shipped with a Unity project actually lives.
//
// **Engine-referencing, so it sits under `Runtime/Engine/`** — the half of
// `Runtime/` that `docs/decisions/r-e10-is-checked-in-two-halves.md` D2 carves
// out for files that need `UnityEngine`, checked by `just unity-editor` rather
// than by `unity/package-compat`. It was in `Samples~/FrameLoop` when story
// #1124 wrote it, because that directory did not exist yet and no gate could
// compile a `UnityEngine` reference at all; story #1122 landing #1286's ruling
// is what made this its right home, and a sample nothing compiles the wrong
// one.

using System;
using System.IO;
using UnityEngine;

namespace Driftsys.Dashscene
{
    /// Resolves a document path a host configured into a `DocumentRange` the
    /// runtime can load, on whichever platform the player is running.
    ///
    /// **Static and stateless on purpose.** It reads Unity's own paths and asks
    /// the APK where an entry is; it holds nothing, so a host may call it from
    /// `Awake`, from a scene load, or per document switch.
    public static class StreamingAssetDocument
    {
        /// Where the document is, on whichever platform this is running on.
        ///
        /// **Three cases, and only the third needs anything Unity-specific.**
        /// An absolute path is taken as it stands. Off Android, a relative path
        /// is resolved under `Application.streamingAssetsPath`, which is a real
        /// directory there. On Android it is not a directory at all: it
        /// resolves to `jar:file:///data/app/<pkg>/base.apk!/assets`, and the
        /// document is a byte range inside that APK.
        ///
        /// **Why this is not a copy.** The obvious Android fix is to
        /// `UnityWebRequest` the asset out to `Application.persistentDataPath`
        /// on first run and map that. It works, and it costs a full copy of the
        /// file plus a second copy of it on disk for the life of the install,
        /// on a first run — which is the cost R5 and the mapped loader exist to
        /// avoid. `AssetManager.openFd` reports where the entry already is, and
        /// `DashsceneRuntime.LoadDocumentMapped(DocumentRange, uint)` maps it
        /// there. `docs/decisions/the-document-is-mapped-where-it-is-packed.md`
        /// carries the four shapes this was chosen from.
        ///
        /// **`openFd` throws when the entry is compressed**, which is the one
        /// build setting this depends on. Unity's own gradle template marks
        /// every StreamingAssets file `noCompress`, so a stock build stores the
        /// `.dsb` whole — a custom `mainTemplate.gradle` that drops
        /// `unityStreamingAssets` from that list is what would break it, and the
        /// exception says so rather than being swallowed.
        ///
        /// **What checks this, now that it is not in a sample.**
        /// `just unity-editor` compiles it — R-E10's second half, over
        /// `Runtime/Engine/`. That is a compile and not an execution: whether
        /// `openFd` reports the offset an APK actually holds is answered by a
        /// device, and
        /// `docs/decisions/the-document-is-mapped-where-it-is-packed.md`
        /// records the run that answered it. `DocumentRange` and the loader
        /// this feeds are in the engine-free half, where `unity/ffi-check`
        /// executes them against the real library on every pull request.
        public static DocumentRange Resolve(string relativeOrAbsolute)
        {
            if (string.IsNullOrEmpty(relativeOrAbsolute))
            {
                throw new ArgumentException(
                    "documentPath is empty", nameof(relativeOrAbsolute));
            }

            if (Path.IsPathRooted(relativeOrAbsolute))
            {
                return DocumentRange.WholeFile(relativeOrAbsolute);
            }

#if UNITY_ANDROID && !UNITY_EDITOR
            return InsideTheApk(relativeOrAbsolute);
#else
            return DocumentRange.WholeFile(
                Path.Combine(Application.streamingAssetsPath, relativeOrAbsolute));
#endif
        }

#if UNITY_ANDROID && !UNITY_EDITOR
        /// The APK entry `name` occupies, as a container path plus a byte range.
        ///
        /// **The container is read off the file descriptor rather than assumed
        /// to be `Application.dataPath`.** The two agree for an ordinary
        /// install — both were `/data/app/<pkg>/base.apk` when this was measured
        /// — and they do not have to: `AssetManager` serves an asset out of
        /// whichever APK holds it, which for a split install is not the base.
        /// `/proc/self/fd/<n>` names the file the descriptor is open on, so it
        /// is right in both cases.
        ///
        /// **The descriptor is closed here, and `using` is not what closes it.**
        /// `AndroidJavaObject.Dispose` releases the JNI global reference to the
        /// Java object; it does not call `close()` on a `Closeable`. Left to
        /// `using` alone the file descriptor onto `base.apk` stays open until
        /// ART finalizes the object, so a host that resolves more than once —
        /// an additive scene load, a pooled prefab, a document switch —
        /// accumulates descriptors against the process limit. `finally` rather
        /// than a `using`, because the close must happen after the path is read
        /// out of `/proc/self/fd/<n>` and `using` would order it the other way.
        ///
        /// **Closing it costs the mapping nothing**, which is why it is safe to
        /// do here rather than after the load: this method hands back a path,
        /// the library opens that path itself, and no descriptor of ours is
        /// ever the one mapped.
        private static DocumentRange InsideTheApk(string name)
        {
            using var player = new AndroidJavaClass("com.unity3d.player.UnityPlayer");
            using var activity = player.GetStatic<AndroidJavaObject>("currentActivity");
            using var assets = activity.Call<AndroidJavaObject>("getAssets");

            // Java's AssetManager takes forward slashes whatever the host's
            // separator is, and Path.Combine on Windows would have produced
            // backslashes into the serialized field.
            using var descriptor = assets.Call<AndroidJavaObject>(
                "openFd", name.Replace('\\', '/'));
            try
            {
                long start = descriptor.Call<long>("getStartOffset");
                long length = descriptor.Call<long>("getLength");

                // **Both are checked, and both are `long` on the Java side.**
                // `AssetFileDescriptor.UNKNOWN_LENGTH` is -1, and a negative
                // cast to `ulong` becomes a value near 2^64 — which the library
                // would refuse as a range past the end of the APK, with a
                // message about arithmetic rather than about the asset. The
                // offset has no documented sentinel and is checked anyway,
                // because the cast is the same and a wrong answer here is
                // silent.
                if (start < 0 || length < 0)
                {
                    throw new InvalidOperationException(
                        $"the APK reports offset {start} and length {length} for {name}, "
                        + "which cannot name bytes inside it");
                }

                using var parcel = descriptor.Call<AndroidJavaObject>("getParcelFileDescriptor");
                int fd = parcel.Call<int>("getFd");
                using var link = new AndroidJavaObject("java.io.File", "/proc/self/fd/" + fd);
                string container = link.Call<string>("getCanonicalPath");

                return DocumentRange.Window(container, (ulong)start, (ulong)length);
            }
            finally
            {
                // Closes the ParcelFileDescriptor it holds, and so the
                // descriptor onto base.apk. It runs on the unknown-length throw
                // above as well as on the ordinary return. A COMPRESSED entry
                // does not reach here at all — `openFd` throws before the try
                // is entered, and it opened nothing to leak.
                //
                // **Logged, never thrown.** `AssetFileDescriptor.close()`
                // declares `IOException`, and a throw from a `finally` discards
                // the pending return — so a resolve that had already read the
                // right offset and length would be replaced by a cleanup
                // failure, and the sample would disable itself over a document
                // it had located correctly.
                try
                {
                    descriptor.Call("close");
                }
                catch (Exception e)
                {
                    Debug.LogWarning(
                        $"[dashscene] the APK asset descriptor for {name} did not close: "
                        + e.Message);
                }
            }
        }
#endif
    }
}
