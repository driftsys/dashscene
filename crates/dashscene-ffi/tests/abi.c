/*
 * The ABI exercised as a C caller, not as a Rust one.
 *
 * The Rust tests in src/lib.rs call the same functions, but they call them as
 * Rust: they see the real enum, the real types, and a header that was never
 * involved. This program is the only thing in the workspace that checks the
 * two halves of the contract agree —
 *
 *   - that include/dashscene.h declares what the library actually exports, so a
 *     missing or renamed symbol is a link error here rather than a runtime
 *     surprise in a host;
 *   - that DS_ABI_VERSION in the header equals what ds_abi_version() returns,
 *     which is the one check that catches a header shipped out of step with the
 *     library it describes.
 *
 * Run by `just c-abi`.
 */

#include "dashscene.h"

#include <stdio.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>

static int failures = 0;

/* Every array of a frame, named on the C side.
 *
 * Written out rather than walked, because the point is that a C caller sees the
 * same nineteen members this build declares: if the header gained or lost one,
 * this stops compiling. Whether the two declarations AGREE IN ORDER is checked
 * on the Rust side, by `the_header_declares_the_frame_exactly_as_this_build_
 * lays_it_out` — a C compiler cannot see a permutation of same-typed members,
 * and neither can sizeof. */
static int frame_is_empty(const DsFrame *f) {
  const DsSlice all[] = {
      f->rects,         f->groups,        f->dirty,
      f->paint_entries, f->extra_fills,   f->strokes,
      f->shapes,        f->solids,        f->gradients,
      f->gradient_stops, f->image_fills,  f->shadows,
      f->blurs,         f->clip_regions,  f->clip_boxes,
      f->image_entries, f->image_payload, f->glyph_runs,
      f->glyph_quads,
  };
  if (f->generation != 0 || f->document_replaced) {
    return 0;
  }
  for (size_t i = 0; i < sizeof all / sizeof all[0]; i++) {
    /* Empty means no rows and no pointer — but the stride still describes a
     * row, which is what lets a host validate every array at the top of the
     * frame instead of only the populated ones. */
    if (all[i].ptr != NULL || all[i].count != 0 || all[i].stride == 0) {
      return 0;
    }
  }
  return 1;
}

static void check(int condition, const char *what) {
  if (condition) {
    printf("  ok    %s\n", what);
  } else {
    printf("  FAIL  %s\n", what);
    failures += 1;
  }
}

int main(void) {
  printf("dashscene C ABI\n");

  /* The check that only a C caller can make: the header and the library are
   * two halves of one contract, and nothing else compares them. */
  check(ds_abi_version() == DS_ABI_VERSION,
        "the header's DS_ABI_VERSION matches ds_abi_version()");

  DsRuntime runtime = 0;
  check(ds_runtime_new(&runtime) == DS_OK, "ds_runtime_new succeeds");
  check(runtime != 0, "ds_runtime_new mints a handle, and 0 is never one");

  check(ds_runtime_new(NULL) == DS_NULL_ARGUMENT,
        "a null out pointer is a status, not a crash");

  bool advanced = true;
  check(ds_runtime_tick(runtime, 0.016f, &advanced) == DS_NO_DOCUMENT,
        "ticking without a document reports DS_NO_DOCUMENT");

  check(ds_runtime_resize(runtime, 640, 480) == DS_NO_SURFACE,
        "resizing without a surface reports DS_NO_SURFACE");

  /* The draw call exists and is reachable from C — the symbol layer 0 needs to
   * put a pixel on screen. Without a document it must say so rather than draw
   * nothing and report success. */
  bool drawn = true;
  check(ds_runtime_draw(runtime, &drawn) == DS_NO_DOCUMENT,
        "drawing without a document reports DS_NO_DOCUMENT");

  /* The data plane, from C. Without a document there is nothing to lease, and
   * it must say so rather than hand out a frame of zero rows that a host would
   * read as an empty scene. */
  DsFrame frame;
  memset(&frame, 0xAB, sizeof frame);
  check(ds_runtime_acquire_frame(runtime, &frame) == DS_NO_DOCUMENT,
        "acquiring without a document reports DS_NO_DOCUMENT");
  check(frame_is_empty(&frame),
        "a refused acquire overwrote EVERY array of the caller's frame, not just "
        "the first");

  check(ds_runtime_acquire_frame(runtime, NULL) == DS_NULL_ARGUMENT,
        "a null frame pointer is a status, not a crash");

  /* And no lease was taken by either refusal, which is the half a status code
   * alone does not tell you. */
  bool was_leased = true;
  check(ds_runtime_release_frame(runtime, 0, &was_leased) == DS_OK,
        "releasing without a lease succeeds");
  check(!was_leased, "and reports that there was none");

  /* Nothing follows glyph_quads. An exact bound rather than `>=`, which would
   * pass over a twentieth member added to the header alone.
   *
   * This is header-against-header and cannot be otherwise: a C caller has no
   * view of the Rust layout. What ties the two is the Rust-side test
   * `the_header_declares_the_frame_exactly_as_this_build_lays_it_out`, which
   * checks this file's declarations and `offset_of!` against one list. Said
   * here because the check it replaced — `sizeof frame.rects.stride ==
   * sizeof(size_t)` — looked like a cross-check and was not. */
  check(sizeof frame == offsetof(DsFrame, glyph_quads) + sizeof(DsSlice),
        "glyph_quads is the last member of DsFrame, with nothing after it");

  /* Junk must fail as a status. An unwind across this boundary would be
   * undefined behaviour, so "it returned at all" is part of the assertion. */
  const uint8_t junk[32] = {0};
  check(ds_runtime_load_document(runtime, junk, sizeof junk) == DS_OPEN,
        "junk bytes are DS_OPEN and not a panic");

  /* The message is reachable, sized before it is fetched, and terminated. */
  size_t needed = ds_last_error_message(NULL, 0);
  check(needed > 1, "the error message reports a size to allocate");
  char small[8];
  size_t again = ds_last_error_message(small, sizeof small);
  check(again == needed, "the size does not change when a buffer is passed");
  check(small[sizeof small - 1] == '\0' || strlen(small) < sizeof small,
        "a short buffer is still NUL-terminated");

  /* This build is not Android, so the Android arm must decline rather than be
   * absent — one library serves every host and says which handles it takes. */
  uint8_t not_a_window = 0;
  check(ds_runtime_attach_surface(runtime, DS_SURFACE_ANDROID_NDK,
                                  &not_a_window, NULL, 64, 64) ==
            DS_UNSUPPORTED_HANDLE,
        "an Android handle on a host build is declined, not accepted");

  /* An unknown kind must be rejected, not merely unmatched. Passing an
   * out-of-range value to a Rust enum parameter would be undefined behaviour
   * at the call boundary; this asserts the library takes an integer and
   * validates it. 9999 is not any declared DsSurfaceKind. */
  check(ds_runtime_attach_surface(runtime, (DsSurfaceKind)9999, &not_a_window,
                                  NULL, 64, 64) == DS_UNSUPPORTED_HANDLE,
        "an out-of-range surface kind is rejected, not undefined");

  /* Detaching is what surfaceDestroyed's handshake calls, and a host tearing
   * down on a path it cannot fully predict has to be able to call it
   * unconditionally. So it succeeds with no surface attached and reports that
   * there was none, rather than answering with a status the caller would have
   * to treat as benign. */
  bool had_surface = true;
  check(ds_runtime_detach_surface(runtime, &had_surface) == DS_OK,
        "detaching with no surface attached succeeds");
  check(!had_surface, "detaching with no surface reports there was none");
  check(ds_runtime_detach_surface(runtime, NULL) == DS_OK,
        "detaching twice, and with a NULL out pointer, is allowed");

  /* The text entry point and its struct exist as this header declares them.
   * Junk bytes, so this exercises the symbol and the argument checks rather
   * than a document: a real .dsb is not reachable from this program, and the
   * faces are validated first in any case. */
  DsFontFace face;
  memset(&face, 0, sizeof face);
  face.family = "Inter";
  face.weight = 400;
  uint8_t not_a_font[64] = {0};
  face.font_bytes = not_a_font;
  face.font_len = sizeof not_a_font;
  uint8_t not_a_document[32] = {0};
  check(ds_runtime_load_document_with_text(runtime, not_a_document,
                                           sizeof not_a_document, &face,
                                           1) == DS_FONT_FACE,
        "a face that does not parse is DS_FONT_FACE from C");
  check(ds_runtime_load_document_with_text(runtime, not_a_document,
                                           sizeof not_a_document, NULL,
                                           3) == DS_NULL_ARGUMENT,
        "a null face array with a count is a status, not a crash");

  /* The weight range, enforced once inside the library rather than at this
     ABI since issue #1206. What a C caller sees is unchanged, which is what
     this asserts. 0 is what an
   * uninitialised struct carries. */
  DsFontFace zero_weight = face;
  zero_weight.weight = 0;
  check(ds_runtime_load_document_with_text(runtime, not_a_document,
                                           sizeof not_a_document, &zero_weight,
                                           1) == DS_FONT_FACE,
        "a weight outside 1..=1000 is DS_FONT_FACE from C");
  /* The junk font beside it is DS_FONT_FACE as well, so the status alone does
   * not say the weight was what failed. The message does, and without it this
   * check passes with the range check deleted. */
  char weight_message[256];
  ds_last_error_message(weight_message, sizeof weight_message);
  check(strstr(weight_message, "weight") != NULL,
        "and the weight is what it names, not the font bytes beside it");

  /* The atlas half of the struct, which nothing above reads. A face that
   * names atlas_metrics and no atlas_png is refused before the font is
   * parsed, so this reaches DS_ATLAS with the same junk font as above. */
  uint8_t not_an_atlas[48] = {0};
  DsFontFace half_atlas = face;
  half_atlas.atlas_metrics = not_an_atlas;
  half_atlas.atlas_metrics_len = sizeof not_an_atlas;
  check(ds_runtime_load_document_with_text(runtime, not_a_document,
                                           sizeof not_a_document, &half_atlas,
                                           1) == DS_ATLAS,
        "a face naming atlas_metrics and no atlas_png is DS_ATLAS from C");

  /* And the four trailing fields read for their values rather than for
   * NULL. A mixed set — one face with a whole sheet, one with none — is
   * refused before either sheet is decoded, and getting there copies
   * atlas_png_len and atlas_metrics_len bytes from atlas_png and
   * atlas_metrics. A layout disagreement between this header and the
   * #[repr(C)] struct in those fields is what that copy would trip over. */
  DsFontFace mixed[2];
  mixed[0] = face;
  mixed[0].atlas_png = not_an_atlas;
  mixed[0].atlas_png_len = sizeof not_an_atlas;
  mixed[0].atlas_metrics = not_an_atlas;
  mixed[0].atlas_metrics_len = sizeof not_an_atlas;
  mixed[1] = face;
  check(ds_runtime_load_document_with_text(runtime, not_a_document,
                                           sizeof not_a_document, mixed,
                                           2) == DS_ATLAS,
        "one face with a sheet and one without is DS_ATLAS from C");

  /* The mapped load (issue #925). No .dsb is written here — this file is
   * compiled and run to check that the header and the library agree, not to
   * re-test the loader, which the Rust tests cover over a two-root fixture.
   * What C is uniquely able to check is that the declaration binds: a
   * const char * path and a uint32_t ordinal, in that order, reaching the same
   * argument slots the library reads. A wrong declaration would show up here
   * as a wrong status rather than as a link error. */
  check(ds_runtime_load_document_mapped(runtime, NULL, 0, NULL, 0) ==
            DS_NULL_ARGUMENT,
        "a NULL path is DS_NULL_ARGUMENT from C");
  check(ds_runtime_load_document_mapped(0, "/nonexistent/no-such.dsb", 0, NULL,
                                        0) == DS_NULL_ARGUMENT,
        "a 0 handle is DS_NULL_ARGUMENT from C, where NULL was");
  check(ds_runtime_load_document_mapped(runtime, "/nonexistent/no-such.dsb", 0,
                                        NULL, 0) == DS_MAP,
        "a path nothing is at is DS_MAP from C");
  /* The ordinal is read as an ordinal and not as something else: the path
   * fails first whatever it is, so this only confirms the call survives a
   * large value in that slot rather than treating it as a pointer. */
  check(ds_runtime_load_document_mapped(runtime, "/nonexistent/no-such.dsb",
                                        4294967295u, NULL, 0) == DS_MAP,
        "a large ordinal still reports the path failure from C");
  /* faces NULL with a non-zero count is refused before the path is touched,
   * the same ordering ds_runtime_load_document_with_text keeps. */
  check(ds_runtime_load_document_mapped(runtime, "/nonexistent/no-such.dsb", 0,
                                        NULL, 1) == DS_NULL_ARGUMENT,
        "NULL faces with a non-zero count is DS_NULL_ARGUMENT from C");

  /* The ranged mapped load (story #1124), which is the one entry point where a
   * C caller can check something the Rust tests structurally cannot: that the
   * two uint64_t slots bind. A length declared here at a narrower width than
   * the library takes leaves the upper half of the register unspecified, and
   * the pairs below are one byte apart across the file's end — so a length that
   * did not arrive intact turns a DS_OPEN into a DS_MAP.
   *
   * That needs a real file, which the tests above deliberately avoid. It is
   * 200 bytes of junk rather than a .dsb: this file checks that the declaration
   * binds, and the loader itself is covered on the Rust side over a two-root
   * fixture.
   *
   * **Written under the temporary directory, not under target/.** A relative
   * path would resolve against the caller's working directory, so running this
   * binary from anywhere but the repository root — which is what a developer
   * debugging it does — would fail the open and SKIP every range check below
   * rather than run it. The write is asserted even so. */
  {
    const char *tmp = getenv("TMPDIR");
    char range_path[512];
    unsigned char junk[200];
    size_t i;
    FILE *f;

    if (tmp == NULL || tmp[0] == '\0') {
      tmp = "/tmp";
    }
    /* snprintf truncates rather than overflowing, and a truncated path simply
     * fails to open below, which is reported. */
    snprintf(range_path, sizeof range_path, "%s/c-abi-range-fixture.bin", tmp);

    for (i = 0; i < sizeof junk; i++) {
      junk[i] = (unsigned char)(i % 251u + 1u);
    }
    f = fopen(range_path, "wb");
    check(f != NULL, "the range fixture opens for writing");
    if (f != NULL) {
      check(fwrite(junk, 1, sizeof junk, f) == sizeof junk,
            "the range fixture is written whole");
      check(fclose(f) == 0, "the range fixture closes");

      /* The whole file as a range: inside the file, so it reaches the parser
       * and fails as bytes rather than as a range. */
      check(ds_runtime_load_document_mapped_range(runtime, range_path, 0,
                                                  sizeof junk, 0, NULL,
                                                  0) == DS_OPEN,
            "a range that is inside the file reaches the parser from C");
      /* One byte more, and it is a range failure instead. The pair is what
       * says the length slot arrived intact. */
      check(ds_runtime_load_document_mapped_range(runtime, range_path, 0,
                                                  sizeof junk + 1, 0, NULL,
                                                  0) == DS_MAP,
            "one byte past the end is DS_MAP from C");
      /* And the same pair moved off zero, so the offset slot is not simply
       * being ignored. */
      check(ds_runtime_load_document_mapped_range(runtime, range_path, 100, 100,
                                                  0, NULL, 0) == DS_OPEN,
            "a range at a non-zero offset reaches the parser from C");
      check(ds_runtime_load_document_mapped_range(runtime, range_path, 100, 101,
                                                  0, NULL, 0) == DS_MAP,
            "one byte past the end from a non-zero offset is DS_MAP from C");
      check(ds_runtime_load_document_mapped_range(runtime, range_path, 0, 0, 0,
                                                  NULL, 0) == DS_MAP,
            "a length of 0 is DS_MAP from C");
      /* faces NULL with a non-zero count is refused before the range is
       * touched, the same ordering the other loaders keep. */
      check(ds_runtime_load_document_mapped_range(runtime, range_path, 0,
                                                  sizeof junk, 0, NULL,
                                                  1) == DS_NULL_ARGUMENT,
            "NULL faces with a non-zero count is DS_NULL_ARGUMENT from C");
      check(remove(range_path) == 0, "the range fixture is removed");
    }

    check(ds_runtime_load_document_mapped_range(runtime, NULL, 0, 1, 0, NULL,
                                                0) == DS_NULL_ARGUMENT,
          "a NULL path is DS_NULL_ARGUMENT from C on the ranged loader");
    check(ds_runtime_load_document_mapped_range(0, "/nonexistent/no-such.dsb",
                                                0, 1, 0, NULL,
                                                0) == DS_NULL_ARGUMENT,
          "a 0 handle is DS_NULL_ARGUMENT from C on the ranged loader");
  }

  /* Discriminants pinned by value.
   *
   * Reaching a status from C is the stronger check and is what the tests above
   * do wherever it is cheap. What is left needs a real two-root document with
   * a corrupted payload (95 lines of flatbuffer assembly in the Rust tests), a
   * surface whose swapchain is lost, or a panic crossing the boundary — none
   * of which belong here. Leaving those unchecked would leave the one gate
   * that compares the two halves blind to exactly the mistake it exists to
   * catch: a discriminant typed wrong in this hand-written header.
   *
   * **No count.** Three successive versions of this comment claimed a
   * correspondence with the Rust test the_abi_version_did_not_move — "four and
   * four", then "five and five", then a derivation that double-counted DS_MAP,
   * which this file both reaches by call and pins below. The two cover
   * overlapping sets on purpose and there is no number that describes them
   * both, so stating one has only ever produced a claim to falsify. Add a pin
   * when a variant is added; do not count them. */
  check(DS_PANIC == 8, "DS_PANIC is 8 in the header");
  check(DS_MAP == 11, "DS_MAP is 11 in the header");
  check(DS_NO_SUCH_ROOT == 12, "DS_NO_SUCH_ROOT is 12 in the header");
  check(DS_DERIVED == 13, "DS_DERIVED is 13 in the header");
  check(DS_PAYLOAD == 14, "DS_PAYLOAD is 14 in the header");
  /* The tail variant issue #884 added. The one most likely to be typed wrong,
   * being the newest and the only one a host is told to branch on differently
   * from its neighbour. */
  check(DS_SURFACE_LOST == 15, "DS_SURFACE_LOST is 15 in the header");
  /* The three story #1226 appended. A Rust test over DsStatus cannot read the
   * header, so these pins are the only thing that can see a header-only typo
   * in them. DS_BAD_HANDLE is also exercised below; these two are reachable
   * only from another thread or from an exhausted table, so they are pinned by
   * value rather than provoked. */
  check(DS_BAD_HANDLE == 16, "DS_BAD_HANDLE is 16 in the header");
  check(DS_WRONG_THREAD == 17, "DS_WRONG_THREAD is 17 in the header");
  check(DS_HANDLES_EXHAUSTED == 18, "DS_HANDLES_EXHAUSTED is 18 in the header");
  check(DS_FRAME_LEASED == 19, "DS_FRAME_LEASED is 19 in the header");

  check(ds_runtime_free(runtime) == DS_OK, "a live handle frees");
  check(ds_runtime_free(runtime) == DS_BAD_HANDLE,
        "and freeing it twice is reported rather than corrupting the table");
  check(ds_runtime_free(0) == DS_OK, "0 frees nothing and succeeds, like free(NULL)");
  check(ds_runtime_tick(runtime, 0.016f, NULL) == DS_BAD_HANDLE,
        "a freed handle drives nothing");
  check(ds_runtime_tick(0, 0.016f, NULL) == DS_NULL_ARGUMENT,
        "and 0 is the null argument it replaces");

  if (failures == 0) {
    printf("dashscene C ABI: all checks passed\n");
    return 0;
  }
  printf("dashscene C ABI: %d check(s) FAILED\n", failures);
  return 1;
}
