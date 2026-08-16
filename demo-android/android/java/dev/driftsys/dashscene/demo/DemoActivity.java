package dev.driftsys.dashscene.demo;

import android.app.Activity;
import android.os.Bundle;
import android.util.Log;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.ViewGroup;

/**
 * The showcase on Android: the same demonstration {@code demo} and
 * {@code demo-web} run.
 *
 * <p>Which scene is a launch parameter rather than a keystroke, because Android
 * has no command line and no keyboard:
 *
 * <pre>adb shell am start -n ... --es scene typography</pre>
 *
 * <p>An absent or unknown name draws the first scene rather than failing the
 * launch. Touch input is not wired: that would be layer 1, app state writing
 * signals, and the showcase writes its own from Rust.
 */
public final class DemoActivity extends Activity implements SurfaceHolder.Callback {
    private static final String TAG = "dashscene";

    static {
        System.loadLibrary("demo_android");
    }

    private long handle = 0;
    private String scene = null;

    /**
     * Set when {@code surfaceChanged} declined to start the loop because the
     * extent it reported described no drawable (issue #1154).
     *
     * <p>Distinguishes a surface that is waiting for a real extent from one
     * whose {@code nativeStart} returned zero, which look identical from
     * {@code surfaceDestroyed}: {@code handle} is 0 in both, and only the
     * second is a failure. {@code HarnessActivity} carries the same field for
     * the same reason.
     */
    private boolean awaitingDrawable = false;

    /**
     * Whether {@code nativeStart} has been called for this surface at all.
     *
     * <p>Without it {@code awaitingDrawable} can be set <em>after</em> a start
     * that already failed — {@code surfaceChanged(1080, 1920)} returning a zero
     * handle, then a {@code surfaceChanged(0, 0)} on the teardown transition,
     * which finds {@code handle} still 0 and takes the wait branch. A real JNI
     * failure would then be reported by {@code surfaceDestroyed} as an ordinary
     * lifecycle transition, which is the misreading {@code awaitingDrawable}
     * exists to prevent rather than to create.
     */
    private boolean startAttempted = false;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        if (getIntent() != null) {
            scene = getIntent().getStringExtra("scene");
        }
        Log.i(TAG, "demo: scene extra = " + scene);

        SurfaceView view = new SurfaceView(this);
        view.getHolder().addCallback(this);
        view.setLayoutParams(new ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        setContentView(view);
    }

    @Override
    public void surfaceCreated(SurfaceHolder holder) {
        // The extent is not known until surfaceChanged, and a scene built in
        // code needs an extent to build for.
        Log.i(TAG, "demo: surfaceCreated");
    }

    @Override
    public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
        Log.i(TAG, "demo: surfaceChanged " + width + "x" + height);
        if (handle == 0) {
            // **Nothing is started for a surface with no pixels** (issue
            // #1154). surfaceChanged reports 0x0 during teardown and on some
            // backgrounding transitions, and starting here would spawn a render
            // thread that acquires an adapter, a device and the whole pipeline
            // set — 0.74 s on an emulator for a release build and over 218 s for
            // a debug one (issue #960) — for a drawable that cannot be
            // configured, and would park a surfaceDestroyed arriving next behind
            // the whole of it.
            //
            // Returning costs nothing: this callback is the only thing that
            // starts the loop, and the framework calls it again with the real
            // extent. surfaceCreated's comment above has said the extent is not
            // known until here since this file was written; this is that
            // sentence enforced rather than stated.
            //
            // **Here rather than in the native half**, and issue #1154 rules
            // on it directly: `loop_::start` could hold the guard and would
            // close both hosts at once, but refusing the attach there was
            // written and removed in PR #1152 — `LoopState::start` answering
            // false stops a render thread that no later surfaceChanged
            // restarts, so the window would stay blank until the surface
            // cycled. The Java callback can simply wait, because the framework
            // calls it again. (HarnessActivity gives a second reason that is
            // its own and not this host's: it reaches the entry point twice per
            // callback and would attribute the refusal to the cascade. This
            // host calls nativeStart once.)
            //
            // `machine::publish_extent` already keeps a non-drawable extent out
            // of the cell on the resize path, so nativeResize below is covered
            // and only the seed was not.
            if (width <= 0 || height <= 0) {
                // Not over a start that was attempted and failed — see
                // startAttempted. That case is a fault and must keep the
                // marker that names one.
                awaitingDrawable = !startAttempted;
                Log.i(TAG, "demo: surfaceChanged reported no drawable — not starting yet");
                return;
            }
            awaitingDrawable = false;
            startAttempted = true;
            handle = DemoNative.nativeStart(holder.getSurface(), scene, width, height);
            Log.i(TAG, "demo: handle " + handle);
        } else {
            DemoNative.nativeResize(handle, width, height);
        }
    }

    @Override
    public void surfaceDestroyed(SurfaceHolder holder) {
        Log.i(TAG, "demo: surfaceDestroyed — entering the handshake");
        if (handle != 0) {
            DemoNative.nativeStop(handle);
            handle = 0;
            // Inside the guard for the reason HarnessActivity gives: logged
            // outside it, this marker claims a handshake for a callback that
            // handed nothing back (issue #1006). The two hosts log the same
            // lifecycle event and should not disagree about it.
            Log.i(TAG, "demo: surfaceDestroyed — handshake complete");
        } else if (awaitingDrawable) {
            // **A distinct marker, and the guard above is why** (issue #1154).
            // The line below means nativeStart could not obtain the window or
            // spawn the thread, which is a fault; a surface destroyed without
            // ever having carried a drawable extent is an ordinary lifecycle
            // transition. Falling through would report the second as the first,
            // with a diagnosis pointing at a JNI problem that did not happen.
            awaitingDrawable = false;
            Log.i(TAG, "demo: surfaceDestroyed — no drawable extent was ever reported, "
                    + "so nothing was started");
        } else {
            Log.i(TAG, "demo: surfaceDestroyed — no runtime handle, nothing to hand back");
        }
    }
}
