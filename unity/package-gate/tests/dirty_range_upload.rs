//! The instance buffer is uploaded as ranges on a partial pack, and whole on a
//! full one — and there is exactly one member that sends any of it.
//!
//! **A scan rather than an execution, for `painter_diagnostics`' reason**: no
//! CI job compiles `Runtime/Engine/`, which references `UnityEngine`. What the
//! halves of this file can and cannot say is therefore worth stating.
//! `unity/ffi-check` executes `StreamLayout`, `InstanceSpans` and
//! `InstanceUpload.CanSendRanges` — the span, coalescing, layout and decision
//! arithmetic — so what is left here is the SHAPE of the member that
//! composes them: which branch the whole-array upload sits in, that a ranged
//! loop exists and writes and sends one list, that nothing reaches the
//! `GraphicsBuffer` except through the one wrapper that counts what it sent,
//! and that the painter asks the predicate with its own readings in order.
//!
//! **What a scan cannot do is judge a boolean.** An earlier version of the
//! delegation test below asserted that three clause spellings appeared
//! somewhere in `UploadInstances`' body, which passes just as happily when the
//! `&&` joining them becomes `||`, and says nothing at all about a fourth
//! clause being deleted. That is why the predicate lives outside
//! `Runtime/Engine/` and this file checks the call rather than the condition.

use package_gate::{cs_scan, painter_source};

/// The whole-array upload is the whole path's, and the ranged loop is the
/// other one's.
///
/// **`member_body` brace-matches any substring's block**, an `if` or a `for`
/// included, so the second and third calls below return branches rather than
/// members.
#[test]
fn the_whole_array_upload_sits_inside_the_full_pack_branch_and_nowhere_else() {
    let scanned = painter_source();
    let (start, end) = cs_scan::member_body(&scanned, "private void UploadInstances()");
    let body = &scanned[start..end];

    let whole = "UploadRows(0, _staging.Length)";
    assert_eq!(
        body.matches(whole).count(),
        1,
        "UploadInstances sends the whole staging array in exactly one place"
    );

    let (branch_start, branch_end) = cs_scan::member_body(body, "if (!ranged)");
    let branch = &body[branch_start..branch_end];
    assert!(
        branch.contains(whole),
        "the whole-array upload is inside the whole-path branch, and this one holds: {branch}"
    );

    let (loop_start, loop_end) =
        cs_scan::member_body(body, "for (var r = 0; r < ranges.Count; r++)");
    let ranged = &body[loop_start..loop_end];
    assert!(
        !ranged.contains(whole),
        "the ranged loop sends ranges, and this one sends the whole array: {ranged}"
    );

    // **The written words and the sent words are one list.** `Ranges` fills
    // `_wordRanges`, `WriteStreams` writes at those offsets, and the loop under
    // it sends those same offsets. A version that derived the two separately
    // could put correct rows into the staging array and send a different slice
    // of it — which no golden catches, because every golden is drawn from a
    // full pack.
    let computed = ranged
        .find("StreamLayout.Ranges(")
        .expect("the ranged loop does not ask StreamLayout where the rows sit");
    let written = ranged
        .find("WriteStreams(")
        .expect("the ranged loop writes no rows");
    let sent = ranged
        .find("UploadRows(first, count)")
        .expect("the ranged loop sends nothing");
    assert!(
        computed < written && written < sent,
        "the word ranges are computed, then written into, then sent — in that order: {ranged}"
    );
    assert_eq!(
        ranged.matches("_wordRanges").count(),
        3,
        "one list of word ranges, named three times — filled, written into, and sent: {ranged}"
    );
}

/// Every byte of the instance buffer goes through the wrapper that records what
/// it sent.
///
/// **This is what makes `LastUpload` a reading rather than a claim.** A second
/// `SetData` call anywhere in the painter would send rows the counter never
/// saw, so the report would say `Ranges` while the whole array was sent — the
/// exact defect the ranged path exists to remove, reported as its own fix.
#[test]
fn nothing_reaches_the_instance_buffer_except_the_one_wrapper_that_counts_it() {
    let scanned = painter_source();

    let (start, end) =
        cs_scan::member_body(&scanned, "private void UploadRows(int first, int count)");
    let wrapper = &scanned[start..end];
    assert!(
        wrapper.contains("_instanceBuffer.SetData("),
        "UploadRows is the member that reaches the buffer: {wrapper}"
    );

    // The whole file, minus that one member. `matches` over the scanned source
    // cannot see a call in a comment or a string — `blank_comments_and_strings`
    // has already removed both.
    let elsewhere = scanned[..start].matches("_instanceBuffer.SetData(").count()
        + scanned[end..].matches("_instanceBuffer.SetData(").count();
    assert_eq!(
        elsewhere, 0,
        "the instance buffer is written only by UploadRows, and {elsewhere} other call(s) reach it"
    );
}

/// The ranged path's decision is delegated, not restated here.
///
/// The predicate is `InstanceUpload.CanSendRanges`, outside `Runtime/Engine/`,
/// where `unity/ffi-check` drives every condition false in turn. What is left
/// for a scan is that the painter asks it, and asks it with the right readings
/// in the right order — which a scan CAN check, because a call with an argument
/// in the wrong place is a different call.
#[test]
fn the_ranged_decision_is_delegated_with_the_painters_own_readings() {
    let scanned = painter_source();
    let (start, end) = cs_scan::member_body(&scanned, "private void UploadInstances()");
    let body = &scanned[start..end];

    // **The whole assignment, not the call alone.** Matching only
    // `InstanceUpload.CanSendRanges(` says nothing about what is done with the
    // answer: `var ranged = !InstanceUpload.CanSendRanges(…)` holds that
    // substring, with the same arguments in the same order, and inverts the
    // decision — a frame that must be sent whole would be sent as ranges
    // instead, and every gate would stay green.
    let call = "var ranged = InstanceUpload.CanSendRanges(";
    assert_eq!(
        body.matches(call).count(),
        1,
        "UploadInstances binds CanSendRanges' answer directly and unnegated, exactly once"
    );
    assert_eq!(
        body.matches("InstanceUpload.CanSendRanges(").count(),
        1,
        "and asks it nowhere else"
    );

    let at = body.find(call).expect("no CanSendRanges call") + call.len();
    assert_eq!(
        arguments(&body[at..]),
        vec![
            "_packer.LastPackWasPartial",
            "_haveUploaded",
            "_uploadedGeneration",
            "_packer.PackedGeneration",
            "ReferenceEquals(held, _instanceBuffer)",
            "_haveStagedHead && _stagedToWorld == DocumentToWorld",
        ],
        "the painter passes its own readings, in CanSendRanges' parameter order"
    );

    // The buffer is captured BEFORE the call that can replace it. Reading it
    // afterwards compares a reference against itself and the check is vacuous.
    let held = body
        .find("var held = _instanceBuffer;")
        .expect("no `held` capture");
    let ensure = body
        .find("EnsureCapacity(InstanceCount);")
        .expect("no EnsureCapacity call");
    assert!(
        held < ensure,
        "the buffer is captured before EnsureCapacity can replace it, and here it is captured \
         after"
    );
}

/// The two statements of the buffer's layout agree.
///
/// **The layout is stated twice because only one of the two can be executed.**
/// `BrgPainter` holds it in bytes, because that is what `AddBatch` and
/// `GraphicsBuffer` take, and nothing compiles that file. `StreamLayout` holds
/// it in words, outside `Runtime/Engine/`, so `unity/ffi-check` runs it against
/// real numbers. A change to either that is not made to the other puts every
/// ranged upload at the wrong offset, which no golden covers — every golden is
/// drawn from a full pack.
#[test]
fn the_painters_byte_layout_and_the_word_layout_are_the_same_layout() {
    let painter = painter_source();
    let layout = package_source("Runtime/StreamLayout.cs");

    let head_bytes = int_const(&painter, "HeadBytes");
    let bytes_per_instance = int_const(&painter, "BytesPerInstance");
    let head_words = int_const(&layout, "HeadWords");
    let streams = int_const(&layout, "Streams");
    let words_per_row = int_const(&layout, "WordsPerRow");

    assert_eq!(
        head_words * 4,
        head_bytes,
        "StreamLayout.HeadWords ({head_words}) and BrgPainter.HeadBytes ({head_bytes}) are the \
         same head"
    );
    assert_eq!(
        streams * words_per_row * 4,
        bytes_per_instance,
        "StreamLayout's {streams} streams of {words_per_row} words and \
         BrgPainter.BytesPerInstance ({bytes_per_instance}) are the same instance"
    );
}

/// The partial pack judges the run table before it writes a row.
///
/// **The member is executed and the CALL is scanned, and it takes both.**
/// `unity/ffi-check` drives `FramePacker.RunsAreWalkable` over tables no
/// producer can commit — every fixture in this repository carries a well-formed
/// run table — so the rule itself is pinned. What that cannot see is the packer
/// still asking it: deleting the call reddens nothing, because the member goes
/// on answering correctly to the gate that drives it directly. This is that
/// half.
#[test]
fn the_partial_pack_judges_the_run_table_before_it_writes_a_row() {
    let packer = package_source("Runtime/FramePacker.cs");
    let (start, end) = cs_scan::member_body(&packer, "private unsafe bool TryPackDirty(");
    let body = &packer[start..end];

    // **The guard's shape, not just the call's position.** A call whose answer
    // is discarded — `RunsAreWalkable(…);` with no `if` — sits in the same
    // place and passes a position check, while a corrupt run table goes on
    // being walked in silence.
    let guard = "if (!RunsAreWalkable(";
    let judged = body.find(guard).unwrap_or_else(|| {
        panic!(
            "TryPackDirty does not REFUSE on the run table, so a corrupt one is walked in \
             silence on the partial path"
        )
    });
    let (block_start, block_end) = cs_scan::member_body(body, guard);
    assert!(
        body[block_start..block_end].contains("return false;"),
        "the run-table guard abandons to the full pack, and this one does not: {}",
        &body[block_start..block_end]
    );

    let writes = body
        .find("PackRect(")
        .expect("TryPackDirty writes no rect's rows");
    assert!(
        judged < writes,
        "the run table is judged before the first row is written, and here it is judged after"
    );
}

/// The arguments of a call, split at TOP-LEVEL commas only.
///
/// **A plain `split(',')` cuts `ReferenceEquals(held, _instanceBuffer)` in
/// half**, which reads as two arguments and makes the comparison above assert
/// over a list that has nothing to do with the call's shape.
///
/// # Panics
///
/// If the argument list never closes.
fn arguments(after_open: &str) -> Vec<&str> {
    let mut depth = 0usize;
    let mut split = Vec::new();
    let mut from = 0usize;
    for (offset, byte) in after_open.bytes().enumerate() {
        match byte {
            b'(' | b'[' => depth += 1,
            b')' if depth == 0 => {
                split.push(after_open[from..offset].trim());
                return split;
            }
            b')' | b']' => depth -= 1,
            b',' if depth == 0 => {
                split.push(after_open[from..offset].trim());
                from = offset + 1;
            }
            _ => {}
        }
    }
    panic!("the argument list never closes");
}

/// One package source, comments and string bodies blanked.
fn package_source(relative_path: &str) -> String {
    let files = package_gate::package_cs_files();
    let source = &files
        .iter()
        .find(|(path, _)| path.ends_with(relative_path))
        .unwrap_or_else(|| panic!("the package no longer ships {relative_path}"))
        .1;
    cs_scan::blank_comments_and_strings(source)
}

/// The value of `const int <name> = <expr>;`, where `<expr>` is a sum of
/// products of integer literals.
///
/// **Evaluated rather than matched.** Asserting the declaration verbatim would
/// pass over a change to the value with the same spelling elsewhere, and would
/// fail on a reformatting that changes nothing — so the two things this
/// compares are numbers.
///
/// # Panics
///
/// If no such declaration exists, or its right-hand side is not a sum of
/// products of integer literals.
fn int_const(scanned: &str, name: &str) -> i64 {
    let needle = format!("const int {name} = ");
    let at = scanned
        .find(&needle)
        .unwrap_or_else(|| panic!("no `const int {name} = …;` declaration"))
        + needle.len();
    let end = scanned[at..]
        .find(';')
        .unwrap_or_else(|| panic!("`const int {name}`'s declaration never ends"))
        + at;

    scanned[at..end]
        .split('+')
        .map(|term| {
            term.split('*')
                .map(|factor| {
                    factor.trim().parse::<i64>().unwrap_or_else(|_| {
                        panic!(
                            "`{name}` is declared as `{}`, which this test evaluates only as a \
                             sum of products of integer literals",
                            &scanned[at..end]
                        )
                    })
                })
                .product::<i64>()
        })
        .sum()
}
