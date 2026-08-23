/*
 * A dashscene library that predates most of its own surface.
 *
 * Several builds come out of this one file, and each one exists to reach a
 * failure `unity/ffi-check` could otherwise only describe. **This list is the
 * count**: the recipe builds one library per entry below, and nothing else in
 * the tree states how many there are.
 *
 *   default          ds_abi_version + ds_runtime_new. A package newer than its
 *                    library: the handshake agrees, the runtime constructs, and
 *                    every other entry point fails where .NET binds the import.
 *   -DDS_STUB_SKEW=N ds_abi_version alone, reporting a version N ahead. The
 *                    same failure for ds_runtime_new — which the default build
 *                    exports and so cannot test — and the only fixture that can
 *                    tell a version READ from the library from one assumed
 *                    equal to the package's.
 *   -DDS_STUB_SILENT nothing at all. The degenerate case: with no
 *                    ds_abi_version there is no version to report, so the
 *                    binding failure is handed back unchanged rather than
 *                    translated, and it must still name the symbol that failed.
 *   -DDS_STUB_REFUSES_FREE
 *                    the handshake, the constructor, and a ds_runtime_free that
 *                    REFUSES — but no ds_last_error_message. The only fixture
 *                    where a call answers a status and the channel that
 *                    describes it cannot bind, which is the one route by which
 *                    a translated exception could still escape Dispose.
 *   -DDS_STUB_LEASE_REFUSES
 *                    the handshake, the constructor, a ds_runtime_release_frame
 *                    that PANICS and a ds_runtime_free that SUCCEEDS. The
 *                    fixture for a runtime that was freed while
 *                    LastDisposeStatus reports a failure — the state the
 *                    property documents and no other build can produce.
 *
 * **Why a library rather than a mock.** .NET binds a `[DllImport]` lazily, at
 * the first call, and consults an assembly's `DllImportResolver` once per
 * library name — so the failure only exists when a real loader really fails to
 * find a real symbol. `SetDllImportResolver` also throws on a second call for
 * one assembly, which is why the gate presents each of these to its own copy of
 * the package assembly in its own `AssemblyLoadContext`. A second process would
 * do as well and costs more.
 *
 * **The version comes from the header**, so it cannot drift: a stub reporting a
 * hard-coded number would refuse the handshake the day `DS_ABI_VERSION` moves,
 * and the gate would fail somewhere that says nothing about what it checks.
 *
 * ds_runtime_new hands back a handle no library minted. Nothing dereferences
 * it: every call the gate makes on it fails to bind before the library sees it.
 */

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "dashscene.h"

#if defined(DS_STUB_LEASE_REFUSES)
#define DS_STUB_KEEPS_HANDSHAKE 1
#endif

#ifndef DS_STUB_SILENT

uint32_t ds_abi_version(void) {
#ifdef DS_STUB_SKEW
  return DS_ABI_VERSION + DS_STUB_SKEW;
#else
  return DS_ABI_VERSION;
#endif
}

#if !defined(DS_STUB_SKEW)
DsStatus ds_runtime_new(DsRuntime *out) {
  if (out == NULL) {
    return DS_NULL_ARGUMENT;
  }
  *out = 1;
  return DS_OK;
}
#endif

#ifdef DS_STUB_LEASE_REFUSES
/*
 * A release that answers a failure, and a free that then succeeds. The pair is
 * what makes LastDisposeStatus report a failure on a runtime that WAS freed —
 * the shape the property's own documentation describes and that no library
 * built from this tree can produce, because a real release clears the lease
 * before anything that can fail.
 */
DsStatus ds_runtime_release_frame(DsRuntime runtime, int32_t drawn,
                                  bool *out_was_leased) {
  (void)runtime;
  (void)drawn;
  if (out_was_leased != NULL) {
    *out_was_leased = false;
  }
  return DS_PANIC;
}

DsStatus ds_runtime_free(DsRuntime runtime) {
  (void)runtime;
  return DS_OK;
}
#endif

#ifdef DS_STUB_REFUSES_FREE
/*
 * A free that answers rather than one that cannot bind. The handle this library
 * minted is not one it knows, which is exactly what DS_BAD_HANDLE means, and it
 * sends the host down the branch that asks ds_last_error_message what happened
 * — the symbol this build does not export.
 */
DsStatus ds_runtime_free(DsRuntime runtime) {
  (void)runtime;
  return DS_BAD_HANDLE;
}
#endif

#endif /* DS_STUB_SILENT */

/*
 * A translation unit exporting nothing is legal C but an empty shared library
 * is easy to mistake for a failed build, so every build carries this and the
 * gate never looks it up. It is also what makes the silent build's `nm` output
 * distinguishable from a compile that produced no code at all.
 */
const char ds_stub_marker[] = "dashscene ffi-check stub";
