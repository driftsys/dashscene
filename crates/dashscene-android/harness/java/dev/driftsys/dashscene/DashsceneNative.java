package dev.driftsys.dashscene;

import android.view.Surface;

/**
 * The JNI surface `dashscene-android` exports.
 *
 * <p>This class is the Java half of the binding and nothing else: every method
 * here is implemented in Rust, and the symbol names in the shared library are
 * derived from this exact package and class name. Renaming either breaks the
 * link at run time rather than at build time, which is why the name is stated
 * in the crate's own documentation too.
 *
 * <p>An embedder is free to write its own equivalent — the contract is the
 * symbol names, not this file — but there is no reason to.
 */
public final class DashsceneNative {
    private DashsceneNative() {}

    /**
     * Hands over a Surface and the document to draw, and starts the frame loop
     * on its own thread.
     *
     * @param surface  the Surface from SurfaceHolder.Callback
     * @param document the .dsb bytes
     * @param width    physical pixels
     * @param height   physical pixels
     * @return an opaque handle, or 0 if the window or the thread could not be
     *     obtained. <b>A non-zero handle does not mean the runtime started</b>
     *     — acquiring a GPU device takes on the order of a second, and blocking
     *     the UI thread inside {@code surfaceCreated} for that long risks an
     *     ANR, so the handle comes back as soon as the thread is spawned. Ask
     *     {@link #nativeIsRunning} whether the loop came up. A handle whose
     *     runtime failed is still valid and must still be passed to
     *     {@link #nativeSurfaceDestroyed}.
     */
    public static native long nativeSurfaceCreated(
            Surface surface, byte[] document, int width, int height);

    /**
     * Hands over a Surface, the document to draw, and the fonts its text needs,
     * and starts the frame loop on its own thread.
     *
     * <p>One {@link DsFace} per face, in cascade order. This took five parallel
     * arrays until issue #981, which could not carry {@code DsFontFace}'s
     * {@code face_index} at all. {@code docs/design/host-integration.md}
     * carries why the shape changed and what it costs.
     *
     * <p>An atlas is a committed MSDF sheet — a PNG and the metrics blob beside
     * it. <b>Nothing bakes one at run time</b>, so read them from your own
     * assets. {@link #nativeSurfaceCreated} is this call with no faces, and a
     * document loaded that way lays its text nodes out as empty leaves.
     *
     * <p>The sheets are optional, and either every face carries one or none
     * does. Give both of a face's sheets an empty array to declare a
     * measure-only cascade: its text is shaped and measured, and no glyph is
     * drawn. One of the two empty and the other filled is a face that
     * half-described its atlas, and the load fails rather than quietly
     * dropping that face's glyphs.
     *
     * <p>Every value is checked by the ABI rather than here, so a Kotlin host
     * and a C host get the same answer to the same input: a family that is
     * empty or only spaces, a weight outside 1..1000, or font bytes that do not
     * parse all fail the load rather than being repaired. What this method
     * refuses on its own is only what cannot cross to the descriptor at all — a
     * null face, a null field, a negative {@code faceIndex}, a weight a
     * {@code uint16_t} cannot hold, or a family carrying a NUL.
     *
     * @param faces one entry per face, in cascade order; an empty array is
     *     {@link #nativeSurfaceCreated}
     * @return an opaque handle, or 0. The same caveat as
     *     {@link #nativeSurfaceCreated}: a non-zero handle does not mean the
     *     runtime started.
     */
    public static native long nativeSurfaceCreatedWithText(
            Surface surface, byte[] document, DsFace[] faces, int width, int height);

    /**
     * As {@link #nativeSurfaceCreatedWithText}, but the document is
     * <b>mapped from a path</b> and only one root's assets are read (issue
     * #1035).
     *
     * <p>The byte-taking methods above read the whole file into the JVM heap,
     * copy it again into native memory, and then have every payload copied a
     * third time by the owning loader — including the payloads of artboards
     * nothing draws. This hands over a path, and the runtime reads out of the
     * file's cold half only what the named root's subtree needs. The cost of
     * opening a document then tracks the artboard being shown rather than the
     * file's size.
     *
     * <p><b>An APK asset is not a path.</b> An asset compressed inside the APK
     * cannot be mapped at all, and an uncompressed one is reachable only as a
     * file descriptor plus an offset and a length. So a host using this
     * extracts the document to app storage once — see
     * {@code HarnessActivity.documentPath} — and passes that. There is no
     * descriptor-taking variant yet.
     *
     * <p>The mapping belongs to the runtime and lasts until the document is
     * replaced or the runtime is freed, so the caller has no lifetime rule to
     * keep. <b>But the file must stay where it is</b> for as long as the
     * handle lives: app storage, not a cache directory the system may clear.
     *
     * @param path a filesystem path the process can read and map
     * @param shownRoot the document ordinal of the one root that will be drawn.
     *     Required, and there is no value meaning "every root": a bound that
     *     can be switched off reads as a bound when it is not one. A host that
     *     wants every root uses the byte-taking methods and pays the whole
     *     file knowingly.
     * @param faces one entry per face, in cascade order; an empty array loads
     *     without text
     * @return an opaque handle, or 0 for a null path, a path containing a NUL,
     *     a negative {@code shownRoot}, a face this method refuses, or a window
     *     or thread that could not be obtained. <b>Not for a document that
     *     fails to load</b>: the mapping and the load run on the render thread
     *     after this returns, so an unmappable path, a derived payload and a
     *     {@code shownRoot} naming no root all give a non-zero handle and then
     *     stop the loop, reported in logcat as {@code attach failed:} with the
     *     status and the path. The same caveat as
     *     {@link #nativeSurfaceCreated}, and for the same reason: a non-zero
     *     handle does not mean the runtime started.
     */
    public static native long nativeSurfaceCreatedMapped(
            Surface surface,
            String path,
            int shownRoot,
            DsFace[] faces,
            int width,
            int height);

    /** Reports a new physical-pixel extent. Picked up by the next frame. */
    public static native void nativeSurfaceChanged(long handle, int width, int height);

    /**
     * Stops the frame loop and drops the surface. <b>Blocks</b> until both have
     * happened.
     *
     * <p>Call this from {@code surfaceDestroyed} and do not return from that
     * callback before it returns. When {@code surfaceDestroyed} returns the
     * framework's Surface is invalid, and a render thread still holding a
     * surface built from it is a use-after-free on rotation, backgrounding and
     * split-screen. That is D4 of the host-integration record, and this call is
     * the whole of honouring it.
     *
     * <p>The handle is dead afterwards.
     */
    public static native void nativeSurfaceDestroyed(long handle);

    /**
     * The ABI generation the library was built with. Check it once and refuse a
     * library you do not recognise; the alternative is discovering the mismatch
     * as a corrupted argument.
     */
    public static native int nativeAbiVersion();

    /**
     * Whether the frame loop is still live.
     *
     * <p>False once it has ended, whether because teardown was requested or
     * because it stopped on its own — a failed tick or draw, or a device that
     * could not be obtained. This is the call that tells you a non-zero handle
     * from {@link #nativeSurfaceCreated} did not come up.
     */
    public static native boolean nativeIsRunning(long handle);
}
