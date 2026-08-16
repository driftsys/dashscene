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

    /**
     * The one face the document's text is drawn with, staged by build.sh.
     *
     * <p>Four assets, because a .dsb carries neither a font nor a sheet and
     * <b>nothing bakes a sheet at run time</b>: the font file, the committed
     * MSDF sheet as a PNG and its metrics blob, and {@code cascade} — one
     * tab-separated line holding the family name and the CSS weight, which are
     * the only two values that cannot be read out of the bytes.
     *
     * <p>The names are the contract with build.sh and nothing else. The family
     * and the weight are deliberately <i>not</i> constants here: they are
     * chosen in build.sh beside the files they describe, so changing the font
     * is one edit rather than two that must agree (issue #969).
     */
    private static final String FONT = "face.font";

    private static final String ATLAS_PNG = "face-atlas.png";

    private static final String ATLAS_METRICS = "face-atlas.metrics";

    private static final String CASCADE = "cascade";

    static {
        System.loadLibrary("dashscene_android");
    }

    private long handle = 0;
    private byte[] document = null;

    /**
     * Set when {@code surfaceChanged} declined to start the loop because the
     * extent it reported described no drawable (issue #1094).
     *
     * <p>Distinguishes a surface that is waiting for a real extent from one
     * whose {@code nativeSurfaceCreated} failed, which look identical from
     * {@code surfaceDestroyed}: {@code handle} is 0 in both. Only the second is
     * a failure, and {@code just android-splitscreen} fails the run on the
     * marker that names it.
     */
    private boolean awaitingDrawable = false;

    /**
     * The cascade, or null when any part of it could not be read.
     *
     * <p>Null is not a failure to report and stop on: the entry point takes an
     * empty cascade, which is exactly {@code nativeSurfaceCreated}, and a
     * harness that refused to draw rectangles because a font was missing would
     * lose the lifecycle coverage that is its actual purpose. What it must not
     * do is claim the text path ran, which is why the marker below names which
     * of the two it took.
     */
    private DsFace cascade = null;

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
        cascade = readCascade();

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
            // **Nothing is started for a surface with no pixels** (issue #1094).
            // surfaceChanged reports 0x0 during teardown and on some
            // backgrounding transitions, and starting here would spawn a render
            // thread that acquires an adapter, a device and the whole pipeline
            // set — 0.74 s on an emulator for a release build and over 218 s for
            // a debug one (issue #960) — for a drawable that cannot be
            // configured, and would park a surfaceDestroyed arriving next behind
            // the whole of it.
            //
            // Returning costs nothing: this callback is the only thing that
            // starts the loop, and the framework calls it again with the real
            // extent.
            //
            // **Here rather than in the native half, which could do it.**
            // loop_::start has spawned nothing when it is called, so returning
            // null there is a path both hosts already handle — it would close
            // this host, DemoActivity and any future one at once. It is not done
            // there because this host reaches the entry point twice per
            // callback, once with the cascade and once without, and would report
            // the second refusal as "the cascade was refused" when the cascade
            // is not what was wrong. Logcat is the only witness a device gives.
            // LoopState::start is a different matter again: refusing *there*
            // stops a render thread that no later surfaceChanged restarts.
            //
            // surfaceCreated's comment above has said the extent is not known
            // until here since this file was written; this is that sentence
            // enforced rather than stated.
            if (width <= 0 || height <= 0) {
                // **A distinct marker, and `just android-splitscreen` is why.**
                // That recipe fails the run on `no runtime handle, nothing to
                // hand back`, whose whole meaning is that nativeSurfaceCreated
                // could not obtain the window or spawn the thread. Falling
                // through to it here would report a benign wait as that
                // failure, with a diagnosis pointing at a JNI problem that did
                // not happen. `awaitingDrawable` is what keeps the two apart in
                // surfaceDestroyed below.
                awaitingDrawable = true;
                Log.i(TAG, "harness: surfaceChanged reported no drawable — not starting yet");
                return;
            }
            awaitingDrawable = false;
            // Physical pixels, which is what surfaceChanged reports and what the
            // ABI's resize takes.
            //
            // **The text entry point, with a real cascade** — issue #969. It
            // was compiled and called by nothing, so a device run measured the
            // path that draws no glyphs while the one an embedder with text
            // would use had never executed. `nativeSurfaceCreated` is this call
            // with no faces, so taking the other branch below is not a second
            // code path so much as the empty case of this one.
            if (cascade != null) {
                handle = DashsceneNative.nativeSurfaceCreatedWithText(
                        holder.getSurface(),
                        document,
                        new DsFace[] {cascade},
                        width,
                        height);
                Log.i(TAG, "harness: runtime handle " + handle + " (with text: "
                        + cascade.family + " " + cascade.weight + ")");
            }
            // **A refused cascade falls back to the no-text call**, and this
            // is not belt-and-braces. The four assets can read cleanly and the
            // ABI still reject the face — metrics that do not decode, a PNG
            // whose extent disagrees with them, a weight outside 1..=1000 —
            // and the entry point answers 0 for all of it. Without this the
            // harness would then draw nothing at all, which costs the
            // lifecycle coverage that is its whole purpose, and the zero
            // handle would take a branch whose own comment says a zero handle
            // means the window or the thread could not be obtained. Reading
            // the assets is not the only way a cascade fails.
            if (handle == 0) {
                handle = DashsceneNative.nativeSurfaceCreated(
                        holder.getSurface(), document, width, height);
                Log.i(TAG, "harness: runtime handle " + handle + " (no glyphs — "
                        + (cascade == null
                                ? "no cascade was staged"
                                : "the cascade was refused; check the log above")
                        + ", so text lays out as empty leaves)");
            }
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
            // Logged inside the guard, because this marker is what
            // `just android-splitscreen` reads as proof the handshake ran.
            // Until 2026-08-15 it sat outside, so a callback that handed
            // nothing back still claimed it had (issue #1006).
            Log.i(TAG, "harness: surfaceDestroyed — handshake complete, returning");
        } else if (awaitingDrawable) {
            // Ordinary, and deliberately NOT the marker below: no drawable
            // extent was ever reported for this surface, so nothing was started
            // and there is nothing to hand back. A surface destroyed without
            // ever carrying pixels is a lifecycle transition, not a fault
            // (issue #1094).
            awaitingDrawable = false;
            Log.i(TAG, "harness: surfaceDestroyed — no drawable extent was ever reported, "
                    + "so nothing was started");
        } else {
            // **Three causes, and only the third is a failure.** The surface
            // never carried a drawable extent, so surfaceChanged returned
            // without starting anything (issue #1094); the document could not
            // be read, so it returned before that; or nativeSurfaceCreated
            // could not obtain the window or spawn the thread — see
            // start_document_host. The first two are ordinary and expected on a
            // teardown or backgrounding transition.
            //
            // It is NOT the case where the painter never got a device: that
            // returns a non-zero handle and takes the branch above.
            //
            // **nativeIsRunning does not tell those apart, and this activity
            // deliberately does not call it.** It answers `Handshake::is_running`,
            // which is true for `Starting` as well as `Running`, and the render
            // thread reports `started()` only once its attach has returned. So
            // a thread wedged inside an attach answers `true` — the same answer
            // a drawing loop gives — and a marker built on it would assert the
            // loop was live at the exact moment it was not. Measured on
            // 2026-08-15 (issue #960).
            //
            // What does tell them apart is the native markers around the
            // attach, read as three cases rather than two (issue #1080).
            // `attaching a WxH surface` precedes every acquisition;
            // `attached a WxH surface` follows one that succeeded; and
            // `attach failed:` — or `could not rebuild the surface:` — follows
            // one that finished and failed, with the loop already stopped.
            // **Only `attaching` followed by none of those is a wedged
            // acquisition.** A missing `attached` on its own is not: that
            // reports every failed attach as a wedge.
            //
            // `surfaceDestroyed has been waiting N s` names the wait it holds.
            //
            // A distinct marker rather than silence, so the recipe can tell
            // this apart from a handshake that entered and hung.
            Log.i(TAG, "harness: surfaceDestroyed — no runtime handle, nothing to hand back");
        }
    }

    /**
     * Reads the one face build.sh staged, or null if any part of it is absent
     * or malformed.
     *
     * <p>All four assets or none. A cascade assembled from three of them would
     * declare a face whose sheet was missing, and the ABI refuses a
     * half-described face on purpose — so a partial read here would turn a
     * staging mistake into a load failure at the surface, which is much further
     * from its cause.
     */
    private DsFace readCascade() {
        try {
            String[] fields = new String(readAsset(CASCADE), "UTF-8").trim().split("\t");
            if (fields.length != 2) {
                Log.e(TAG, "harness: " + CASCADE + " is not 'family<TAB>weight'");
                return null;
            }
            // Not clamped and not defaulted. The ABI checks the CSS range in one
            // place; a weight repaired here would make this host and a C host
            // give different answers to the same input, which is the divergence
            // story #947's review removed from the JNI layer.
            int weight = Integer.parseInt(fields[1].trim());
            // faceIndex 0, written down rather than defaulted: the harness
            // stages an .otf rather than a .ttc, so there is no second face to
            // name. The index this entry point gained in issue #981 is
            // exercised by a collection, and no committed corpus font is one.
            DsFace read = new DsFace(
                    fields[0].trim(),
                    weight,
                    0,
                    readAsset(FONT),
                    readAsset(ATLAS_PNG),
                    readAsset(ATLAS_METRICS));
            Log.i(TAG, "harness: cascade " + read.family + " " + read.weight + " — font "
                    + read.font.length + " B, sheet " + read.atlasPng.length + " B, metrics "
                    + read.atlasMetrics.length + " B");
            return read;
        } catch (IOException | NumberFormatException error) {
            Log.e(TAG, "harness: no cascade; the document's text will draw no glyphs", error);
            return null;
        }
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
