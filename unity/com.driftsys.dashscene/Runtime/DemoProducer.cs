// The demonstration producer's entry points, and the managed calls over them
// (story #1342).
//
// **Everything here is behind `DASHSCENE_DEMO_PRODUCER` and off in every
// shipped configuration.** A player built from this package as a customer
// installs it compiles none of it, declares none of these imports, and calls
// nothing.
//
// Three things define the symbol, and each asks a different question:
// `just unity-demo`'s player build, which runs it; `just unity-ffi`'s second
// pass, which binds these declarations against the demo library; and
// `just unity-abi`'s second `package-compat` build, which is the only thing
// that compiles this file's real body at netstandard2.1, the way Unity will.
//
// **It is under `Runtime/` and not under `Samples~/` on purpose.**
// `unity/ffi-check` refuses a `[DllImport]` anywhere outside `Runtime/` minus
// `Runtime/Engine/`, because Unity compiles a sample into the customer's own
// assembly where the forwarder rule reaches nothing — which is issue #1308's
// class. Story #1342's second condition says so directly.
//
// **What these bind is not the shipped library.** `ds_demo_*` is exported by
// `unity/demo-producer`, which is `dashscene-ffi` linked as an rlib plus these
// six entry points, and `just unity-demo` stages it under the shipped library's
// own file name. That is not a disguise: the player must load ONE library, or
// `DashsceneRuntime` and `BuildDemoScene` would resolve into two instantiations
// of a `thread_local!` runtime table and no handle minted by one would resolve
// in the other. `just demo-exports` asserts that the staged library exports the
// shipped seventeen unchanged plus a set carrying only the `ds_demo_` prefix.
// `unity/ffi-check`'s demonstration pass is what holds these six by name, and
// drives each one.
//
// See `docs/decisions/the-demo-producer-links-the-abi-rather-than-shipping-in-it.md`.

#if DASHSCENE_DEMO_PRODUCER

using System;
using System.Runtime.InteropServices;
using System.Text;

namespace Driftsys.Dashscene
{
    /// The `ds_demo_*` entry points, bound the way `Native` binds the shipped
    /// ones: a private nested import class nobody else can name, and one
    /// forwarder each that turns a missing symbol into the package's own
    /// exception rather than letting `EntryPointNotFoundException` reach a host
    /// untranslated (issue #1308).
    internal static class DemoNative
    {
        internal static uint ds_demo_scene_count()
        {
            try
            {
                return Imports.ds_demo_scene_count();
            }
            catch (EntryPointNotFoundException e)
            {
                throw Native.SymbolMissing(e);
            }
        }

        internal static UIntPtr ds_demo_scene_name(uint index, byte[] buf, UIntPtr cap)
        {
            try
            {
                return Imports.ds_demo_scene_name(index, buf, cap);
            }
            catch (EntryPointNotFoundException e)
            {
                throw Native.SymbolMissing(e);
            }
        }

        internal static UIntPtr ds_demo_scene_summary(uint index, byte[] buf, UIntPtr cap)
        {
            try
            {
                return Imports.ds_demo_scene_summary(index, buf, cap);
            }
            catch (EntryPointNotFoundException e)
            {
                throw Native.SymbolMissing(e);
            }
        }

        internal static DsStatus ds_demo_build(ulong runtime, uint index, uint width, uint height)
        {
            try
            {
                return Imports.ds_demo_build(runtime, index, width, height);
            }
            catch (EntryPointNotFoundException e)
            {
                throw Native.SymbolMissing(e);
            }
        }

        internal static DsStatus ds_demo_pulse(ulong runtime, ulong phase)
        {
            try
            {
                return Imports.ds_demo_pulse(runtime, phase);
            }
            catch (EntryPointNotFoundException e)
            {
                throw Native.SymbolMissing(e);
            }
        }

        internal static DsStatus ds_demo_action(ulong runtime, out byte outRan)
        {
            try
            {
                return Imports.ds_demo_action(runtime, out outRan);
            }
            catch (EntryPointNotFoundException e)
            {
                throw Native.SymbolMissing(e);
            }
        }

        /// The `[DllImport]`s themselves, reachable from nowhere else.
        private static class Imports
        {
            [DllImport(Native.Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern uint ds_demo_scene_count();

            /// Returns the bytes the name needs including the terminator, so a
            /// null `buf` or a short one tells you what to allocate — the same
            /// contract `ds_last_error_message` has.
            [DllImport(Native.Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern UIntPtr ds_demo_scene_name(
                uint index, byte[] buf, UIntPtr cap);

            [DllImport(Native.Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern UIntPtr ds_demo_scene_summary(
                uint index, byte[] buf, UIntPtr cap);

            [DllImport(Native.Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_demo_build(
                ulong runtime, uint index, uint width, uint height);

            [DllImport(Native.Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_demo_pulse(ulong runtime, ulong phase);

            /// `outRan` is a `bool` in the producer and binds as `byte`, for the
            /// reason the note at the top of `Native.cs` gives: .NET's default
            /// marshalling for `bool` is the four-byte Win32 `BOOL`, which
            /// writes three bytes past a one-byte target.
            [DllImport(Native.Lib, CallingConvention = CallingConvention.Cdecl)]
            internal static extern DsStatus ds_demo_action(ulong runtime, out byte outRan);
        }
    }

    /// What the demonstration can draw: the `corpus/showcase` scenes the three
    /// Rust hosts draw, named by the library rather than listed here.
    ///
    /// A list written in C# would be a second definition that drifts from
    /// `showcase::SCENES`, and the comparison the Unity demonstration exists to
    /// make is against the document `demo-android` draws.
    public static class DemoScenes
    {
        /// How many scenes this library carries.
        public static int Count => (int)DemoNative.ds_demo_scene_count();

        /// The name of scene `index`, or the empty string past the end.
        public static string Name(int index) =>
            Read(index, DemoNative.ds_demo_scene_name);

        /// One line describing what scene `index` shows — what a viewer compares
        /// against the painter's refusals.
        public static string Summary(int index) =>
            Read(index, DemoNative.ds_demo_scene_summary);

        /// Sized, then read, then terminated — `DashsceneException.LastMessage`'s
        /// shape, for its reasons: the library reports what it needed rather
        /// than what it wrote, so the terminator is trusted over either count.
        private static string Read(int index, Func<uint, byte[], UIntPtr, UIntPtr> read)
        {
            if (index < 0)
            {
                return string.Empty;
            }

            var needed = read((uint)index, null, UIntPtr.Zero).ToUInt64();

            // The count includes the terminator, so 0 and 1 are both "no name" —
            // 0 being an index past the end, which is the one answer a caller
            // cannot mistake for a name.
            if (needed <= 1)
            {
                return string.Empty;
            }

            var buffer = new byte[needed];
            var written = read((uint)index, buffer, new UIntPtr(needed)).ToUInt64();

            var length = Array.IndexOf(buffer, (byte)0);
            if (length < 0)
            {
                length = (int)Math.Min(written, needed);
            }

            return Encoding.UTF8.GetString(buffer, 0, length);
        }
    }

    public sealed partial class DashsceneRuntime
    {
        /// Builds showcase scene `index` into this runtime and installs it as
        /// the loaded document.
        ///
        /// This is what the demonstration calls instead of `LoadDocument`.
        /// Afterwards `Tick`, `AcquireFrame` and `ReadAtlases` behave as they do
        /// over a loaded document — and the scene's own solver carries a
        /// typesetter and its atlases, which a plain `LoadDocument` does not
        /// (issue #863), so text in these scenes lays out and shades.
        public void BuildDemoScene(int index, uint width, uint height)
        {
            Check(
                DemoNative.ds_demo_build(Handle(), (uint)index, width, height),
                "ds_demo_build");
        }

        /// Applies the installed scene's scripted signal change for `phase`.
        ///
        /// The scene is a pure function of its phase, which is why this takes a
        /// number rather than a direction: the three Rust hosts re-apply the
        /// current phase after a rebuild on resize for exactly that reason.
        ///
        /// Stages, never commits. The write is visible to the next `Tick`.
        public void PulseDemoScene(ulong phase)
        {
            Check(DemoNative.ds_demo_pulse(Handle(), phase), "ds_demo_pulse");
        }

        /// Runs the installed scene's own variant switch.
        ///
        /// Returns whether there was one to run, so a host can say "this scene
        /// has no switch" rather than inventing a fallback — which is the seam
        /// `showcase::Showcase::action` exists to give it.
        public bool RunDemoAction()
        {
            Check(DemoNative.ds_demo_action(Handle(), out var ran), "ds_demo_action");
            return ran != 0;
        }
    }
}

#endif
