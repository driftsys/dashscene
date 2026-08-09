package dev.driftsys.dashscene;

import android.app.Activity;
import android.os.Bundle;
import android.util.Log;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.ViewGroup;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;

/**
 * The smallest host that exercises layer 0: a SurfaceView, the three lifecycle
 * callbacks, and a compiled .dsb.
 *
 * <p><b>This is a lifecycle harness, not the demonstration.</b> Story #842's
 * {@code demo-android} is the demonstration — the showcase scenes, driven by
 * their own pulse, with the frame-timing instrument — and it waits for target
 * hardware because its deliverable is a frame-rate number. What this exists for
 * is the part that needs no hardware: proving a Surface reaches the painter, and
 * that rotation, backgrounding and split-screen run the destroy handshake
 * without a use-after-free.
 *
 * <p>A plain SurfaceView rather than Compose's {@code AndroidExternalSurface}.
 * D5 chose SurfaceView <i>semantics</i>, and both arrive at the same
 * {@code android.view.Surface} and therefore at the same handle type — so the
 * choice between them costs nothing here, and the plain one needs no Kotlin and
 * no Compose runtime to build.
 */
public final class HarnessActivity extends Activity implements SurfaceHolder.Callback {
    private static final String TAG = "dashscene";

    /** The document this harness draws, from the APK's assets. */
    private static final String DOCUMENT = "scene.dsb";

    static {
        System.loadLibrary("dashscene_android");
    }

    private long handle = 0;
    private byte[] document = null;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);

        Log.i(TAG, "harness: dashscene ABI " + DashsceneNative.nativeAbiVersion());
        try {
            document = readAsset(DOCUMENT);
            Log.i(TAG, "harness: " + DOCUMENT + " is " + document.length + " bytes");
        } catch (IOException error) {
            Log.e(TAG, "harness: could not read " + DOCUMENT, error);
        }

        SurfaceView view = new SurfaceView(this);
        view.getHolder().addCallback(this);
        view.setLayoutParams(new ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        setContentView(view);
    }

    @Override
    public void surfaceCreated(SurfaceHolder holder) {
        // Deliberately nothing. The extent is not known until surfaceChanged,
        // and a scene built for a zero drawable configures no swapchain.
        Log.i(TAG, "harness: surfaceCreated");
    }

    @Override
    public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
        Log.i(TAG, "harness: surfaceChanged " + width + "x" + height);
        if (document == null) {
            return;
        }
        if (handle == 0) {
            // Physical pixels, which is what surfaceChanged reports and what the
            // ABI's resize takes.
            handle = DashsceneNative.nativeSurfaceCreated(
                    holder.getSurface(), document, width, height);
            Log.i(TAG, "harness: runtime handle " + handle);
        } else {
            DashsceneNative.nativeSurfaceChanged(handle, width, height);
        }
    }

    @Override
    public void surfaceDestroyed(SurfaceHolder holder) {
        Log.i(TAG, "harness: surfaceDestroyed — entering the handshake");
        if (handle != 0) {
            // Blocks until the frame loop has stopped and the surface has been
            // dropped. Returning from this callback before that is the
            // use-after-free D4 names, so there is nothing asynchronous here on
            // purpose.
            DashsceneNative.nativeSurfaceDestroyed(handle);
            handle = 0;
        }
        Log.i(TAG, "harness: surfaceDestroyed — handshake complete, returning");
    }

    private byte[] readAsset(String name) throws IOException {
        try (InputStream in = getAssets().open(name)) {
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            byte[] chunk = new byte[16 * 1024];
            int read;
            while ((read = in.read(chunk)) != -1) {
                out.write(chunk, 0, read);
            }
            return out.toByteArray();
        }
    }
}
