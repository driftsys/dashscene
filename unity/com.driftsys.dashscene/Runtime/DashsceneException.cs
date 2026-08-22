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
            : base($"dashscene ABI mismatch: this package was built against DS_ABI_VERSION "
                   + $"{expected} and the native library reports {actual}. "
                   + "The library and the C# package must come from one commit.")
        {
            Expected = expected;
            Actual = actual;
        }
    }
}
