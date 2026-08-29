package dev.driftsys.dashscene.demo;

import android.view.Surface;

/** The JNI surface `demo-android` exports. Implemented in Rust. */
public final class DemoNative {
    private DemoNative() {}

    /**
     * Starts the showcase on a Surface.
     *
     * @param scene one of the showcase's scene names, or null for the first
     * @param captureScene the scene a capture launch photographs, or null
     * @param capturePhase the phase it holds, or -1 when absent
     * @param captureSignal the signal it holds, or NaN when absent
     * @return an opaque handle, or 0. A non-zero handle does not mean the loop
     *     came up — ask {@link #nativeIsRunning}.
     *
     * <p>The three capture parameters are taken together or not at all. A
     * capture with a defaulted phase or signal photographs a different state
     * than the other host is holding, so the native half refuses a partial set
     * and runs the demonstration instead.
     */
    public static native long nativeStart(
            Surface surface,
            String scene,
            int width,
            int height,
            String captureScene,
            int capturePhase,
            float captureSignal);

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

    /**
     * Queues one command from the shared showcase vocabulary for the render
     * thread to apply on its next frame.
     *
     * <p><b>No handle.</b> The queue on the other side is process-global,
     * because this demonstration runs one activity and one loop at a time. A
     * host running two would need a queue keyed by handle, and that is a change
     * to make when there is a second loop rather than in advance of one.
     *
     * @param code 0 next, 1 previous, 2 action, 3 orientation, 4 readout. The
     *     codes are the contract's; renumbering them rebinds every gesture and
     *     every key the measurement harness sends. Orientation and readout are
     *     handled on this side and are not sent.
     */
    public static native void nativeCommand(int code);

    /**
     * Reports where a horizontal drag currently is, in physical pixels.
     *
     * <p>Coalesced rather than queued on the other side: a drag produces a
     * MotionEvent per touch sample and only the latest names where the finger
     * is now, so a queue would replay a stale path one position per frame.
     */
    public static native void nativeDrag(float xPhysical);

    /**
     * The readout text the render thread last published, or an empty string
     * before the first sample completes.
     */
    public static native String nativeReadout();
}
