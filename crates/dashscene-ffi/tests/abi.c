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
#include <string.h>

static int failures = 0;

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

  DsRuntime *runtime = NULL;
  check(ds_runtime_new(&runtime) == DS_OK, "ds_runtime_new succeeds");
  check(runtime != NULL, "ds_runtime_new writes a handle");

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

  /* The weight range, which only this ABI enforces. 0 is what an
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
  check(ds_runtime_load_document_mapped(NULL, "/nonexistent/no-such.dsb", 0,
                                        NULL, 0) == DS_NULL_ARGUMENT,
        "a NULL runtime is DS_NULL_ARGUMENT from C");
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

  ds_runtime_free(runtime);
  ds_runtime_free(NULL); /* free(NULL) semantics */
  check(1, "freeing the runtime and NULL both return");

  if (failures == 0) {
    printf("dashscene C ABI: all checks passed\n");
    return 0;
  }
  printf("dashscene C ABI: %d check(s) FAILED\n", failures);
  return 1;
}
