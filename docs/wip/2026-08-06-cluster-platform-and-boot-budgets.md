# Cluster platform targets and boot budgets — the QNX discussion

    status   input, recorded 2026-08-06. Nothing here is implemented and
             nothing is decided. It records a discussion with velizar about
             where the cluster application runs and what it must boot in,
             and traces what those statements reach in this repository.
             Several statements arrived garbled and are marked as such —
             read the "Not yet legible" section before acting on any of them.
             Garden into decisions when the open questions below are ruled on.

## Why this is held rather than decided

The discussion was relayed in note form and parts of it did not survive
transcription. Three terms — `SDV`, `SAIL`, and the condition attached to the
migration gate — appear nowhere in this repository's docs, glossary or code,
so they cannot be resolved from the codebase and were not resolved in the
session. What follows separates what was stated clearly from what was
inferred, because the inferred half would otherwise be copied forward as fact.
That failure mode is not hypothetical here: `README.md` in this directory
records a capture whose stale claim was carried forward for two slices before
anyone re-read it.

## The input

**1. The display scenery is rendered on QNX.** *(reading — the source line was
garbled; "shall be rendered on QNX" is the most probable form.)*

**2. Boot budgets.** Stated as a ladder. The decimal comma in the third entry
is read as European notation.

| budget | milestone                       | whose budget                   |
| ------ | ------------------------------- | ------------------------------ |
| 1.5 s  | first frame                     | plausibly this stack's         |
| 2 s    | safety content available        | system, this stack contributes |
| 3.5 s  | full cluster                    | system                         |
| 15 s   | first Android frame             | not this stack                 |

**3. Display, camera and sound move into QNX.** This is what makes the ladder
coherent. It is a partition rather than one timeline: the latency-regulated
functions — telltales, rear-view camera, warning chimes, all type-approval
items rather than UX preferences — sit on the QNX side and carry the 1.5/2/3.5
budgets. The Android side gets 15 s because nothing on it is safety-critical.

**4. The cluster application stays on `SDV` until it is possible on QNX or
`SAIL`.** So QNX is a target state with an interim platform in front of it,
not a switch being thrown.

**5. Targets and a migration plan are owed**, covering `CCS3` and `CCS2`, with
milestone names reconciled. Two naming systems appeared in the same breath —
model-year designations (`MY27`, `MY28`) and what look like software release
trains (`26.2`, `27.2`, `28.2`) — and the request for "proper milestone names"
is read as an acknowledgement that these two are being conflated. **This is an
action on velizar, not on this repository.** Nothing here should be scheduled
against a milestone until that list arrives.

## What `SW-R` is, and why it is the load-bearing item

`SW-R` is the cluster application, written in **Rust**.

That is the most consequential line in the discussion. A Rust host embeds
`dashscene-core` and a painter directly — no C ABI, no Unity host, no FFI
boundary of the kind `dashscene-unity` exists to serve. It makes `SW-R` the
actual consumer of this stack, and it inverts the platform question: the target
list is whatever `SW-R` builds for, and dashscene inherits it rather than
choosing it.

It also means the interim platform is not a footnote. If `SW-R` ships on `SDV`
before QNX, then **`SDV` is dashscene's first real deployment target**, ahead
of QNX, and the v0 painter set has to satisfy the boot ladder there — or the
ladder applies only post-QNX. Which of those two is true is open.

## Not yet legible

None of these should be guessed at.

- **`SDV`** — Linux-based platform, a specific ECU or SoC, or a vendor product
  name?
- **`SAIL`** — no candidate. Product, silicon, or a mis-transcription.
- **`SW-R`'s gate** — "until we have _possible_ on QNX" is missing its
  subject. Until **what** is possible on QNX: GPU acceleration, a cluster stack
  at all, or certification?
- **`CCS2` / `CCS3`** — platform generations, on the evidence of point 5, but
  unconfirmed.

## What this reaches in existing records

**`docs/decisions/wgpu-is-the-lean-painter.md:94`** — its table of candidate
reasons to keep Skia-GPU lists "the QNX display path" with the verdict
"reachable from `wgpu-hal` as well". That line was written to argue QNX does
not *differentiate* Skia-GPU from wgpu, which is all it needed to do there. It
was never an assertion that anyone had reached QNX, and point 1 promotes it
from a passing dismissal into a load-bearing assumption. Two things under it
now need checking rather than assuming:

- Rust's QNX Neutrino targets are tier 3 — no prebuilt `std`, no CI — which
  implies a `-Z build-std` toolchain story the workspace has no provision for.
- wgpu on QNX means a Vulkan or GLES driver from the SoC vendor. That path is
  real on some silicon and absent on others, so it is an SoC question rather
  than a QNX question, and point 5's target list is what answers it.

**`docs/decisions/startup-scaling-is-measured-by-a-counter.md`** — accepted
2026-08-05, one day before this discussion. Its D1 makes cold-start cost "a
count of bytes, not an elapsed time", explicitly rejecting a timing threshold
because it "will drift, and cannot run on the two-core CI runners without
flaking". D3 stops the measured boundary at the load path and excludes the
frame.

That decision is not wrong, and point 2 does not overturn it — **they answer
different questions**. A byte counter falsifies R5's *scaling* claim, that cost
tracks the shown root rather than file size. It cannot tell anyone whether
first frame lands at 1.5 s on target silicon, which is an absolute wall-clock
budget on named hardware: a bench-on-hardware problem, not a CI problem. If the
budgets are a real requirement, this repository needs a **second instrument
alongside the counter**, and whichever record lands should say so plainly
rather than let the counter appear to cover ground it deliberately does not.

**`docs/technotes/producers-and-ir.md:190`** — files AUTOSAR/QNX integration
as orthogonal to the IR. Points 1 and 3 do not contradict that; the IR stays
orthogonal. What changes is that the *painter and host* are no longer
orthogonal to it.

**`docs/roadmap.md`** — nothing in v0.1 through v0.16 contains a QNX bring-up,
an `SDV` bring-up, or a wall-clock boot benchmark. All three are unbudgeted.

## Open questions

1. Which portion of the 1.5 s first-frame budget is asked of *this* stack,
   versus quoted as the surrounding platform's? Three of the four milestones
   are properties of a whole-system integration — hypervisor, QNX boot, Android
   guest, compositor — and dashscene owns none of the 15 s.
2. Does the boot ladder apply on `SDV`, or only post-QNX?
3. What is the second instrument for wall-clock budgets, and where does it run?
   It cannot be CI on two-core runners, by D1's own reasoning.
4. Does `dashscene-gpu` reach QNX, and on which SoC? Answered by point 5's
   target list, not by this repository.
5. Does a Rust host (`SW-R`) change anything about the umbrella facade or the
   `dashscene-unity` FFI crate's assumed role?

## Gardened when

The terms in "Not yet legible" are resolved and the target list from point 5
arrives. At that stage this splits: a decision record for the measurement
question (open question 3, which is answerable now and does not depend on the
target list), and a roadmap or specification change for the platform targets,
which does.
