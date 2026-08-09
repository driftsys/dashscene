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
        }
        Log.i(TAG, "demo: surfaceDestroyed — handshake complete");
    }
}
