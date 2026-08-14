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
     * <p>The five arrays are parallel and must be the same length: one entry per
     * face. A length disagreement returns 0 and logs, rather than assembling a
     * cascade from entries that do not belong together.
     *
     * <p>There is no array for a face's index within a font collection: every
     * face is declared at index 0, so a .ttc reaches only its first face
     * through this method. The C ABI underneath carries the index; this
     * method is a subset of it.
     *
     * <p>An atlas is a committed MSDF sheet — a PNG and the metrics blob beside
     * it. <b>Nothing bakes one at run time</b>, so read them from your own
     * assets. {@link #nativeSurfaceCreated} is this call with no faces, and a
     * document loaded that way lays its text nodes out as empty leaves.
     *
     * <p>The sheets are optional, and either every face carries one or none
     * does. Pass an empty array for both of a face's sheets to declare a
     * measure-only cascade: its text is shaped and measured, and no glyph is
     * drawn. One of the two empty and the other filled is a face that
     * half-described its atlas, and the load fails rather than quietly
     * dropping that face's glyphs.
     *
     * @param families one family name per face; faces sharing a name become one
     *     family however they are ordered, matched after trimming and ignoring
     *     ASCII case, and a name that is empty or only spaces fails the load
     * @param weights CSS weight per face, parallel to families; must be in
     *     1..1000, and a value outside that range fails the load rather than
     *     being repaired — including 0, which no CSS weight can be
     * @param fonts the font file's bytes per face
     * @param atlasPngs the sheet per face, or an empty array for none
     * @param atlasMetrics the metrics blob per face, or an empty array for none
     * @return an opaque handle, or 0. The same caveat as
     *     {@link #nativeSurfaceCreated}: a non-zero handle does not mean the
     *     runtime started.
     */
    public static native long nativeSurfaceCreatedWithText(
            Surface surface, byte[] document, String[] families, int[] weights,
            byte[][] fonts, byte[][] atlasPngs, byte[][] atlasMetrics,
            int width, int height);

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
