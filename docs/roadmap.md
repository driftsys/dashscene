# Roadmap

    status  living — revised at each phase-end epic close (AGENTS.md,
            "Plan tracking")
    source  docs/archive/2026-07-14-design-1-seed.md §11
            docs/archive/2026-07-14-scope-decisions.md §18, §19, §22, §23

This file carries the plan's **shape**. GitHub carries the plan's **state**.
They are different things, so nothing here is duplicated from GitHub and there
is nothing to keep in sync between the two.

| This file (shape)               | GitHub (state)                                |
| ------------------------------- | --------------------------------------------- |
| Which slices exist (v0.1-v0.22) | Which stories exist under each epic           |
| What each slice delivers        | Which stories are open, closed, who owns them |
| Inter-slice dependency edges    | Story-level dependency edges                  |
| Which E-criteria a slice closes | Debt triage and milestone assignment          |
| Each slice's epic issue numbers | Everything that churns weekly                 |
| The v1 and v2 outlines          |                                               |

The dividing line is churn. A slice-level dependency — "v0.6 needs v0.5's atlas"
— changes at a phase-end plan revision, a handful of times across the whole of
v0. A story-level dependency — "issue X blocks issue Y" — changes weekly and
stays in the issue body, where it already lives. This file therefore names each
slice's epic issues — usually one, more where its parts gate separately or where
one of them is declared not to gate the slice at all, as v0.21's four do — and
links no further: the stories under them, and their state, are GitHub's job.

## Why this file exists at all

An earlier position held that GitHub alone was enough, and no in-repo plan
record was kept. That position is reversed here, for three reasons:

- **It might not have survived the move to the public name.**
  [`decisions/repo-staging-and-public-facade.md`](decisions/repo-staging-and-public-facade.md)
  left the mechanism open between a fresh push and a history merge, and a fresh
  push takes no GitHub issues with it — which would have lost the plan, the one
  engineering artifact held nowhere else. It was settled on 2026-08-11 by
  renaming the repository instead, so the issues did survive; the reason for
  writing the plan down in the tree stands regardless, because it was not
  knowable at the time.
- **It is not reviewable.** A change to the plan cannot be proposed, discussed,
  and approved in a pull request alongside the code it plans.
- **It is not readable offline**, and it is not versioned with the code.

## Staying current — the phase-end revision ritual

When a slice's epic closes, the remaining epics and stories are revised against
what that slice taught, before the next slice starts: update, split, merge, or
re-order the issues, and record the scope-level outcome in a retrospective
(`AGENTS.md`, "Plan tracking"). **On a slice with more than one epic the trigger
is the last of its _gating_ epics to close**, not the first — a slice is not
finished while one of its halves is still open, and firing the ritual on the
first close would revise the plan against half a slice. An epic explicitly
declared not to gate the slice, as v0.21's #1120 is, is not part of that
trigger: what it still holds moves out at the close rather than delaying it.
**The ritual edits the slice entry in place.** It rewrites the entry to state
the scope as it now stands, and stamps it with one line naming the revision that
produced it — nothing more. It does not append a block describing how to read
the text beneath it, and it does not leave a superseded paragraph standing with
a note about which parts still hold.

The reason is that an appended amendment makes the reader reconstruct the
current scope by applying a chain: this block is later than that one, except for
a count, except for an exception that stood until a date. The chain is prose, so
it can contradict itself, and every revision makes the next reader's job harder.
The edit history it encodes is already in git, exactly dated and impossible to
misread — `git log -p docs/roadmap.md`. Recording it twice keeps only the
ambiguous copy in front of the reader.

This matches the repository's standing rule for every other record: when new
work changes one, edit it in place.

The ritual has one gate that is not a document edit: run `just calibrate` before
revising anything. It re-derives the committed asset tables, and it is the only
run in the schedule not driven by a path filter — the backstop against a table
that drifted through a change the filter did not predict
(`docs/decisions/test-tiers.md`).

**The ritual also sweeps for unanchored work, at three levels.**
[`decisions/pre-v1-hardening-slice.md`](decisions/pre-v1-hardening-slice.md)
made the first level part of this ritual on 2026-07-19, after 23 issues were
found carrying no milestone and so sitting in nobody's count; the same failure
recurred at 55 and is why v0.20 exists. **The second and third levels were both
added at the v0.20 close (2026-08-18)** — the second when nine open issues on
v0.21 were found belonging to none of that milestone's three epics, which is the
same failure one level down, where a milestone query finds the issue and an
epic-driven session does not; and the third for story #859, which an epic named
in prose and which no query returned for want of a label. So:

- **Milestone** — every open issue carries one.
- **Epic** — every open issue on a slice is named by one of that slice's epics.
  **A milestone with no epic is skipped, not flagged.** Two shapes have none: a
  slice opened and not yet planned, which is v0.22 today, and v0.23, which is a
  holding milestone and will never have one.
- **Label** — every open issue carries a label some listing returns.

**Each level's scope, its command and its standing exceptions are stated once**,
in
[`decisions/slices-are-planned-against-their-inflow.md`](decisions/slices-are-planned-against-their-inflow.md)
— not here and not in `AGENTS.md`. This file names the three levels because the
shape of the ritual is its job; how each one is run is not, and the three copies
of that detail drifted apart six times while this revision was being written.

**Level 3's first run returned five, and none was relabelled**: the repository
has no label for design, investigation or tracking work, which is what all five
are. That is **#1247**, carrying `owner-input`, and until it is ruled those five
are a standing exception list.

**And it gives the rolling-debt milestone a cluster pass**, also added at the
v0.20 close: group its open population by subject and act on the groups rather
than on the items. That milestone's own rule — one focused pull request each —
is what hides the three things only the population shows: duplicates that later
work already repaired, clusters that are one property described N times, and
items sized against the gate they came from rather than against the milestone
they landed on. The v0.23 entry below records what the first pass found of each.

**And it records the slice's inflow against its plan**, the third addition made
at the v0.20 close and the one a slice is planned by rather than closed by: a
slice's epic states the issue count it plans where it states its tracks, and
this ritual writes the closing count beside it. v0.20 planned 13 and closed 142,
which nothing predicted and nothing recorded until afterwards. Neither number
gates anything and neither is a target.

[`decisions/slices-are-planned-against-their-inflow.md`](decisions/slices-are-planned-against-their-inflow.md)
carries all three additions and the measurements behind them, including what a
cluster pass deliberately does **not** do.

The ritual has fired off-cycle three times, ahead of its own slice's close: v0.4
was revised by a design session before epic #19 closed, v0.7 was revised at the
v0.3 close even though epic #36 had not yet closed at that point, and v0.20 to
v0.23 were planned on 2026-08-12 while v0.19 was still open. A slice can be
revised earlier than its own close if something learned elsewhere bears on it;
the mechanism is not strictly "close, then revise the next one" — it is "revise
whenever the ground shifts enough that carrying the old shape forward would be
misleading."

A slice marked **provisional** below has not been revised since
`docs/archive/2026-07-14-design-1-seed.md` §11's original breakdown; it stands
until the slice before it closes and gets checked against what that slice
taught.

## v0 exit criteria

Seven exit criteria, `E1`-`E7`, gate v0. Each slice below states which it
closes; full definitions and current proof status live in
[`specification/05-qualification.md`](specification/05-qualification.md) — that
file is the one place a criterion's status can drift out of date, so it is the
only place that states it. `E7` — the design-source render oracle (guardrail
G-11) — was targeted for the v0.7 importer close and slipped; its tooling is
carried by the v0.8 fidelity slice and asserted at the v0.9 gate.

## Slices

### Closed — v0.1 to v0.20

Twenty slices, closed in order. Each one's full record — what it delivered, what
it depended on, and the revision notes written at its close — is in
[`roadmap-closed.md`](roadmap-closed.md); the links below land on it.

- **[v0.1 — walking skeleton](roadmap-closed.md)** — Epic #1. Delivered: the
  `dashbuf` schema (the `.dsb` flatbuffer format), the golden harness,
  `dashscene-core`'s arena and staged-mutation API…
- **[v0.2 — flex core](roadmap-closed.md)** — Epic #7. Delivered:
  `dashscene-engine` solving every scene through Taffy as the sole solver; the
  H/V flex modes, hug/fill/fixed sizing, gap/padding/alignment,…
- **[v0.3 — basic paint + importer enters](roadmap-closed.md)** — Epic #12.
  Delivered: four gradients, rounded-rect and stroke alignment, images, clip;
  and the Figma importer enters, single frame, minimal (`importers/figma/`,…
- **[v0.4 — variants + staged mutation + minimal FLIP](roadmap-closed.md)** —
  Epic #19, closed 2026-07-16. Delivered: the variant table and the
  `set_variant` commit path, `dashcue`'s animation vocabulary and scheduler,
  minimal FLIP on a variant switch…
- **[v0.5 — text I: Latin](roadmap-closed.md)** — Epic #24, closed 2026-07-16.
  Delivered: `dashscene-typeset`'s Latin pipeline (metrics, glyph atlas), and
  the engine measure callback so text drives hug sizing.
- **[v0.6 — text II: bidi/Arabic + charsets](roadmap-closed.md)** — Epic #31,
  closed 2026-07-16. Delivered: bidi run splitting and RTL, Arabic shaping
  (GSUB/GPOS) and mixed numerals, per-locale charset coverage feeding the glyph
  atlas, and the…
- **[v0.7 — importer catch-up](roadmap-closed.md)** — Epic #36, closed
  2026-07-17. Delivered: widening the lowering beyond fixed layout to
  auto-layout and grid — the gate on the rest of this slice.
- **[v0.8 — fidelity](roadmap-closed.md)** — Epic #42, closed 2026-07-17.
  Delivered: layout fidelity (wrap, grid spans, baseline — including the Taffy
  baseline-behavior question tracked in…
- **[v0.9 — parity](roadmap-closed.md)** — Epic #47. Delivered: the
  same-screen-both-ways fixture, and the v0 exit gate — `E1` through `E7`
  asserted in CI.
- **[v0.10 — real-file fidelity](roadmap-closed.md)** — Epic #343, closed
  2026-07-19. Delivered: the named, counted gaps the real-file import left as
  skip-with-warning holes, in measured-value order — the `LIGA:0` text unlock
  (#341, one…
- **[v0.11 — document sections + asset model](roadmap-closed.md)** — Epic #344,
  closed 2026-07-26. Delivered: the `.dsb` sectioned-container envelope
  ([`decisions/dsb-sectioned-container.md`](decisions/dsb-sectioned-container.md)
  deferred it to…
- **[v0.12 — packer + quality profiles](roadmap-closed.md)** — Epic #345, closed
  2026-07-27. Delivered: `dashpack` (an in-workspace standalone tool — vendored
  astcenc, an own KTX2 writer, no external CLIs), the RAW/HiFi/LoFi quality
  profiles…
- **[v0.13 — pre-v1 hardening](roadmap-closed.md)** — Epics #362 (the burn-down)
  and #474 (the decisions track), closed 2026-07-31. Delivered: the independent
  code-debt that accumulated across v0.1–v0.12 and is resolvable before v1 —
  perf and allocation micro-debt, cleanup,…
- **[v0.14 — the showcase runtime](roadmap-closed.md)** — Epic #568, closed
  2026-08-01. Delivered: the first frame this project has ever drawn into a
  window, and the `README.md` it does not have.
- **[v0.15 — the lean painter](roadmap-closed.md)** — Epic #569, closed
  2026-08-05. Delivered: `dashscene-gpu` behind boundary B, covering native and
  web.
- **[v0.16 — loading performance](roadmap-closed.md)** — Epic #594, closed
  2026-08-07. Delivered: r5 made falsifiable, and met.
- **[v0.17 — embedding and integration](roadmap-closed.md)** — Epic #793, closed
  2026-08-08. Delivered: **the integration surface as a thing rather than an
  example.** `crates/dashscene-web` (story #741) and `crates/dashscene-desktop`
  (story…
- **[v0.18 — animation vocabulary](roadmap-closed.md)** — Epic #769, closed
  2026-08-11. Delivered: **motion as data in the document.** When the slice
  opened, a dashscene animation could not ship in a file.
- **[v0.19 — Android, the C ABI, and layer 0](roadmap-closed.md)** — Epic #833,
  closed 2026-08-16. Delivered: **the second platform.** Much of what follows is
  the plan as written on 2026-08-09, kept because the reasoning is the record;
  read those…
- **[v0.20 — hardening: the critical findings and the Android recovery path](roadmap-closed.md)**
  — Epic #951, closed 2026-08-18. Delivered: **the correctness sweep, and an
  Android path that can report its own failure.** What it did not deliver is a
  documentation file that agrees…

### v0.21 — Unity and Android on target hardware — open

**Four epics, three of which gate the slice.** #1106 (Unity) and #1107 (target
hardware) were filed 2026-08-16 as the MVP pair; #1441 was added 2026-09-05 and
gates beside them; #1120 holds what is not MVP and does not gate. **v0.21 closes
when the three gating epics have closed.** #1106 closed on 2026-09-05; #1107 and
#1441 remain. Whatever #1120 still holds at that point moves out rather than
holding the slice open. The design findings are held on tracking issue #851 and
must not be re-derived.

The MVP pair are two epics because the halves gate on different kinds of thing —
#1106 on owner-supplied decisions and #1107 on a device — so one epic would have
made the whole slice read as blocked whenever either half was. Optimization and
debt this slice motivates sit on #1120; anything that would read the same if
v0.21 had never happened goes to `v1` instead.

**#1441 measures the Unity painter beside a faithful uGUI Canvas** — lower on
CPU, at or below on GPU, with no tolerance above — on the target device, and
that criterion holds the slice open. The ruling is
[`decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md`](decisions/the-unity-painter-is-measured-against-a-faithful-canvas.md).
Its ten stories are #1442 to #1451, listed in the epic's table; #1442 and #1443
have closed. The specification and its plan stay in `docs/wip/` until #1451
archives them.

**The GPU frame is now budgeted, on the owner's ruling of 2026-09-04 to halve
it.** The Pixel 5 measurement
([`design/android-toolchain.md`](design/android-toolchain.md), "The Unity host's
presented rate") found the Unity host GPU fill-bound at native resolution. The
ruling is
[`decisions/the-gpu-frame-on-the-target-device-is-budgeted.md`](decisions/the-gpu-frame-on-the-target-device-is-budgeted.md):
one display frame at native resolution, read from the compositor. It is carried
under #1120 by #1412 (R-T2 in the Unity painter) and #1413 (per-kind cost and
fast paths), with #1293 and #1296 reassigned from `v1` as the lean painter's
measure-then-decide half and the shaded-area derivation. The order, from epic
#1120's comment of 2026-09-04 and the record's D3, is #1408 and #1402 first,
#1296 before #1412, then #1413. Whether the halving gates the slice has not been
ruled; #1120's non-gating status stands until it is.

**All three entry conditions are settled.** They were, in owner-supplied terms:
the layer question of
[`decisions/host-integration-in-three-layers.md`](decisions/host-integration-in-three-layers.md),
the BRG record moving from `proposed` to `accepted`, and, for the Android half,
a target device available. (A fourth condition, the Unity C# repository being
created by the owner, was resolved differently on 2026-08-17: the owner instead
ruled the C# package sited in this repository under `unity/` —
[`decisions/unity-package-sited-in-this-repository.md`](decisions/unity-package-sited-in-this-repository.md),
reversed in place — which turned it from an artifact only the owner could supply
into work this repository could do. Story #1239 carried it out, together with
the rename of `dashscene-unity` to `dashpaint-abi` ruled the same day: `unity/`
holds the UPM package and the .NET check that compiles its declarations against
the Rust layouts, and the crate and its symbols carry the new name.) The
remaining two — the layer question and the BRG record, both #1106's — were ruled
by the owner on 2026-08-18: `unity-painter-uses-brg.md` is `accepted` against a
fallback ladder, and a Unity host occupies layer 0 in its host-draws form, with
layers 1 and 2 deferred to `v1` as issues #1261 and #1262. #1106 has no entry
condition left. The third — the target device, #1107's — was met on 2026-08-17,
when a Pixel 5 measured #885, #969, #842 and #1128.

**#171 stays open**, holding three records that were all `proposed` when it
moved here from `v1` on 2026-08-16; only `unity-painter-uses-brg.md` belonged to
this slice, and ratifying that one record alone lifted the BRG condition. The
other two records #171 holds are still unruled. The layer-question condition is
tracked without an issue of its own, as open question 4 on #851: "which of the
three layers a Unity host occupies."

**The Unity half: the engine painter over BatchRendererGroup, and the C# host
that sits on the `dashscene-ffi` data plane.** Proposed for v0.20 on 2026-08-09
and moved here on 2026-08-12, behind the hardening slice; the Android half
(below) was added on 2026-08-16, and the slice was called "v0.21 — Unity" until
then. What moved here from `v1`, in the terms that section used: the engine
painter with its SDF shader library and its material classes, and the C#
declarative skin. Everything else `v1` listed stays there.

**Its design is already worked out and must not be re-derived.** A long-form
capture was drafted and closed unmerged after four review rounds found 4, 12, 15
and 13 findings, concentrated in the sections making recommendations about
unwritten code — and twice a fix introduced a worse defect than the one it
repaired. What survived every round is held as a short set of checked readings
on #851 instead, where it is short enough to verify. The painter architecture
itself is
[`decisions/unity-painter-uses-brg.md`](decisions/unity-painter-uses-brg.md) and
[`technotes/rendering-and-painters.md`](technotes/rendering-and-painters.md).

**The data plane is built.** `dashscene-ffi` was shaped around a surface handle:
a host gives dashscene a surface and dashscene draws into it. A host that draws
the frame itself — which is what a Unity host is — needs the opposite direction,
and the committed tables crossed no boundary. Boundary B was made
FFI-representable at v0.15 by story #600 and had had no consumer since; it has
one now. `ds_runtime_acquire_frame` and `ds_runtime_release_frame` hand the
tables out under a lease
([`decisions/the-frame-crosses-under-a-lease.md`](decisions/the-frame-crosses-under-a-lease.md)),
which leaves #1122 and #1123 unblocked and leaves the glyph atlases, which are
not rows, to #1123. Story #859 built it on 2026-08-20, after #1226 (below) was
ruled on 2026-08-18, since that ruling decided the signature of every entry
point the data plane would add. #859 itself carried no label until this slice's
2026-08-16 revision, so it appeared in no `story` or `debt` listing and was
missing from epic #1106's own story table even though the epic calls it "the
first build step" and gates #1122 and #1123 on it; it is now a `story` and the
table's first entry. `docs/design/c-abi.md` records the same gap from the other
side: layer 0 shipped complete for a host that hands over a surface, which is
exactly the shape a host drawing its own frames cannot use — and Unity is that
host.

**#1226 is a gate rather than debt beside the slice, ruled 2026-08-18.** It
asked whether the C ABI's runtime handle stays a raw pointer. The answer changes
the signature of ten of the twelve entry points the ABI exported then —
`ds_abi_version` and `ds_last_error_message` take no runtime and keep theirs —
and therefore every P/Invoke declaration a C# host writes, so it was owed before
#859 added entry points and before #1121 writes the host. The ruling is a
generational handle in a thread-affine table —
[`decisions/the-c-abi-runtime-handle-is-generational.md`](decisions/the-c-abi-runtime-handle-is-generational.md)
carries it, its reasoning and its cost. It is not an entry condition on the
slice; #1226 itself tracks the implementation.

**#876 was a live falsehood in a design record**: `architecture.md` said `Node`
"already carries" four placeholder fields, and all four were absent from the
schema and from the workspace. Story #1126 builds them. Corrected, and #876 is
now labelled `debt`.

**A gap analysis found the Unity epic owned no story that built anything**, and
eight stories were filed: seven for the C# side — a deployment spike, the host,
the painter, the text seam, the packaging path, the placeholder surface and its
diagnostic — and one for the render-target budget that Q-6 has left as a
placeholder since the seed document.

**The Android half: work that a target device gates, plus "Unity on Android
hardware, integration and performance," which is scope on #1107 rather than
stories.** v0.19 closed without ever running on the target device class — D3a's
confirmation was required before anything was built on that assumption and was
never taken, and #842's on-device half moved here with #885 — which left #1107
carrying a risk that was supposed to be retired a slice earlier. That was
settled 2026-08-17: #885 was measured on a Pixel 5, and
[`design/android-toolchain.md`](design/android-toolchain.md) now records D3a as
measured — one device, and a property to re-check per device class. Nothing may
describe Android as working until #885's measurement is taken (it now has been),
and emulator results stay labelled as emulator results.

**#1107 has two tracks that do not gate alike.** Track A is device-only work.
#885 (D3a, the Vulkan measurement), #969 (the device run for the text path) and
#960 (the debug attach) exist as code and owe only the run: `just android-probe`
and the text-carrying harness both run today. #885 closes by running
`just android-probe` on the device and recording the adapter,
`max_storage_buffers_per_shader_stage` and the device-request verdict in
[`design/android-toolchain.md`](design/android-toolchain.md); it is windowless
and needs nothing else built. #969's harness already calls
`nativeSurfaceCreatedWithText` and its glyphs draw, but so far only on an
emulator. #960 asks whether a **debug** attach ever completes and whether it
wedges on target hardware — not the surface-destroy handshake, which was
measured at 27 ms on a tablet image and belongs to #874 (emulator-gated, and
sitting on v0.23 rather than this slice): what is unmeasured for #960 is the
acquisition, at 0.74 s in release against no observed completion in debug. #842
is the exception to "owes only the run": its frame-timing instrument lives in
`demo/src/shell.rs` and is not reachable from a third host, so its story writes
that reachability into `demo-android` before any device measures anything.

#885, #969, #842 and #1128 all closed against the Pixel 5 (`redfin`, Adreno 620,
Android 14 / API 34) measured 2026-08-17. Two further items are open on Track A
but are not device runs — #1215 and #1236, defects in the harness and in the
measurement's own table — bringing Track A to seven items: four closed (#885,
#969, #842, #1128) and three open (#960, #1215, #1236). The owner ruled on
2026-08-23 (#1291) that #960 is the device run it appears to be, so **Track A is
hardware-gated until #960's run.**

**Track B is Unity on Android hardware** — device work by definition, and part
of #1107's definition of done. It waits on #1106 rather than on a device being
absent: there is no Unity host to run on a device until that epic delivers one.
The measurements are in
[`design/android-toolchain.md`](design/android-toolchain.md) under "What the
device measured", and every number there is a number about one device.

**Provenance: this slice took nine existing issues from other milestones on
2026-08-16**, in four passes over one day (the earliest predates this slice's
rename):

- #828 from v0.19 — the conformance suite, held across slices because no painter
  had landed that was not written in Rust.
- #885 from v0.19, and #960 and #969 from v0.20 — the hardware-gated three, on
  the ruling epic #951 records.
- #171 and #134 from `v1` — an entry condition, and the clip-edge decision this
  slice's parity gate would otherwise discover.
- #872 and #708 from `v1` — costs measured so far only on an emulator or a
  development machine, and not MVP.
- #842 from v0.19 — the frame-rate number from a device.

Each of these was on a milestone whose own work could not finish it: the blocker
was an owner-supplied artifact (a ruling or a device), not more coding. Epic
#951 records the ruling. The Android work that a device does not gate stayed on
v0.20 — fourteen issues at the time of the move, against which its own
recovery-path work is written.

**Nine open issues that belonged to no epic were placed on 2026-08-18**: #1226
to #1106; #1215 and #1236 to #1107; #1029, #1191, #1232, #1235 and #1195 to
#1120; and #1149 moved to v0.23. Which issue sits under which epic is GitHub's,
per the table at the top of this file.

**Two stories no plan had named were filed and closed on 2026-08-17**: #1229,
the Android measurement apparatus, and #1230, the Unity build environment and
the seam proven end to end. Both were built while the thing they serve was still
blocked — #1229 before the device arrived, #1230 while all of #1106's entry
conditions were open, its own body saying it exists so the Unity work "starts
against a toolchain that is known to work rather than against one being
installed." #1229 was spent the same day it landed:
`design/android-toolchain.md` records the first bundle as "taken 2026-08-17 with
`just android-measure` (story #1229's apparatus)." That ordering is why v0.22's
profile and census items were filed as issues rather than left as prose: #1242,
the profile, can be built before that slice's entry condition is settled — it is
a specification document and needs no dependency — and #1243's corpus capture
can be pinned before anything imports it, though the harness itself waits on the
import path.

**Precedent for running more than one epic**: v0.13's #474, "the inputs and
rulings this slice waits on," was that slice's second track beside the
burn-down, and held exactly the items a coding session could not finish because
what they needed was an owner input — the same reason for the split here. (v0.13
ran five epics: #362, the burn-down, plus #438, #439 and #475, streams within
it, plus #474; the three streams were split on a different rule —
[`decisions/debt-streams-own-artifact-classes.md`](decisions/debt-streams-own-artifact-classes.md),
by which artifact class a branch owns. v0.14's apparent second epic is #47, the
v0.9 epic, which carries the v0.14 milestone; issue #1114 tracks that. So v0.21
is the first non-burndown slice to run more than one genuine epic.)

**Depends on**: v0.19, for the C ABI it extends, and v0.20, for the failure
reporting that ABI gains there.

---

### v0.22 — SVG as a second producer — open

**Revised at the v0.20 phase-end revision (2026-08-18): its four items are now
four issues.** The two that were named here and filed nowhere are #1242, the SVG
vocabulary profile, and #1243, the census harness. Nothing about the slice's
scope, order or entry condition changed — what changed is that a `story` listing
now returns all four rather than half of them.

**Three failures make work invisible and they are not the same failure**, which
this revision had to separate before it could act: an issue on **no milestone**
is what v0.20 was planned to fix, 55 of them; an issue named by **no epic** is
what this revision found on v0.21, nine of them; and an issue with **no label**
is #859, which appeared in no `story` listing until the v0.19 revision gave it
one, and which was missing from epic #1106's story table so that only a sentence
named it. **An epic sweep passes it** — the epic did name it — which is why the
ritual now checks labels as a third level rather than two. These two v0.22 items
are a fourth: named in this file's prose and filed as no issue at all, so every
one of the three queries misses them.

**Checked and unchanged at the v0.19 phase-end revision (2026-08-16).** Two
stories, #848 and #774, and no epic — the right shape for a slice opened but not
yet planned, and nothing v0.19 taught bears on it. Recorded so a later reader
can tell "checked" from "not checked". **Its story count is superseded by the
block above**: four since 2026-08-18. The "no epic" reading is not, and still
holds.

**No epic filed yet.** A second producer beside Figma. Proposed on 2026-08-09 as
v0.21 and moved here on 2026-08-12. No other slice depends on it and it depends
on none, so its position is a priority choice rather than a dependency.

**What it tests is P5** — that the dashscene document is a schema-first IR with
its own specification, and that no producer's limitations define the format.
That claim has never been tested, because Figma has been the only producer. A
second one is the test.

Four items in dependency order, all four the slice's own work: the SVG
vocabulary profile, which is the P4 prerequisite because refusing by name needs
a list to refuse against; stroke-to-fill before baking, without which 46 % of
icon content has no fill to bake; the icon import itself; and a census harness
that runs both corpora, publishes the counts and gates on zero silent drops,
which is what makes the profile falsifiable. **All four are filed**: #1242,
#848, #774 and #1243, in that order.

**#1242 can be built before the entry condition is settled, and #1243 can be
started**, which is the one place v0.20 changed how the slice would be begun
rather than what it contains. The profile is a specification document and needs
no dependency. The census harness needs its two corpora pinned before it needs
an importer, so that half can be done early; the harness itself waits on #774.
v0.21 ran that ordering by accident and it worked — stories #1229 and #1230
built the Android measurement apparatus and the Unity build environment before
the device and the rulings arrived. #1229 was spent the day it landed; #1230's
payoff was still owed when this was written, because epic #1106 then had two
open entry conditions — which was the point rather than a qualification of it,
since the toolchain was ready for the day they lifted. **They lifted on
2026-08-18**, and that day's ruling also moved the target to Unity 6.5, whose
installed editor carried no Android module — reversed to Unity 6.3 LTS on
2026-08-20
([`decisions/unity-painter-uses-brg.md`](decisions/unity-painter-uses-brg.md)
D2, which carries the reasons).

**One entry condition, owner-supplied**: the `usvg` dependency adopted. The
licence question that was open when this was proposed is resolved — the front
half is Apache-2.0 OR MIT — but the adoption decision has not been made.
Background, and both candidate corpora measured on the day, are in the
2026-08-09 design capture under `docs/wip/`, which moves to `docs/archive/` when
this slice lands; issue #914 covers that class of citation.

Depends on: nothing. Blocked by: its one entry condition.

### v0.23 — rolling quick debt — open, and not a slice

**A holding milestone rather than a slice**, and the one entry under this
heading that delivers nothing. It has no epic, no deliverable and no close: it
is worked between slices and at each slice close.

It holds the quick items of the 2026-08-12 sweep — one focused pull request
each, under half a day, which is the milestone's own threshold rather than an
approximation of it.
[`decisions/review-before-ready-not-before-open.md`](decisions/review-before-ready-not-before-open.md)
fixes a quick finding in the pull request that found it rather than filing it,
so the only review-sourced item that still lands here is a **blocked** one — a
quick, non-correctness finding waiting on a ruling or on hardware. **Two kinds
sit in it, and the `owner-input` label separates them.** An unlabelled item is
burn-down work a session can finish alone. A labelled item is the third term of
[`decisions/pre-v1-hardening-slice.md`](decisions/pre-v1-hardening-slice.md):
resolvable now, gated on no v1 consumer, and still not burn-down work, because
the next step is a ruling or an input only the repository owner can supply. A
session taking work from here reads the label first, and derives the population
rather than reading a count:

    gh issue list --label owner-input --milestone "v0.23 — rolling quick debt"

**This sentence has stated that count twice and both statements went stale** —
four until 2026-08-16, then two, which was correct on the day and had multiplied
several times over before the sentence was next read. It no longer states a
count; run the query above.

**Revised at the v0.20 phase-end revision (2026-08-18): read as a population,
not as a queue.** That slice added twenty-four items here and nobody had ever
looked at the milestone as a whole. Doing so once found two duplicates that
later work had already repaired without naming them — #511, repaired by #1193,
and #647, repaired by #1186, both closed by that revision. It found adjacent
pairs that read as one item: #1033 and #1060 make the same statement about
`dashscene-desktop` and `dashscene-ffi` duplicating each other, were filed on
the same day, and **both already cite the same cause, #925** — so the link
exists and the split survived it. And it found one item on the wrong milestone
for its size: **#1241**, filed at the v0.20 close as the remainder of a gate and
placed here, whose own body is headed "Why this is not a quick fix". It moved to
`v1` beside #1246. This milestone takes nothing over half a day, and an item
sized against the gate it came from rather than against the milestone it lands
on is the third thing a population view shows.

**So the phase-end ritual now gives this milestone a cluster pass**, described
in the ritual section at the top of this file. It is not a re-verification pass:
a large part of the population asserts an **absence** — "no test covers X" — and
an absence is checkable only by mutating the code and running the tier, which is
a slice's work rather than a step inside a revision.
[`decisions/slices-are-planned-against-their-inflow.md`](decisions/slices-are-planned-against-their-inflow.md)
records what was and was not checked.

It exists so that the three slices above stay readable. A slice whose scope is
"the critical findings" means nothing if forty cosmetic items are scheduled
beside them, and the alternative — leaving them unmilestoned — is the state this
sweep was called to fix. **An item here that turns out to block something moves
to the slice it blocks**, which is the only rule the milestone needs.

## v1 — full feature set, performance, production toolchain

**The Unity painter and the C# host moved out of this section on 2026-08-12**,
to slice v0.21 above, and the section was named after them until then. What
stays here is the work that unlocks once a Unity host exists rather than the
host itself. The GitHub milestone was renamed to match on 2026-08-13, and
[`decisions/host-integration-in-three-layers.md`](decisions/host-integration-in-three-layers.md)
re-scoped with it: iOS stays v1, Unity is v0.21.

**Layers 1 and 2 are here, for every host, since 2026-08-18** — the ruling that
settled which layer a Unity host occupies deferred the two above it rather than
taking them into v0.21. **#1261** is layer 1, app state as signals, and it
carries that ruling's own broadening: signal binding is to be reachable from C#
**or** native code, because by D2 it sits on the C ABI and every host inherits
it. **#1262** is layer 2, scenes authored in the host's language over
`dashlang`. Neither is built for any platform today, which is what makes them v1
rather than debt.

LATER-tier features land per priority, including shadow baking switching on and
`profile:core` being enforced on target documents; **of the loading-performance
work, only placeholder activation remains here** — its foundations (the
sectioned envelope, the asset table, the KTX2 texture pipeline) landed in
v0.11–v0.12, and the mapping, the prefetch choreography and the startup-scaling
benchmark that makes R5 falsifiable moved to v0.16 at the v0.13 close, because a
ratio needs no target hardware to measure; what stays is blocked on a producer
supplying the placeholder colour, not on loading (guardrail G-20,
[`specification/05-qualification.md`](specification/05-qualification.md));
rendering performance (tiler rules measured on target hardware; whether the lean
native painter lands here or later is decided on those measurements, not in
advance); and the production toolchain — `dashc` as a shipped product, with a
stable CLI, versioned diagnostics, a waiver workflow, linter rule packs, and
golden/report tooling for design review.

Two things were added to v1 at the v0.13 open (2026-07-27), both because they
need a number only target hardware can supply:

- **The perf and allocation debt, selected against the measured performance pass
  (epic #476, 20 items).** Deferred out of v0.13's burn-down because none has a
  measurement behind it. The epic states its own entry condition — the
  performance pass runs first and produces a profile, then these are selected,
  ordered and validated against it, and an item the profile shows is not on a
  hot path is closed as measured-and-not-worth-it. Held as an epic rather than
  loose on the milestone precisely so they do not repeat the "buried under v1"
  failure that
  [`decisions/pre-v1-hardening-slice.md`](decisions/pre-v1-hardening-slice.md)
  exists to fix.
- **The packer's memory budget and the target display resolution (#462),** set
  alongside #170's measurable-requirements work. Neither exists anywhere in
  `docs/specification/` today —
  [`specification/03-target-hardware-rules.md`](specification/03-target-hardware-rules.md)
  carries R-T1 to R-T4 and no number. `dashpack` treats a profile exceeding the
  budget as a validator error, so **for the whole of v0 that error is
  unreachable**: a document can pack successfully and still not fit the target,
  and nothing detects it. The per-asset bands still bind — each is measured and
  ships the mutation that fails it — but the aggregate residency contract does
  not. That is an accepted gap from the 2026-07-27 ruling, recorded here so it
  is visible rather than implied.

Full script coverage (v1, epic #463): v0 ships Latin and Arabic, which v0.6
delivered, and that is the whole of v0's language scope. Everything beyond it is
v1 — Arabic weight parity (one face today against Latin's four), a CJK scope
decision (CJK appears nowhere in the specification, so it has never been ruled
in or out), Indic support for the commercially load-bearing scripts, and the
glyph-atlas residency design all three depend on.

They are one epic rather than four because they cannot be solved separately: CJK
cannot ship without residency, residency cannot be designed without knowing
which scripts it must hold, and Indic constrains the same closure that residency
needs — text-driven rather than charset-driven
([`decisions/glyph-coverage-is-declared-at-build-time.md`](decisions/glyph-coverage-is-declared-at-build-time.md)).
Designing residency against Latin and Arabic alone would mean designing it
twice.

Open spike (v1): platform-font provisioning — resolve and hash-pin target fonts
at build time so a platform-provided font is baked through the same atlas
pipeline as a bundled one (guardrail G-2) — plus a target-hardware benchmark of
platform text raster against the MSDF-atlas path. It feeds the Q-1 small-size
decision ([`technotes/open-questions.md`](technotes/open-questions.md)), which
resolved MSDF-only for v0.

Full-feature-set candidate (v1): the remainder of the gauge and radial animation
vocabulary. **Revised at the v0.18 phase-end (2026-08-11): the rotation half is
built and this paragraph said otherwise.** A bound scalar driving rotation about
a pivot landed at slice v0.18 — the angle and both pivot coordinates are
bindable channels, with matching document rows and an override arm — so what
remains a v1 candidate is the **arc sweep** over absolute placement, which rides
on dashcue's per-prop smoothing row and has no property today. Neither is a
layout mode
([`decisions/radial-is-not-a-layout-mode.md`](decisions/radial-is-not-a-layout-mode.md)).

This paragraph is the reason the phase-end re-check reads the whole file rather
than the slice being closed: it is four hundred lines from v0.18's own entry,
and the same stale claim was corrected in [`features.md`](features.md) in the
same pass without this copy being noticed until review.

## v2 — remote/streaming

Scenes, and scene updates, streamed to displays not local to the renderer. The
architecture is already shaped for this: streaming a scene is streaming its
`.dsb` once, plus the staged-mutation commit stream — descriptive animation
keeps updates small, specs rather than frames. The wire format is the same
flatbuffer schema used for the file; the remote end runs a painter behind the
same trait
([`decisions/remoting-two-transports.md`](decisions/remoting-two-transports.md)
records the accepted direction and what it already binds in v0). Open then:
transport refinements, remote painter choice, latency budgets, and the admission
policy for untrusted producers
([`technotes/open-questions.md`](technotes/open-questions.md), Q-5).
