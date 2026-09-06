//! The settle path: the host idles when the tick reports no advance, and the
//! heap binds when the binding goes stale rather than on every frame.
//!
//! **Text, for `paint_heap_binding.rs`'s reason.** `Runtime/Engine/` is
//! compiled by no CI job and `Samples~/` by nothing at all, so what this file
//! asserts is that the calls are still written the way story #1445 says.
//! `just unity-render` is what observes the consequence: its settle step drives
//! sixty Unity frames through `SettleLoop` and fails unless exactly one of them
//! drew, unless the captures after the first and the sixtieth are pixel
//! identical, and unless the heap rebinds on a changed drawable extent and on
//! nothing else.
//!
//! **Three questions, and each of them can be wrong on its own.** `Draw` can
//! bind unconditionally, which costs the same five `Material.Set…` calls per
//! material per frame the story removes and passes any scan that only asks
//! whether the binding happens. The flag can be raised by four of its five
//! reasons, which draws a correct picture until the reason that was missed
//! occurs — a reallocated buffer draws a freed one, a changed anti-aliasing
//! width draws the previous frame's. And a host loop can tick and then draw
//! anyway, which is the whole of what the story removes.
//!
//! **Every assertion about where a call sits is bounded by a member's own
//! braces**, through [`member_body`], which is `paint_heap_binding.rs`'s bound
//! and its reason: a first version of that file asked only that a call appear
//! somewhere under `Runtime/`, and three review seats each ran it green over a
//! painter that bound nothing.

use package_gate::cs_scan::{blank_comments_and_strings, member_body, squeeze};
use package_gate::{PAINTER_PATH, painter_source, root};

/// The three host loops this story routes through `SettleLoop`.
///
/// **All three from the start, and not the two the samples had.**
/// `DashsceneCanvasBaseline.cs` landed with PR #1463 carrying a settle skip
/// written inline, and its own comment says it is written to be replaced by
/// this class. A scan over two of the three would leave the third free to keep
/// a second copy of the decision, which is the one thing a shared class is for.
const LOOPS: [&str; 3] = [
    "Samples~/FrameLoop/DashsceneFrameLoop.cs",
    "Samples~/Showcase/DashsceneShowcase.cs",
    "Samples~/Showcase/DashsceneCanvasBaseline.cs",
];

/// The loop every one of the three writes.
const UPDATE: &str = "private void Update()";

/// One sample's source, comments and string bodies blanked.
///
/// Blanked for `painter_source`'s reason: the loops carry prose about the
/// decision they make, and an unblanked scan would report a comment naming
/// `ShouldDraw` as the call.
fn sample(rel: &str) -> String {
    let path = root().join("unity/com.driftsys.dashscene").join(rel);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} could not be read: {e}", path.display()));
    blank_comments_and_strings(&source)
}

/// `Draw` binds the heap once, and only when the binding is stale.
///
/// **The count and the guard are two assertions.** A second call site under no
/// guard binds on every frame with the guarded one still present, and a guard
/// over a call site that is no longer the only one says nothing.
#[test]
fn draw_binds_the_heap_exactly_once_and_only_when_pending() {
    let scanned = painter_source();
    let (start, end) = member_body(&scanned, "public void Draw(FrameLease lease)");
    let body = &scanned[start..=end];

    assert_eq!(
        body.matches("BindHeap()").count(),
        1,
        "{PAINTER_PATH}'s `Draw` calls `BindHeap()` {} time(s), not once. A \
         second call site binds on every frame whatever the first one is \
         guarded by.",
        body.matches("BindHeap()").count()
    );

    assert!(
        squeeze(body).contains("if (HeapBindingPending) { BindHeap(); }"),
        "{PAINTER_PATH}'s `Draw` does not guard its one `BindHeap()` on \
         `HeapBindingPending`. `BindHeapTo` makes five `Material.Set…` calls \
         per material per frame, and story #1445 is that those calls happen \
         when the binding goes stale rather than on every frame of a settled \
         scene."
    );

    // **And `BindHeap` clears it.** A flag that is raised by five reasons
    // and cleared by none is true for ever after the first frame, so the guard
    // above is written, correct, and never false — every drawn frame pays the
    // five `Material.Set…` calls per material that this story removes, and the
    // picture is right the whole time. Nothing else in CI can see that:
    // `BrgPainter` is `Runtime/Engine/`, which `unity/ffi-check` excludes and
    // no CI job compiles, so `just unity-render`'s settle step was the only
    // thing standing between that mutation and `main`.
    let (bind_start, bind_end) = member_body(&scanned, "private void BindHeap()");
    assert!(
        squeeze(&scanned[bind_start..=bind_end]).contains("_heapBindingPending = false;"),
        "{PAINTER_PATH}'s `BindHeap` does not clear `_heapBindingPending`, so \
         the guard in `Draw` is true on every frame after the first reason \
         raises it and the binding is redone on all of them."
    );

    // **The upload still comes first**, which `paint_heap_binding.rs` states
    // over the unguarded form and which the guard does not change: `Upload`
    // disposes and re-creates a `GraphicsBuffer` when its table outgrows it,
    // and it is that reallocation which raises the flag this guard reads. A
    // guard evaluated before the upload reads the PREVIOUS frame's answer.
    let uploaded = body
        .find("UploadHeap();")
        .unwrap_or_else(|| panic!("{PAINTER_PATH}'s `Draw` does not call `UploadHeap()`"));
    let bound = body.find("BindHeap()").expect("counted above");
    assert!(
        uploaded < bound,
        "{PAINTER_PATH}'s `Draw` reads `HeapBindingPending` at {bound} and \
         uploads the heap at {uploaded}. The flag is raised BY the upload, so \
         a guard evaluated first answers for the previous frame."
    );
}

/// Every reason the binding can go stale raises the flag.
///
/// **Five reasons and not one.** `BindHeap` binds three buffers, the gradient
/// strip and the scalars `(EdgeWidth, SolidBase, GradientBase, StripRows)`, and
/// the halves go stale for different reasons: a buffer is reallocated when its
/// table outgrows it, the strip texture is re-created when its row count moves,
/// and the scalars move with no reallocation at all — the anti-aliasing width
/// on every change of drawable extent, the gradient base whenever the paint
/// table interns a new solid. A flag raised by reallocation alone draws a
/// settled scene at the previous frame's edge width.
#[test]
fn the_pending_flag_is_raised_by_every_reason_the_binding_can_go_stale() {
    let scanned = painter_source();

    // **The reallocation is pinned where it is DECIDED, not where it is
    // returned.** A first version of this file asked only that `Upload` end in
    // `return reallocated;`, and a review seat deleted the one
    // `reallocated = true;` inside the growth branch — so the member disposed a
    // `GraphicsBuffer`, created a new one and reported that nothing had
    // changed, leaving every material naming freed native memory. All three
    // tests in this file and all six in `paint_heap_binding.rs` stayed green.
    // The sequence below pins the declaration, the condition that decides a
    // growth, and the assignment as that branch's first statement.
    let (start, end) = member_body(&scanned, "private static bool Upload(");
    let upload = squeeze(&scanned[start..=end]);
    assert!(
        upload.contains(
            "var reallocated = false; if (buffer == null || buffer.count < rows) \
             { reallocated = true;"
        ),
        "{PAINTER_PATH}'s `Upload` does not set `reallocated` inside the branch \
         that disposes and re-creates the buffer. A member that recreates a \
         `GraphicsBuffer` and reports no reallocation leaves every material \
         naming the freed one, and `return reallocated;` at the end of it says \
         nothing about that: {upload}"
    );

    for (member, needle) in [
        // `Upload` is `private static` over a `ref GraphicsBuffer`, so it
        // cannot set an instance field: it reports the reallocation instead.
        ("private static bool Upload(", "return reallocated;"),
        // The skip itself, whose "already there" direction freezes a table.
        (
            "private static bool Upload(",
            "if (HeapUpload.AlreadyUploaded(staging, source, uploaded, floats)) { return reallocated; }",
        ),
        (
            "private void UploadHeap()",
            "if (scalars != _boundScalars) { _heapBindingPending = true; }",
        ),
        (
            "public void SetAtlases(TextAtlasSet atlases)",
            "_heapBindingPending = true;",
        ),
        (
            "private void ReleaseAtlases()",
            "_heapBindingPending = true;",
        ),
        // Story #1449's fifth reason. A `Texture2D` cannot be resized, so a
        // strip whose row count moved is a NEW texture and every material still
        // names the previous one — the same failure a reallocated
        // `GraphicsBuffer` is, reached through a binding that is not a buffer.
        //
        // **The CONDITION, not the assignment alone**, which is the strengthening
        // `Upload`'s own needle above already carries and this one did not: a
        // review seat deleted the `_stripTexture.height != height` half, leaving
        // `if (_stripTexture == null)`, and the whole `package-gate` suite stayed
        // green. A document whose gradient count grows then samples the previous,
        // too-short texture at a v coordinate computed from the NEW row count —
        // every gradient's colour wrong, with the flag never raised so the rebind
        // never happens either.
        (
            "private void UploadStrip(FrameLease lease)",
            "if (_stripTexture == null || _stripTexture.height != height) { if \
             (_stripTexture != null) { UnityEngine.Object.DestroyImmediate(_stripTexture); }",
        ),
        (
            "private void UploadStrip(FrameLease lease)",
            "_stripRows = height; _stripUploaded = false; _heapBindingPending = true;",
        ),
        // And the strip's own skip, whose "already there" direction freezes the
        // document's gradients rather than the binding. The decision is
        // `GradientStripUpload.AlreadyUploaded`'s for `HeapUpload`'s reason —
        // this file's own subject — so what is pinned here is that
        // `UploadStrip` asks it rather than carrying a second copy that
        // `unity/ffi-check` cannot execute.
        (
            "private void UploadStrip(FrameLease lease)",
            "if (GradientStripUpload.AlreadyUploaded( _stripUploaded, lease.DocumentReplaced, \
             strip.Generation, _stripGeneration)) { return; }",
        ),
        // **The ORDER, as one span, and it is the whole of a defect that
        // shipped once.** The bake must be recorded before the empty-strip
        // return, not after the byte copy that an empty strip never reaches: a
        // document with no gradient otherwise records nothing and consumes no
        // replacement flag, so a later document whose own first gradient lands
        // on the same generation — the count restarts per document — is treated
        // as already uploaded and draws the previous document's colours.
        //
        // A needle per statement cannot say this. `member_body` plus
        // `squeeze(…).contains(…)` has no positional constraint between two
        // needles, so the three above are all satisfied by the broken order —
        // measured, by reverting `UploadStrip` to it and watching this file and
        // `paint_heap_binding.rs` stay green. One contiguous span is what
        // carries the ordering.
        (
            "private void UploadStrip(FrameLease lease)",
            "_stripGeneration = strip.Generation; _stripUploaded = true; if (rows == 0) \
             { return; }",
        ),
    ] {
        let (start, end) = member_body(&scanned, member);
        assert!(
            squeeze(&scanned[start..=end]).contains(needle),
            "{PAINTER_PATH}'s `{member}` does not carry `{needle}`, so that \
             reason leaves the binding stale and the next frame draws through \
             it."
        );
    }

    // **Every call site, not one of them.** `UploadHeap` uploads four tables
    // and any one of them can be the one that grows, so a `|=` on three of the
    // four call sites draws a freed buffer the first time the fourth table
    // outgrows its own.
    let (start, end) = member_body(&scanned, "private void UploadHeap()");
    let upload_heap = squeeze(&scanned[start..=end]);
    //
    // **`Upload(` and not `Upload(ref `**, because the argument list wraps:
    // `squeeze` leaves a space after the parenthesis wherever it does, and a
    // needle carrying the first argument would pin the line breaks as much as
    // the code.
    let calls = upload_heap.matches("Upload(").count();
    let raising = upload_heap
        .matches("_heapBindingPending |= Upload(")
        .count();
    assert_eq!(
        calls, 4,
        "{PAINTER_PATH}'s `UploadHeap` makes {calls} `Upload(…)` call(s), not \
         the four heap tables. The count below is stated over that set."
    );
    assert_eq!(
        raising, calls,
        "{PAINTER_PATH}'s `UploadHeap` makes {calls} `Upload(…)` call(s) and \
         takes the reallocation of {raising} of them. A table whose \
         reallocation is discarded leaves every material naming a freed \
         `GraphicsBuffer` from its first growth onward."
    );

    // **`SetAtlases` raises the flag before its first early return.** Its own
    // header says so, and until this assertion nothing held it to that: the
    // literal appears somewhere in the member whether it sits above the
    // `atlases == null` return or below it, and a rung-3 painter or an empty
    // set takes one of those returns. It is masked today because
    // `ReleaseAtlases` runs earlier and raises the flag itself, which is
    // incidental rather than the rule the header states.
    let (start, end) = member_body(&scanned, "public void SetAtlases(TextAtlasSet atlases)");
    let set_atlases = &scanned[start..=end];
    let raised = set_atlases
        .find("_heapBindingPending = true;")
        .expect("asserted above");
    let first_return = set_atlases
        .find("return;")
        .unwrap_or_else(|| panic!("{PAINTER_PATH}'s `SetAtlases` carries no early return"));
    assert!(
        raised < first_return,
        "{PAINTER_PATH}'s `SetAtlases` raises the binding flag at {raised}, \
         after its first early return at {first_return}. That return is the \
         empty-set path and the one below it is the rung-3 path, so a set \
         installed on either leaves the flag clear."
    );
}

/// The body an `if (…)` opens, braces matched, taken from the start of the
/// `if` itself.
///
/// `squeeze`d input, so the brace is one space after the condition's `)`. A
/// brace-less `if` has no block, and this repository's `Runtime/` carries none
/// — the panic says so rather than returning the rest of the member.
fn guarded_branch(from: &str) -> &str {
    let open = from
        .find('{')
        .unwrap_or_else(|| panic!("the settle guard opens no block: {from}"));
    let bytes = from.as_bytes();
    let mut depth = 0usize;
    for (offset, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &from[open..=offset];
                }
            }
            _ => {}
        }
    }
    panic!("the settle guard's block never closes: {from}");
}

/// Every host loop notes the extent, reads `Tick`'s return, and decides through
/// `SettleLoop` before it acquires anything.
///
/// **Before the acquire, and that is the whole of the saving.** A loop that
/// acquired and then decided would still take the lease, and the pack, the
/// upload and the bind all sit behind it. What is asserted is the order — the
/// extent, then the tick, then the decision, then the acquire — and that the
/// decision's own branch leaves the loop.
#[test]
fn every_host_loop_decides_through_settle_loop() {
    const NOTE: &str = "_settle.NoteExtent(Screen.width, Screen.height);";
    const TICK: &str = "var advanced = _runtime.Tick(dt);";
    const GUARD: &str = "if (!_settle.ShouldDraw(advanced))";
    const ACQUIRE: &str = "_runtime.AcquireFrame()";

    for rel in LOOPS {
        let scanned = sample(rel);
        let (start, end) = member_body(&scanned, UPDATE);
        let body = squeeze(&scanned[start..=end]);

        let note = body.find(NOTE).unwrap_or_else(|| {
            panic!(
                "{rel}'s `Update` does not carry `{NOTE}`. The drawable extent \
                 is the one forced-redraw reason the runtime cannot report: it \
                 is host-side, and a settled loop that never hears about a \
                 resize draws the previous size for ever."
            )
        });
        let tick = body.find(TICK).unwrap_or_else(|| {
            panic!(
                "{rel}'s `Update` does not carry `{TICK}`. `DashsceneRuntime.Tick` \
                 returns whether the generation moved, and a loop that discards \
                 it has nothing to settle on."
            )
        });
        let guard = body.find(GUARD).unwrap_or_else(|| {
            panic!(
                "{rel}'s `Update` does not carry `{GUARD}`. The decision is \
                 `SettleLoop`'s so that the three loops make one decision \
                 rather than three that drift apart."
            )
        });
        let acquire = body
            .find(ACQUIRE)
            .unwrap_or_else(|| panic!("{rel}'s `Update` does not call `{ACQUIRE}`"));

        assert!(
            note < tick && tick < guard && guard < acquire,
            "{rel}'s `Update` orders these as extent {note}, tick {tick}, \
             decision {guard}, acquire {acquire}. A tick read before the \
             extent is noted answers for the previous size, and a decision \
             taken after the acquire has already paid for the lease."
        );

        // **The guard's own branch, not the text between it and the
        // acquire.** A first version asked only that `return;` appear
        // somewhere in that span, which an unrelated `if (…) { return; }`
        // below an empty settle guard satisfies while the loop draws every
        // frame. This takes the block the guard opens, braces matched.
        let branch = guarded_branch(&body[guard..]);
        assert!(
            branch.contains("return;"),
            "{rel}'s `Update` does not leave the loop inside `{GUARD}`'s own \
             branch: `{branch}`. The acquire, the pack, the upload and the \
             bind all sit behind that branch, so a skip that does not return \
             skips nothing."
        );
    }
}
