package dev.driftsys.dashscene.demo;

import android.app.Activity;
import android.content.pm.ActivityInfo;
import android.graphics.Color;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import android.view.GestureDetector;
import android.view.Gravity;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.ViewGroup;
import android.view.WindowManager;
import android.view.WindowInsets;
import android.widget.FrameLayout;
import android.widget.TextView;

/**
 * The showcase on Android: the same demonstration {@code demo} and
 * {@code demo-web} run.
 *
 * <p>Which scene a launch OPENS on is a launch parameter, because Android has
 * no command line:
 *
 * <pre>adb shell am start -n ... --es scene typography</pre>
 *
 * <p>An absent or unknown name draws the first scene rather than failing the
 * launch.
 *
 * <p>Which scene is SHOWING is not fixed for the run. The page keys and a
 * vertical swipe walk the entries, and the whole vocabulary — gestures and the
 * key events an {@code adb}-driven run sends — is
 * {@code docs/decisions/the-showcase-hosts-share-one-surface.md}.
 *
 * <p><b>Input is wired, and the signal is still written from Rust.</b> This
 * class forwards a gesture or a key as one command; it authors nothing. The
 * write goes through {@code showcase::input}, which every host that draws these
 * scenes calls, so the desktop, this host and the Unity sample share one
 * vocabulary rather than three.
 * {@code docs/decisions/the-showcase-hosts-share-one-surface.md} carries the
 * bindings, and {@code DemoNative} carries the command codes.
 *
 * <p>A capture launch photographs one state instead of running the
 * demonstration:
 *
 * <pre>adb shell am start -n ... --es capture_scene layout \
 *     --ei capture_phase 2 --ef capture_signal 0.5</pre>
 */
public final class DemoActivity extends Activity implements SurfaceHolder.Callback {
    private static final String TAG = "dashscene";

    static {
        System.loadLibrary("demo_android");
    }

    private long handle = 0;
    private String scene = null;

    /** The three capture extras, or their absent sentinels. */
    private String captureScene = null;

    private int capturePhase = -1;
    private float captureSignal = Float.NaN;

    /**
     * The frame-cost readout, drawn over the surface rather than into it.
     *
     * <p>A {@code TextView} above the {@code SurfaceView}, not nodes appended to
     * the scene: a readout inside the document would be measured by the very
     * instrument it reports. It still composites into {@code adb screencap},
     * which is why a capture launch hides it and why the vocabulary has a
     * command to toggle it.
     */
    private TextView readout = null;

    private boolean readoutVisible = true;

    private GestureDetector gestures = null;

    private final Handler readoutPump = new Handler(Looper.getMainLooper());

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
            captureScene = getIntent().getStringExtra("capture_scene");
            capturePhase = getIntent().getIntExtra("capture_phase", -1);
            captureSignal = getIntent().getFloatExtra("capture_signal", Float.NaN);
        }
        Log.i(TAG, "demo: scene extra = " + scene);
        if (captureScene != null) {
            Log.i(TAG, "demo: capture extras = " + captureScene + " phase " + capturePhase
                    + " signal " + captureSignal);
            // A capture is photographed, and the readout would composite into
            // the photograph.
            //
            // **The same three-way test the native half applies**, and not
            // `captureScene != null`. A partial set is not a capture — the
            // native half says so and runs the demonstration — so hiding the
            // readout on the name alone left a demonstration running with its
            // readout gone and no way back to it but finding the `R` key.
            // `-1` and `NaN` are the sentinels the two `getExtra` defaults
            // above return for an absent extra, which is what `Capture::parse`
            // rejects on the other side.
            boolean whole = capturePhase >= 0 && !Float.isNaN(captureSignal);
            if (whole) {
                readoutVisible = false;
            } else {
                Log.w(TAG, "demo: capture_scene came without a usable phase and signal — "
                        + "running the demonstration, readout shown");
            }
        }

        // **Edge to edge, and the bars hidden.** The Unity player runs
        // fullscreen and measured 1080x2340 on this device where this host
        // measured 1080x1984 — the difference is the system bars. Two hosts at
        // two extents cannot have their frame costs compared or their frames
        // diffed, so the extent is not left to the default.
        getWindow().setDecorFitsSystemWindows(false);
        // **And into the cutout.** Hiding the bars alone left this host at
        // 2204x948 on a Pixel 5 whose display is 2340x1080; the remainder is
        // the cutout inset, which the default mode keeps the window out of.
        // The Unity player draws the whole display, and two hosts at two
        // extents can be neither compared nor diffed.
        WindowManager.LayoutParams attributes = getWindow().getAttributes();
        attributes.layoutInDisplayCutoutMode =
                WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS;
        getWindow().setAttributes(attributes);

        SurfaceView view = new SurfaceView(this);
        view.getHolder().addCallback(this);
        view.setLayoutParams(new ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));

        readout = new TextView(this);
        readout.setTextColor(Color.WHITE);
        readout.setBackgroundColor(0x99000000);
        readout.setTextSize(12.0f);
        readout.setPadding(24, 24, 24, 24);
        readout.setVisibility(readoutVisible ? TextView.VISIBLE : TextView.GONE);
        FrameLayout.LayoutParams readoutParams = new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        readoutParams.gravity = Gravity.TOP | Gravity.START;

        FrameLayout root = new FrameLayout(this);
        root.addView(view);
        root.addView(readout, readoutParams);
        setContentView(root);

        // **After setContentView, not before.** getInsetsController() reaches
        // through the decor view, and the decor view does not exist until the
        // content view is set — calling it in the other order is a null
        // dereference that force-finishes the activity before the surface is
        // ever created. Found by running it, not by reading it.
        hideSystemBars();

        // **Consumed at the root, so no child is inset.** Hiding the bars and
        // going edge to edge still left this host at 1080x2186 on a display
        // whose mMaxBounds is 1080x2340: the decor still dispatched the bar and
        // cutout insets down, and the SurfaceView sized itself inside them.
        // Consuming here is what makes the drawable the whole display.
        root.setOnApplyWindowInsetsListener((ignored, insets) -> WindowInsets.CONSUMED);

        gestures = new GestureDetector(this, new Bindings());
        // The whole vocabulary reaches the native half through DemoNative, so
        // a key and a gesture take the same path and the harness drives the
        // same code a hand does.
        root.setOnTouchListener((ignored, event) -> onTouchEvent(event));
        root.setFocusableInTouchMode(true);
        root.requestFocus();

        pumpReadout();
    }

    /**
     * Polls the render thread's published readout.
     *
     * <p>Polled rather than pushed: the sample completes on the render thread
     * every 240 frames, and a JNI call up to the UI thread would need a
     * {@code JavaVM} attach for a string this side can simply ask for.
     */
    private void pumpReadout() {
        readoutPump.postDelayed(() -> {
            if (readoutVisible && readout != null) {
                String text = DemoNative.nativeReadout();
                if (text != null && !text.isEmpty()) {
                    readout.setText(text);
                }
            }
            pumpReadout();
        }, 500);
    }

    @Override
    protected void onDestroy() {
        readoutPump.removeCallbacksAndMessages(null);
        super.onDestroy();
    }

    /**
     * Hides the system bars, and keeps them hidden.
     *
     * <p>Re-applied on every focus gain: the transient-bar behaviour brings
     * them back after a swipe or after another window takes focus, and a
     * drawable that changes size part-way through a measurement or a capture is
     * the one thing both of those must not do.
     */
    private void hideSystemBars() {
        if (getWindow().getInsetsController() == null) {
            return;
        }
        getWindow().getInsetsController().hide(WindowInsets.Type.systemBars());
        getWindow().getInsetsController().setSystemBarsBehavior(
                android.view.WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            hideSystemBars();
        }
    }

    /** Sends one command from the shared vocabulary. */
    private void command(int code) {
        switch (code) {
            case 3:
                // This side's, not the render thread's: setRequestedOrientation
                // is a UI-thread call on this activity.
                setRequestedOrientation(
                        getResources().getConfiguration().orientation
                                        == android.content.res.Configuration.ORIENTATION_PORTRAIT
                                ? ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
                                : ActivityInfo.SCREEN_ORIENTATION_PORTRAIT);
                Log.i(TAG, "demo: orientation requested");
                break;
            case 4:
                readoutVisible = !readoutVisible;
                readout.setVisibility(readoutVisible ? TextView.VISIBLE : TextView.GONE);
                Log.i(TAG, "demo: readout " + (readoutVisible ? "shown" : "hidden"));
                break;
            default:
                DemoNative.nativeCommand(code);
                break;
        }
    }

    @Override
    public boolean onTouchEvent(MotionEvent event) {
        // Two fingers down is the orientation command, and is checked before
        // the detector so a two-finger gesture is never also read as a tap.
        if (event.getPointerCount() == 2 && event.getActionMasked() == MotionEvent.ACTION_POINTER_DOWN) {
            command(3);
            return true;
        }
        if (event.getActionMasked() == MotionEvent.ACTION_MOVE && event.getPointerCount() == 1) {
            DemoNative.nativeDrag(event.getX());
        }
        return gestures.onTouchEvent(event) || super.onTouchEvent(event);
    }

    @Override
    public boolean onKeyDown(int code, KeyEvent event) {
        // The bindings the measurement harness sends. The two arrow keys drive
        // the signal rather than navigating, which is demo/src/input.rs's
        // binding and the one the contract adopts; navigation is on the page
        // keys.
        switch (code) {
            case KeyEvent.KEYCODE_DPAD_LEFT:
                DemoNative.nativeDrag(0.0f);
                return true;
            case KeyEvent.KEYCODE_DPAD_RIGHT:
                DemoNative.nativeDrag(Float.MAX_VALUE);
                return true;
            case KeyEvent.KEYCODE_PAGE_DOWN:
                command(0);
                return true;
            case KeyEvent.KEYCODE_PAGE_UP:
                command(1);
                return true;
            case KeyEvent.KEYCODE_SPACE:
                command(2);
                return true;
            case KeyEvent.KEYCODE_DPAD_UP:
                command(3);
                return true;
            case KeyEvent.KEYCODE_R:
                command(4);
                return true;
            default:
                return super.onKeyDown(code, event);
        }
    }

    /** Gestures, mapped onto the same commands the keys send. */
    private final class Bindings extends GestureDetector.SimpleOnGestureListener {
        /** Below this, a fling is a tap that moved rather than a swipe. */
        private static final float SWIPE_MIN_PX = 120.0f;

        @Override
        public boolean onDown(MotionEvent event) {
            return true;
        }

        @Override
        public boolean onSingleTapUp(MotionEvent event) {
            command(2);
            return true;
        }

        @Override
        public void onLongPress(MotionEvent event) {
            command(4);
        }

        @Override
        public boolean onFling(MotionEvent down, MotionEvent up, float vx, float vy) {
            if (down == null || up == null) {
                return false;
            }
            float dy = up.getY() - down.getY();
            if (Math.abs(dy) < SWIPE_MIN_PX || Math.abs(dy) < Math.abs(up.getX() - down.getX())) {
                // A horizontal movement is the signal's, and the drag path has
                // already written it.
                return false;
            }
            command(dy < 0 ? 0 : 1);
            return true;
        }
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
            handle = DemoNative.nativeStart(
                    holder.getSurface(), scene, width, height,
                    captureScene, capturePhase, captureSignal);
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
