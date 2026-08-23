// The managed form of a failed `ds_runtime_*` call.
//
// Every entry point on this ABI answers a `DsStatus`, and the description of
// what went wrong is in a separate per-call channel — `ds_last_error_message`.
// A wrapper that returns the status without reading that channel discards the
// only description of the failure, which is one of the two things story #1121
// exists to get right.

using System;
using System.Text;

namespace Driftsys.Dashscene
{
    /// A `ds_runtime_*` call that did not return `DsStatus.Ok`.
    ///
    /// **Branch on `Status`, never on `Message`.** The discriminants are the
    /// contract; the message is diagnostic and the header promises nothing
    /// about its text.
    public class DashsceneException : Exception
    {
        /// The discriminant the call returned. This is the contract.
        public DsStatus Status { get; }

        /// The library's description of the failure, empty when it offered none.
        ///
        /// Diagnostic only. Do not parse it.
        public string Detail { get; }

        internal DashsceneException(DsStatus status, string operation, string detail)
            : base(Describe(status, operation, detail))
        {
            Status = status;
            Detail = detail;
        }

        private static string Describe(DsStatus status, string operation, string detail)
        {
            return detail.Length == 0
                ? $"{operation} failed: {status}"
                : $"{operation} failed: {status} — {detail}";
        }

        /// Reads the last failure's message, or an empty string.
        ///
        /// **The two-call pattern the header specifies.** The first call sizes
        /// the message — including its terminator — and the second fills a
        /// buffer of that size. Asking with a fixed buffer instead would
        /// truncate exactly the long messages worth reading.
        internal static string LastMessage()
        {
            var needed = Native.ds_last_error_message(null, UIntPtr.Zero).ToUInt64();

            // The count includes the NUL terminator, so 0 and 1 are both "no
            // message" — 1 being a message that is only the terminator.
            if (needed <= 1)
            {
                return string.Empty;
            }

            var buffer = new byte[needed];
            var written = Native.ds_last_error_message(buffer, new UIntPtr(needed)).ToUInt64();

            // The library reports what it needed, not what it wrote, and a
            // second failure between the two calls could shorten it. Trust the
            // terminator rather than either count.
            var length = Array.IndexOf(buffer, (byte)0);
            if (length < 0)
            {
                length = (int)Math.Min(written, needed);
            }

            return Encoding.UTF8.GetString(buffer, 0, length);
        }

        /// Throws when `status` is not `Ok`, attaching the library's message.
        internal static void ThrowIfFailed(DsStatus status, string operation)
        {
            if (status != DsStatus.Ok)
            {
                throw new DashsceneException(status, operation, LastMessage());
            }
        }
    }

    /// The ABI the package was built against is not the one the library speaks.
    ///
    /// `docs/specification/07-embedding-and-distribution.md` R-E16 requires the
    /// host to refuse rather than proceed, and to report both numbers — so this
    /// carries them separately rather than only in the message. A binding that
    /// hard-codes the version and proceeds turns a mismatch into undefined
    /// behaviour instead of a refusal at startup.
    public class DashsceneAbiMismatchException : Exception
    {
        /// The value this C# was compiled against.
        public uint Expected { get; }

        /// The value the loaded library reports.
        public uint Actual { get; }

        internal DashsceneAbiMismatchException(uint expected, uint actual)
            : this(
                expected,
                actual,
                $"dashscene ABI mismatch: this package was built against DS_ABI_VERSION "
                + $"{expected} and the native library reports {actual}. "
                + "The library and the C# package must come from one commit.")
        {
        }

        /// For a mismatch the two numbers cannot express — see
        /// `DashsceneSymbolMissingException`.
        internal DashsceneAbiMismatchException(uint expected, uint actual, string message)
            : base(message)
        {
            Expected = expected;
            Actual = actual;
        }
    }

    /// The library is missing an entry point this package calls, and
    /// `ds_abi_version` **agreed**.
    ///
    /// **This is the one mismatch the version number cannot report, and that is
    /// by design rather than by oversight.** Adding a symbol does not move
    /// `DS_ABI_VERSION` — the rule at the top of
    /// `crates/dashscene-ffi/include/dashscene.h` — and it is right for a host
    /// built against an OLDER header, which keeps working because it calls
    /// nothing new. It says nothing about the other direction: a package built
    /// after a symbol was added, loaded against a library from before, passes
    /// the handshake and then fails at the first call to it.
    ///
    /// .NET binds a `DllImport` lazily, so that failure arrives as an
    /// `EntryPointNotFoundException` from inside an ordinary call rather than
    /// at load. Left alone it is not a `DashsceneException` either, so it
    /// escapes the `catch` a host was told to write. This is that failure,
    /// wearing R-E16's own type.
    ///
    /// **A host must catch it at every load site as well as around the
    /// constructor**, and that is the cost of this shape rather than a detail:
    /// `DashsceneAbiMismatchException` derives from `Exception`, so a
    /// `catch (DashsceneException)` around a load does **not** see it. The
    /// frame-loop sample carries both catches. Deriving from
    /// `DashsceneException` instead would have needed a `DsStatus`, and there
    /// is none to give honestly — no call reached the library.
    ///
    /// `Expected` and `Actual` are equal here, and that is the substance rather
    /// than a defect: they agreeing is exactly why the handshake let this
    /// through.
    public sealed class DashsceneSymbolMissingException : DashsceneAbiMismatchException
    {
        /// The entry point the loaded library does not export.
        public string Symbol { get; }

        /// `expected` is this package's constant; `actual` must be **read from
        /// the library**, because `Actual` is documented as the value the
        /// library reports and a caller logging it would otherwise publish a
        /// number nothing observed. They agree in practice — the constructor's
        /// handshake would have refused a disagreement — and that is a fact
        /// about the sequence rather than about this type, so it is not assumed
        /// here.
        internal DashsceneSymbolMissingException(string symbol, uint expected, uint actual)
            : base(
                expected,
                actual,
                $"the loaded '{Native.Lib}' exports no {symbol}, and it reports DS_ABI_VERSION "
                + $"{actual} against this package's {expected}. Adding a symbol does not move "
                + "that number, so a library older than the symbol passes the version check and "
                + "fails at the first call. Rebuild the native library from the commit this "
                + "package came from.")
        {
            Symbol = symbol;
        }
    }
}
