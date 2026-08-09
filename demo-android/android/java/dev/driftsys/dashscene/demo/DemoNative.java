package dev.driftsys.dashscene.demo;

import android.view.Surface;

/** The JNI surface `demo-android` exports. Implemented in Rust. */
public final class DemoNative {
    private DemoNative() {}

    /**
     * Starts the showcase on a Surface.
     *
     * @param scene one of the showcase's scene names, or null for the first
     * @return an opaque handle, or 0. A non-zero handle does not mean the loop
     *     came up — ask {@link #nativeIsRunning}.
     */
    public static native long nativeStart(Surface surface, String scene, int width, int height);

    /** Reports a new physical-pixel extent. */
    public static native void nativeResize(long handle, int width, int height);

    /**
     * Stops the loop and drops the surface. <b>Blocks</b> until both have
     * happened; call it from {@code surfaceDestroyed} and do not return from
     * that callback first.
     */
    public static native void nativeStop(long handle);

    /** Whether the frame loop is still live. */
    public static native boolean nativeIsRunning(long handle);
}
