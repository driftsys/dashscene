// The managed lifetime of a dashscene runtime.
//
// Version negotiation, runtime lifetime, document load and the tick — the half
// of the C ABI a host that draws its own frames uses. The other half
// (`attach_surface`, `detach_surface`, `resize`, `draw`) belongs to a host that
// hands dashscene a surface and lets it paint; a Unity host does the inverse and
// reaches the tables through `AcquireFrame`.

using System;
using System.Text;
using System.Threading;

namespace Driftsys.Dashscene
{
    /// A live dashscene runtime.
    ///
    /// **Thread-affine.** A runtime is reachable only from the thread whose
    /// constructor minted it; from any other thread every call answers
    /// `DsStatus.WrongThread`, including after the minting thread has exited.
    /// The two cases are deliberately not distinguished, because telling them
    /// apart needs a process-wide registry of live threads and that is the
    /// shared state the design exists to avoid.
    ///
    /// **`Dispose` must run on that same thread, and there is deliberately no
    /// finalizer.** A finalizer runs on the GC's own thread, where
    /// `ds_runtime_free` answers `DsStatus.WrongThread` and the runtime leaks
    /// with nothing reported. A type that cannot be collected correctly should
    /// not pretend it can, so this one requires an explicit `Dispose` on the
    /// owning thread and says so rather than installing a finalizer that would
    /// silently do nothing.
    public sealed class DashsceneRuntime : IDisposable
    {
        private ulong _handle;
        private readonly int _ownerThreadId;
        private FrameLease _lease;

        private static readonly object AbiGate = new object();
        private static bool _abiChecked;

        /// Creates an empty runtime — no document, no surface.
        ///
        /// Negotiates the ABI version first: `ds_abi_version` is called before
        /// any other entry point, and a value this package was not built
        /// against is refused here rather than discovered later as a corrupted
        /// argument. That is R-E16.
        public DashsceneRuntime()
        {
            EnsureAbiCompatible();

            var status = Native.ds_runtime_new(out _handle);
            if (status != DsStatus.Ok)
            {
                // The library writes 0 on every failure, so the handle cannot
                // resolve even if this throw were somehow swallowed.
                _handle = 0;
                DashsceneException.ThrowIfFailed(status, "ds_runtime_new");
            }

            _ownerThreadId = Thread.CurrentThread.ManagedThreadId;
        }

        /// Compares the loaded library's ABI version against this package's.
        ///
        /// Called once per process. **The refusal reports both numbers**, which
        /// R-E16 requires: "mismatch" without them tells a customer nothing
        /// about which half to change.
        public static void EnsureAbiCompatible()
        {
            // **A lock, not an interlocked latch, and the latch is set only
            // after the call returns.** Two defects live in the obvious
            // version. Setting a flag before calling `ds_abi_version` means any
            // throw from it — `DllNotFoundException` when no native library is
            // present, which is this package's SHIPPED state, or
            // `EntryPointNotFoundException` against a library old enough to
            // predate the symbol, which is the very mismatch this exists to
            // catch — leaves the flag set, and every later construction then
            // skips the handshake entirely. And a compare-and-swap lets a
            // second thread observe the flag while the first is still inside
            // the call, so it proceeds against a library whose version has not
            // been compared yet.
            lock (AbiGate)
            {
                if (_abiChecked)
                {
                    return;
                }

                CompareAbiVersion(Native.AbiVersion);
                _abiChecked = true;
            }
        }

        /// The comparison R-E16 requires, without the once-per-process latch.
        ///
        /// Separated so `unity/ffi-check` can perform the mismatch that
        /// requirement's own _Check_ asks for — "build a host against a
        /// mismatched value and assert it refuses" — which it otherwise could
        /// not do, because `Native.AbiVersion` is a `const` and a C# compiler
        /// inlines it at every use site, so no reflection can move it.
        internal static void CompareAbiVersion(uint packageVersion)
        {
            var actual = Native.ds_abi_version();
            if (actual != packageVersion)
            {
                throw new DashsceneAbiMismatchException(packageVersion, actual);
            }
        }

        /// The version the loaded library reports.
        public static uint LibraryAbiVersion => Native.ds_abi_version();

        /// The version this package was built against.
        public static uint PackageAbiVersion => Native.AbiVersion;

        /// The native library's base name, as `[DllImport]` asks the loader for
        /// it. Exposed so a host's "not found" message can name what is missing.
        public static string LibraryName => Native.Lib;

        /// Whether a frame lease is outstanding. Every call that would commit is
        /// refused while one is.
        public bool HasOutstandingLease => _lease != null;

        /// Loads a `.dsb` held in memory.
        ///
        /// **The owning path**: every payload is copied, so the cost tracks the
        /// file rather than the shown root. If you have the file rather than its
        /// bytes, `LoadDocumentMapped` is the bounded path and costs less.
        public void LoadDocument(byte[] bytes)
        {
            if (bytes == null)
            {
                throw new ArgumentNullException(nameof(bytes));
            }

            Check(
                Native.ds_runtime_load_document(Handle(), bytes, new UIntPtr((ulong)bytes.Length)),
                "ds_runtime_load_document");
        }

        /// Loads a `.dsb` by mapping it from `path`, bounded by the root shown.
        ///
        /// No payload is copied, and the only bytes touched out of the file's
        /// cold half are the assets the shown root's subtree draws.
        ///
        /// **The mapping is the runtime's.** Do not unlink or rewrite the file
        /// while it is loaded. A load that FAILS releases nothing, so a refused
        /// load leaves the previously loaded document drawable and its mapping
        /// held — do not unlink the previous file until a later load has
        /// succeeded.
        ///
        /// `shownRoot` is a document ordinal and is required; there is no value
        /// meaning "every root". An ordinal past the last root is
        /// `DsStatus.NoSuchRoot` rather than a silent clamp.
        public void LoadDocumentMapped(string path, uint shownRoot)
        {
            if (path == null)
            {
                throw new ArgumentNullException(nameof(path));
            }

            // NUL-terminated UTF-8, which is what the header asks for. Done here
            // rather than by the default string marshaller, which would encode
            // as ANSI on some platforms and mangle any non-ASCII path.
            var encoded = Encoding.UTF8.GetBytes(path + "\0");

            Check(
                Native.ds_runtime_load_document_mapped(
                    Handle(), encoded, shownRoot, null, UIntPtr.Zero),
                "ds_runtime_load_document_mapped");
        }

        /// Advances the scene by `dt` seconds.
        ///
        /// Returns whether the generation moved, which is what says a frame is
        /// worth drawing.
        ///
        /// **This call may touch the attached surface**, so it takes the same
        /// "no other call in flight on this runtime" rule as everything else
        /// rather than being a scene-only call you could make beside a draw.
        public bool Tick(float dt)
        {
            Check(Native.ds_runtime_tick(Handle(), dt, out var advanced), "ds_runtime_tick");
            return advanced != 0;
        }

        /// Takes a lease on the committed frame.
        ///
        /// Requires a document. A tick is not required: loading commits, so a
        /// frame is available before the first `Tick` — and on a static document
        /// the first tick commits nothing, so it is the same frame.
        ///
        /// **Every array's stride is checked against this package's row sizes
        /// before the lease is handed out** (R-E17). A mismatch means the
        /// library and the package came from different commits; the lease is
        /// released and the frame refused, rather than rows being read at a
        /// layout that is not the one they hold.
        public FrameLease AcquireFrame()
        {
            if (_lease != null)
            {
                throw new InvalidOperationException(
                    "a frame lease is already outstanding on this runtime. Dispose it before "
                    + "acquiring another — every call that would commit is refused until then.");
            }

            Check(Native.ds_runtime_acquire_frame(Handle(), out var frame), "ds_runtime_acquire_frame");

            try
            {
                FrameLease.ValidateStrides(frame);
            }
            catch
            {
                // **Release before rethrowing.** The acquire succeeded, so a
                // lease is outstanding; throwing straight out would leave it
                // held and refuse every later tick for the life of the runtime.
                Native.ds_runtime_release_frame(_handle, 0, out _);
                throw;
            }

            _lease = new FrameLease(this, frame);
            return _lease;
        }

        internal void ReleaseLease(int drawn)
        {
            // **Clear only after the library has actually released.** Clearing
            // first and then throwing would leave the managed side believing no
            // lease is outstanding while the library still holds one — so
            // `Dispose` would skip its release branch, `ds_runtime_free` would
            // answer `DsStatus.FrameLeased`, and the runtime would leak.
            var status = Native.ds_runtime_release_frame(Handle(), drawn, out _);
            if (status == DsStatus.Ok)
            {
                _lease = null;
            }

            Check(status, "ds_runtime_release_frame");
        }

        /// Frees the runtime. Must run on the thread that created it.
        ///
        /// An outstanding lease is released first: `ds_runtime_free` is itself
        /// refused with `DsStatus.FrameLeased` while one is held, so a teardown
        /// that skipped this would fail on a path nobody tests.
        public void Dispose()
        {
            if (_handle == 0)
            {
                return;
            }

            DsStatus status;
            try
            {
                // **`finally`, because releasing can throw and the free must
                // still run.** `ds_runtime_release_frame` answering
                // `DsStatus.Panic` is reachable, and the header's own remedy for
                // a panic is to free the runtime and make no further calls on
                // it — which is exactly what an unguarded throw here would skip.
                if (_lease != null)
                {
                    _lease.Dispose();
                }
            }
            finally
            {
                status = Native.ds_runtime_free(_handle);

                // **Cleared only when the runtime was actually freed.** Every
                // status below means it was not, and zeroing the handle anyway
                // would make the retry this method's own exception message
                // prescribes hit the `_handle == 0` guard above and do nothing —
                // turning a reported failure into an unrecoverable leak.
                if (status == DsStatus.Ok)
                {
                    _handle = 0;
                }
            }

            if (status == DsStatus.Ok)
            {
                return;
            }

            // Freeing is the one call where throwing is worse than reporting,
            // so this reports rather than propagating a `DashsceneException`: a
            // `using` block that threw here would mask the exception that caused
            // the unwind. But it does not go silent. Every status that reaches
            // this point means the runtime was NOT freed — `WrongThread` from a
            // foreign thread, `FrameLeased` from a release this class failed to
            // complete, `BadHandle` from a double dispose — and a silent leak is
            // what this type exists to avoid.
            var detail = status == DsStatus.WrongThread
                ? $"it was created on managed thread {_ownerThreadId} and disposed on "
                  + $"{Thread.CurrentThread.ManagedThreadId}; a dashscene runtime is "
                  + "thread-affine, so dispose it on its own thread"
                : DashsceneException.LastMessage();

            throw new InvalidOperationException(
                $"ds_runtime_free answered {status}, so the runtime has NOT been freed"
                + (detail.Length == 0 ? "." : $": {detail}."));
        }

        private ulong Handle()
        {
            if (_handle == 0)
            {
                throw new ObjectDisposedException(nameof(DashsceneRuntime));
            }

            return _handle;
        }

        private void Check(DsStatus status, string operation)
        {
            if (status == DsStatus.Ok)
            {
                return;
            }

            if (status == DsStatus.WrongThread)
            {
                throw new DashsceneException(
                    status,
                    operation,
                    $"the runtime was created on managed thread {_ownerThreadId} and this call "
                    + $"came from {Thread.CurrentThread.ManagedThreadId}");
            }

            DashsceneException.ThrowIfFailed(status, operation);
        }
    }
}
