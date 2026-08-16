package dev.driftsys.dashscene;

/**
 * One font face, as {@link DashsceneNative#nativeSurfaceCreatedWithText} takes
 * it.
 *
 * <p>This is the Java half of the C ABI's {@code DsFontFace}, field for field.
 * It replaced five parallel arrays, which could not carry {@code face_index} at
 * all (issue #981).
 *
 * <p><b>The argument for this shape lives in
 * {@code docs/design/host-integration.md}</b>, under "the same descriptor
 * rather than a subset of it", and is not restated here. It was written out
 * four times when it landed and two of its claims were wrong in all four
 * copies.
 *
 * <p>Public final fields and no accessors on purpose: the native side reads
 * them by name, so a getter would be a second name for the same value and only
 * the field name is load-bearing.
 *
 * <p><b>These names and types are checked against the native side</b> (issue
 * #1089). {@code crates/dashscene-android/src/face.rs} holds the one list the
 * JNI half reads, and its own test asserts that this file still declares every
 * entry in it. That test runs in {@code just test} on every platform, because
 * the JNI half compiles on none of them.
 *
 * <p>So renaming a field here, removing one, or changing its type fails the
 * sanity tier rather than compiling, packaging, installing, and failing as
 * {@code NoSuchFieldError} on the first frame with the handle coming back 0 and
 * no glyph drawn. The type counts as much as the name: {@code GetFieldID}
 * resolves a field by both.
 *
 * <p><b>Change it in {@code face.rs} too — and, for a type change, in
 * {@code host.rs}'s {@code jni_sig!} literal for that field.</b> All three are
 * held mechanically now, but by two different gates, and <b>which gate catches
 * a half-finished change depends on which half you finished</b> (issue #1096):
 *
 * <ul>
 * <li>this file and {@code face.rs} agree, {@code host.rs} does not — a
 *     <b>compile error</b>, from a {@code const} assertion, on {@code just
 *     android} and {@code just android-lint} only, because {@code host.rs}
 *     compiles on no other target;</li>
 * <li>{@code face.rs} and {@code host.rs} agree, this file does not — a
 *     <b>failing test</b>, {@code just test}, on every platform;</li>
 * <li>this file and {@code host.rs} agree, {@code face.rs} does not — both of
 *     the above fire.</li>
 * </ul>
 *
 * <p>So neither gate alone covers every permutation, and "it compiled" is not
 * the same claim as "the three agree". Run {@code just test} and {@code just
 * android} before pushing a type change.
 *
 * <p>The check looks for each declaration it expects, with whitespace
 * <i>between tokens</i> collapsed — so extra spaces and line breaks between
 * words are fine, while {@code weight ;} or {@code byte [] font} are not found
 * even though Java accepts them. That is a red test rather than a broken build,
 * and the failure message says which it is.
 *
 * <p><b>It is looked for in this class's own body</b> (issue #1097). A
 * declaration moved into a nested class, or into a second top-level class in
 * this file, is not one {@code GetFieldID} can resolve on a {@code DsFace}
 * instance and does not satisfy the check. Neither does one that survives only
 * inside a string literal.
 *
 * <p><b>It does not check the other direction</b>: adding a seventh field here
 * fails nothing, because the native half reads these six and would simply
 * ignore it.
 */
public final class DsFace {
    /** Family name. Faces sharing one become a family however they are ordered. */
    public final String family;

    /** CSS weight, 1..=1000. Checked by the ABI and by nothing here. */
    public final int weight;

    /**
     * The face's index inside a font collection; 0 for a single-face file.
     *
     * <p>The field this class exists for. Out of range for the descriptor —
     * negative — fails the load rather than being repaired.
     */
    public final int faceIndex;

    /** The font file's bytes. */
    public final byte[] font;

    /**
     * The committed MSDF sheet, or an empty array for none.
     *
     * <p>Either both of a face's sheets are empty or both are filled. Both
     * empty is the measure-only cascade: text is shaped and measured and no
     * glyph is drawn. One empty and the other filled is a half-described face,
     * which the ABI refuses on purpose rather than quietly dropping that
     * face's glyphs.
     *
     * <p><b>And the rule runs across the whole cascade, not only within a
     * face: either every face carries a sheet or none does.</b> The atlas list
     * is indexed by the font slot of the face that shaped a glyph, so a
     * cascade where one face has a sheet and another does not leaves the sheet
     * at a lower index than the face that owns it — and the glyphs sample the
     * wrong face rather than failing.
     */
    public final byte[] atlasPng;

    /** The sheet's metrics blob, under the same rule as {@link #atlasPng}. */
    public final byte[] atlasMetrics;

    /**
     * The only constructor, and deliberately so.
     *
     * <p>A five-argument overload defaulting {@code faceIndex} to 0 was
     * written first and removed: it is the shorter and more discoverable of
     * the two, and a host holding a {@code .ttc} that reached for it would get
     * silently back the exact behaviour this class exists to remove — the
     * collection's first face and no diagnostic. Writing 0 down is cheap;
     * having it chosen for you is what went wrong before.
     */
    public DsFace(
            String family,
            int weight,
            int faceIndex,
            byte[] font,
            byte[] atlasPng,
            byte[] atlasMetrics) {
        this.family = family;
        this.weight = weight;
        this.faceIndex = faceIndex;
        this.font = font;
        this.atlasPng = atlasPng;
        this.atlasMetrics = atlasMetrics;
    }
}
