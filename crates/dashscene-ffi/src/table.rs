//! The per-thread table a [`crate::DsRuntime`] handle names.
//!
//! A handle is resolved here, and the resolution is the whole safety property
//! of `docs/decisions/the-c-abi-runtime-handle-is-generational.md`: a stale
//! handle, a forged handle and a handle from another thread each produce a
//! status rather than undefined behaviour.
//!
//! # Thread-affine, and unique for the life of the process
//!
//! The record asks for both, and its rationale section argues for the first by
//! avoiding process-wide state — which the second requires. **They are
//! reconciled by where the process-wide state sits: uniqueness is a property
//! of how a handle is _minted_, not of how it is _resolved_.**
//!
//! [`NEXT_THREAD`] is the only object shared between threads. It is touched
//! once per thread, on that thread's first successful mint, to draw a thread
//! number that is never recycled. No lookup reads it, and no call on the frame
//! path reads it. Everything else lives in a `thread_local!`, which is
//! reachable only from its owning thread and therefore needs no lock at all.
//!
//! Without the thread field, two threads' first handles would be the same
//! value — a per-thread slot index with a per-thread generation is exactly the
//! design the record's rationale pushes an implementer toward, and exactly the
//! defect its decision 2 forbids.

use std::cell::RefCell;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicU32, Ordering};

use dashlang::LiveScene;
use dashscene_core::Arena;
use dashscene_gpu::{GpuPainter, SurfaceRenderer};

use crate::handle::{self, MAX_GENERATION, MAX_INDEX, MAX_THREAD};

/// A live runtime: the arena, the scene over it, and the surface it draws to.
///
/// C never sees this. A host holds a handle, so this layout is free to change
/// without moving `DS_ABI_VERSION`.
pub(crate) struct Runtime {
    pub(crate) arena: Arena,
    pub(crate) scene: Option<LiveScene>,
    pub(crate) surface: Option<SurfaceRenderer>,
    /// Boundary B's implementation: it turns the committed tables into an
    /// instance buffer and knows nothing about the window. Held rather than
    /// built per frame, because it owns the packing buffers whose byte ranges
    /// the dirty set decides to upload.
    pub(crate) painter: GpuPainter,
    /// Whether a frame lease is outstanding: `ds_runtime_acquire_frame` has
    /// handed a host borrowed views into [`Self::arena`]'s committed tables
    /// and `ds_runtime_release_frame` has not been called yet.
    ///
    /// **It lives on the runtime rather than in the table**, because a lease
    /// spans calls: the acquire returns, the host dispatches its jobs, and the
    /// release arrives later. A checkout, which `with_runtime` already does,
    /// is the other shape — it lasts one call and cannot express this.
    ///
    /// While it is set, every path that would commit is refused. That is the
    /// whole enforcement: a commit is the only thing that replaces the tables
    /// those views point into (issue #1267, story #859).
    pub(crate) frame_leased: bool,
    /// Whether a document has been installed since the host last took a
    /// frame — the host-draws half of `Present::document_replaced`, which
    /// reaches only an attached surface (story #859).
    ///
    /// Set by `announce_document_replaced`, cleared by the acquire that
    /// reports it, so a host reading it every frame sees each replacement
    /// exactly once.
    pub(crate) document_replaced: bool,
    /// Test-only. Held **for its `Drop`, never read** — hence the underscore:
    /// dropping this field is the observation, and a test asks the counter it
    /// carries rather than asking the runtime.
    #[cfg(test)]
    _dropped: Option<DropTag>,
}

/// Bumps its counter when it is dropped, so one test can observe **its own**
/// runtime's drop.
///
/// A global counter cannot: the suite runs tests in parallel and other
/// runtimes are dropping the whole time, so a before/after delta races. The
/// tag carries the `Drop` rather than `Runtime` doing so, which keeps the
/// shipped type's move semantics identical in test builds — an
/// `impl Drop for Runtime` would make it non-destructurable under `cfg(test)`
/// and compile differently from `cargo build`.
#[cfg(test)]
pub(crate) struct DropTag(pub(crate) std::sync::Arc<std::sync::atomic::AtomicUsize>);

#[cfg(test)]
impl Drop for DropTag {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

impl Runtime {
    pub(crate) fn new() -> Self {
        Self {
            arena: Arena::new(),
            scene: None,
            surface: None,
            painter: GpuPainter::new(),
            frame_leased: false,
            document_replaced: false,
            #[cfg(test)]
            _dropped: None,
        }
    }

    /// A runtime whose drop bumps `tag`.
    #[cfg(test)]
    pub(crate) fn tagged(tag: DropTag) -> Self {
        Self {
            _dropped: Some(tag),
            ..Self::new()
        }
    }
}

/// Why a handle did not resolve.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum LookupError {
    /// Zero, which names no runtime by construction.
    Zero,
    /// Not reachable on this thread right now. `resolve` produces it for a
    /// **zero thread field**, which no mint ever writes; an **index past this
    /// thread's table**; and a slot that fails `runtime.is_some() &&
    /// generation matches`.
    ///
    /// That last test is one short-circuiting expression, not two, and it
    /// covers three situations: the slot is **vacant** because the handle was
    /// freed, it is **checked out** by a call already in flight on that
    /// handle, or it is **occupied by a later runtime** whose generation has
    /// moved past this handle's — the freed-then-re-minted case. Only the
    /// zero-thread shape is ruled out by the thread field.
    Bad,
    /// The handle's thread field is non-zero and not this thread's. That is
    /// all `resolve` knows: the number may name a live thread, one that has
    /// exited, or one never drawn at all. Separating those needs a
    /// process-wide registry on the lookup path — the shared state this design
    /// exists not to have (issue #1267).
    WrongThread,
}

/// Why a handle could not be minted.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum MintError {
    /// This thread holds the maximum number of live runtimes, or the process
    /// has drawn every thread number a handle can carry. Never a wrap.
    Exhausted,
}

struct Slot {
    /// The generation this slot is **currently** answering to: the value
    /// `mint` packs into the handle it hands out, and the value `resolve`
    /// compares against. `remove` advances it, which is what makes the handle
    /// it just freed stop resolving.
    ///
    /// `None` retires the slot: its generation ran out, so it is never reused
    /// rather than wrapping back onto a value already handed out.
    generation: Option<u32>,
    runtime: Option<Box<Runtime>>,
}

struct Table {
    /// This thread's number, drawn on its first successful mint and never
    /// again. `None` before that.
    thread: Option<u32>,
    slots: Vec<Slot>,
    /// Vacant, non-retired slot indices.
    free: Vec<u16>,
}

impl Table {
    const fn new() -> Self {
        Self {
            thread: None,
            slots: Vec::new(),
            free: Vec::new(),
        }
    }
}

impl Drop for Table {
    /// **A runtime still here when the thread exits is leaked, deliberately.**
    ///
    /// This is the record's open question 6. Dropping would run
    /// `wgpu::Surface`'s destructor at thread-exit time — on Android, after
    /// `surfaceDestroyed` has returned and `ANativeWindow_release` has run,
    /// which is a use-after-free of the window. The old pointer design could
    /// not do that: a handle the host never freed was a `Box` that was simply
    /// never dropped.
    ///
    /// So the behaviour is unchanged from before the handle became an integer:
    /// a host that does not free still leaks, and now it cannot do worse than
    /// leak. `std::LocalKey` also documents that pthread-based TLS destructors
    /// may not run on the main thread at all, so this cannot be the path a
    /// host relies on for teardown in any case.
    ///
    /// **This loop must stay infallible, and must not become conditional.**
    /// Dropping only the runtimes that hold no surface was tried and reverted:
    /// it lets a destructor panic here, and a panic part-way through leaves
    /// the remaining slots to the automatic drop glue — which drops a
    /// *surface-holding* runtime normally, committing exactly the
    /// use-after-free this exists to prevent. It also decides "is this
    /// hazardous to drop" by probing one field, which goes stale in silence
    /// the moment another field owns a platform resource.
    fn drop(&mut self) {
        #[cfg(test)]
        TABLE_DROPS.fetch_add(1, Ordering::Relaxed);

        for slot in &mut self.slots {
            if let Some(runtime) = slot.runtime.take() {
                std::mem::forget(runtime);
            }
        }
    }
}

/// Counts `Table::drop` runs, so a test can tell "the destructor ran and
/// forgot" from "the destructor never ran" — `std::LocalKey` documents that
/// pthread-based TLS destructors may be skipped, and without this the leak
/// assertion would pass on a target that skips them.
#[cfg(test)]
static TABLE_DROPS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static TABLE: RefCell<Table> = const { RefCell::new(Table::new()) };
}

/// **The only process-wide object in this design.** Advanced once per thread,
/// inside the first `mint` on that thread. Read by no lookup and by no call on
/// the frame path.
static NEXT_THREAD: AtomicU32 = AtomicU32::new(1);

/// Draws this thread's number, or reports exhaustion. Never recycles.
fn thread_number(table: &mut Table) -> Result<u32, MintError> {
    if let Some(existing) = table.thread {
        return Ok(existing);
    }
    let drawn = NEXT_THREAD
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            (n <= MAX_THREAD).then_some(n + 1)
        })
        .map_err(|_| MintError::Exhausted)?;
    table.thread = Some(drawn);
    Ok(drawn)
}

/// Puts `runtime` in this thread's table and returns the handle naming it.
pub(crate) fn mint(runtime: Runtime) -> Result<u64, MintError> {
    TABLE.with_borrow_mut(|table| {
        let thread = thread_number(table)?;

        // A vacant slot first; only then grow. A retired slot is in neither
        // list, so it is never handed out again.
        let index = match table.free.pop() {
            Some(index) => index,
            None => {
                let next = table.slots.len();
                if next > usize::from(MAX_INDEX) {
                    return Err(MintError::Exhausted);
                }
                table.slots.push(Slot {
                    generation: Some(1),
                    runtime: None,
                });
                next as u16
            }
        };

        let slot = &mut table.slots[usize::from(index)];
        let generation = slot.generation.expect("a vacant slot is never retired");
        slot.runtime = Some(Box::new(runtime));
        Ok(handle::pack(thread, index, generation))
    })
}

/// Runs `f` against the runtime `handle` names, on this thread.
pub(crate) fn with_runtime<T>(
    value: u64,
    f: impl FnOnce(&mut Runtime) -> T,
) -> Result<T, LookupError> {
    // **The runtime is checked out for the call, not borrowed across it.**
    // Holding the `RefCell` borrow while `f` runs would make any re-entrant
    // call — a host callback that calls back in during a draw — a
    // `BorrowMutError` panic surfaced as `DsStatus::Panic` rather than a
    // defined status. Story #859's data plane was named here as the entry
    // point that would invite one, and it is not: it hands out memory and
    // takes no function pointer, so a host's workers read rows without
    // calling in at all. Moving the `Box` out costs one pointer.
    //
    // While it is out the slot is occupied-but-empty, so the generation still
    // matches and a re-entrant call on the *same* handle answers `Bad` rather
    // than aliasing `&mut Runtime`. That is the borrow rule the old pointer
    // design left to the host to honour, now enforced.
    // Returned by the guard below on **both** paths, so an unwind out of `f`
    // leaves the runtime where it was. Without that, a panic — which `guard`
    // reports as `DsStatus::Panic` — would destroy the runtime and strand its
    // slot: occupied by nothing, generation un-advanced, index never freed, so
    // the host could not even free the handle afterwards.
    let mut checkout = TABLE.with_borrow_mut(|table| {
        let index = resolve(table, value)?;
        let runtime = table.slots[usize::from(index)]
            .runtime
            .take()
            .expect("a resolved slot holds a runtime");
        Ok::<_, LookupError>(Checkout {
            index,
            runtime: Some(runtime),
        })
    })?;

    Ok(f(checkout
        .runtime
        .as_mut()
        .expect("the checkout holds the runtime for the call")))
}

/// A runtime taken out of the table for the duration of one call, and put back
/// when the call ends — whether it returns or unwinds.
struct Checkout {
    index: u16,
    runtime: Option<Box<Runtime>>,
}

impl Drop for Checkout {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            TABLE.with_borrow_mut(|table| {
                table.slots[usize::from(self.index)].runtime = Some(runtime);
            });
        }
    }
}

/// The lookup both `with_runtime` and `remove` perform: a handle to a live
/// slot index on **this** thread, or the reason it is not one.
fn resolve(table: &Table, value: u64) -> Result<u16, LookupError> {
    if value == 0 {
        return Err(LookupError::Zero);
    }
    let (thread, index, generation) = handle::unpack(value);

    // Thread numbers start at 1, so a zero field is a value this library
    // never minted — a forged handle, or a leftover integer. It is `Bad`
    // rather than `WrongThread`, because the header gives `WrongThread` the
    // remedy "call from the thread that created it" and that handle has no
    // creating thread.
    if thread == 0 {
        return Err(LookupError::Bad);
    }
    // Otherwise the thread field is checked before anything else, so a handle
    // minted elsewhere can never index this thread's table — even if its index
    // and generation would happen to fit.
    //
    // A forged value whose thread field happens to name some *other* live
    // thread still reads as `WrongThread`. Distinguishing that would mean
    // asking a process-wide registry which numbers were ever drawn, on the
    // lookup path — the shared state this design keeps off it.
    if table.thread != Some(thread) {
        return Err(LookupError::WrongThread);
    }
    let slot = table
        .slots
        .get(usize::from(index))
        .ok_or(LookupError::Bad)?;

    // An occupied slot answers to exactly the generation it was minted with;
    // `remove` is the only thing that moves it.
    let live = slot.runtime.is_some() && slot.generation == Some(generation);
    live.then_some(index).ok_or(LookupError::Bad)
}

/// Frees the runtime `value` names and retires that handle value.
///
/// The slot's generation advances, so the handle just freed answers
/// [`LookupError::Bad`] forever after. A slot whose generation has run out is
/// retired rather than wrapped: it returns to neither the free list nor the
/// slot pool, because reusing it would hand out a value already given once.
pub(crate) fn remove(value: u64) -> Result<(), LookupError> {
    // The slot is advanced and released **first**, then the runtime is dropped
    // outside the borrow. Two reasons, and both are the same hazard the
    // checkout in `with_runtime` exists for: `Runtime` owns a wgpu device and
    // surface whose destructors can re-enter this library, which under a held
    // borrow is a `BorrowMutError`; and if one of them unwinds, the slot has
    // already been made reusable rather than stranded half-freed.
    let runtime = TABLE.with_borrow_mut(|table| {
        let index = resolve(table, value)?;
        let slot = &mut table.slots[usize::from(index)];
        let runtime = slot.runtime.take();

        match slot.generation {
            Some(g) if g < MAX_GENERATION => {
                slot.generation = Some(g + 1);
                table.free.push(index);
            }
            // Exhausted: retire it. Never wrap.
            _ => slot.generation = None,
        }
        Ok::<_, LookupError>(runtime)
    })?;

    drop(runtime);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minted_handle_resolves_to_the_runtime_it_named() {
        let a = mint(Runtime::new()).expect("a first runtime mints");
        let b = mint(Runtime::new()).expect("a second runtime mints");
        assert_ne!(a, b, "two live runtimes never share a handle");

        // Distinguish them by a property only one carries.
        with_runtime(a, |r| {
            let mut txn = r.arena.open();
            txn.add_node(None, Some("a"));
            txn.commit();
        })
        .expect("a resolves");
        let a_nodes = with_runtime(a, |r| r.arena.node_count()).expect("a resolves");
        let b_nodes = with_runtime(b, |r| r.arena.node_count()).expect("b resolves");
        assert_eq!(
            (a_nodes, b_nodes),
            (1, 0),
            "each handle reached its own runtime"
        );
    }

    #[test]
    fn a_freed_handle_reports_bad_rather_than_resolving() {
        let h = mint(Runtime::new()).expect("mints");
        remove(h).expect("frees");
        assert_eq!(
            with_runtime(h, |_| ()),
            Err(LookupError::Bad),
            "a freed handle must not resolve — this is what the generation is for",
        );
        assert_eq!(
            remove(h),
            Err(LookupError::Bad),
            "and freeing twice is reported"
        );
    }

    #[test]
    fn a_reused_slot_never_reissues_a_handle_value() {
        let first = mint(Runtime::new()).expect("mints");
        remove(first).expect("frees");
        let second = mint(Runtime::new()).expect("mints again");

        assert_ne!(
            first, second,
            "the slot is reused but the handle value is not — the generation moved",
        );
        assert_eq!(
            with_runtime(first, |_| ()),
            Err(LookupError::Bad),
            "and the old handle still answers Bad rather than driving the new runtime",
        );
    }

    #[test]
    fn a_full_table_refuses_rather_than_overwriting() {
        let live: Vec<u64> = (0..=MAX_INDEX)
            .map(|_| mint(Runtime::new()).expect("fills the table"))
            .collect();
        assert_eq!(
            mint(Runtime::new()),
            Err(MintError::Exhausted),
            "a full table refuses",
        );
        assert!(
            live.iter().all(|h| with_runtime(*h, |_| ()).is_ok()),
            "and every runtime already in it is untouched",
        );

        // Give the slots back. `TABLE` is a thread-local, and under
        // `--test-threads=1` libtest runs every test body on the runner
        // thread — so leaving 4096 runtimes here exhausts the table for every
        // test that mints after this one. nextest hides that by giving each
        // test its own process.
        for handle in live {
            remove(handle).expect("a live handle frees");
        }
    }

    /// **The test that pins decision 2.** Under a per-thread slot index with a
    /// per-thread generation — the shape the record's rationale pushes an
    /// implementer toward — these two values are equal.
    #[test]
    fn two_threads_first_handles_are_different_values() {
        let first = std::thread::spawn(|| mint(Runtime::new()).expect("mints"))
            .join()
            .expect("thread one");
        let second = std::thread::spawn(|| mint(Runtime::new()).expect("mints"))
            .join()
            .expect("thread two");

        assert_ne!(
            first, second,
            "a handle value identifies at most one runtime for the life of the \
             process, so two threads' first handles cannot collide",
        );
    }

    #[test]
    fn a_handle_from_another_thread_is_wrong_thread_and_drives_nothing_local() {
        let foreign = std::thread::spawn(|| {
            let h = mint(Runtime::new()).expect("mints");
            with_runtime(h, |r| {
                let mut txn = r.arena.open();
                txn.add_node(None, Some("theirs"));
                txn.commit();
            })
            .expect("resolves");
            h
        })
        .join()
        .expect("the foreign thread");

        let mine = mint(Runtime::new()).expect("mints locally");
        assert_eq!(
            with_runtime(foreign, |_| ()),
            Err(LookupError::WrongThread),
            "a foreign handle is named as such",
        );
        assert_eq!(
            with_runtime(mine, |r| r.arena.node_count()).expect("mine resolves"),
            0,
            "and it did not reach into this thread's own table",
        );
    }

    #[test]
    fn a_thread_number_is_drawn_once_per_thread() {
        let a = mint(Runtime::new()).expect("mints");
        let b = mint(Runtime::new()).expect("mints");
        let (ta, _, _) = handle::unpack(a);
        let (tb, _, _) = handle::unpack(b);
        assert_eq!(
            ta, tb,
            "one thread draws one number, however many runtimes it holds"
        );

        let other = std::thread::spawn(|| mint(Runtime::new()).expect("mints"))
            .join()
            .expect("other thread");
        let (to, _, _) = handle::unpack(other);
        assert_ne!(ta, to, "a different thread draws a different number");
    }

    #[test]
    fn zero_names_no_runtime() {
        assert_eq!(with_runtime(0, |_| ()), Err(LookupError::Zero));
        assert_eq!(remove(0), Err(LookupError::Zero));
    }
}

#[cfg(test)]
mod forged {
    use super::*;

    /// A re-entrant call on a *different* runtime resolves rather than
    /// panicking. Holding the table's borrow across the caller's closure would
    /// make this a `BorrowMutError`, which `guard` reports as
    /// `DsStatus::Panic`. No entry point takes a callback today — story #859's
    /// data plane was expected to be the first and is not — so this is the
    /// shape being kept correct ahead of one, rather than a case a host can
    /// reach.
    #[test]
    fn a_re_entrant_call_on_another_runtime_resolves() {
        let outer = mint(Runtime::new()).expect("mints");
        let inner = mint(Runtime::new()).expect("mints");

        let reached = with_runtime(outer, |_| with_runtime(inner, |r| r.arena.node_count()))
            .expect("the outer call resolves");
        assert_eq!(reached, Ok(0), "and the inner one reached its own runtime");
    }

    /// A re-entrant call on the *same* handle is refused rather than handing
    /// out a second `&mut` to one runtime.
    #[test]
    fn a_re_entrant_call_on_the_same_runtime_is_refused() {
        let h = mint(Runtime::new()).expect("mints");
        let inner = with_runtime(h, |_| with_runtime(h, |_| ())).expect("outer resolves");
        assert_eq!(
            inner,
            Err(LookupError::Bad),
            "aliasing one runtime is refused, not granted",
        );
    }

    /// A panic inside the call leaves the runtime where it was.
    ///
    /// `guard` reports an unwind as `DsStatus::Panic` and the crate documents
    /// that the runtime is still alive afterwards. Checking the runtime out
    /// for the call would break that without a guard putting it back: the slot
    /// would be occupied by nothing, its generation un-advanced and its index
    /// never freed, so the handle would answer `Bad` from every entry point
    /// including the one that frees it.
    #[test]
    fn a_panic_inside_the_call_leaves_the_runtime_usable() {
        let h = mint(Runtime::new()).expect("mints");

        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_runtime(h, |_| panic!("the caller's closure panics"))
        }));
        assert!(unwound.is_err(), "the panic escaped the call, as it would");

        assert!(
            with_runtime(h, |r| r.arena.node_count()).is_ok(),
            "and the handle still names its runtime",
        );
        assert_eq!(remove(h), Ok(()), "and the host can still free it");
    }

    /// A forged handle whose thread field names a number **no thread has ever
    /// drawn** still reads as `WrongThread`, not `Bad`.
    ///
    /// `resolve` compares against this thread's number and asks nothing else —
    /// knowing which numbers were ever issued would put a process-wide
    /// registry on the lookup path. This pins the documented behaviour rather
    /// than the behaviour one might assume from the word "forged".
    #[test]
    fn a_forged_handle_with_an_unissued_thread_is_wrong_thread() {
        let _live = mint(Runtime::new()).expect("mints");
        // Thread field 0xF_FFFF: the maximum, which a single-threaded test
        // process has certainly not drawn.
        let forged = handle::pack(MAX_THREAD, 0, 1);
        assert_eq!(
            with_runtime(forged, |_| ()),
            Err(LookupError::WrongThread),
            "any non-zero thread field that is not ours reads as WrongThread, \
             whether or not the number was ever issued",
        );
    }

    #[test]
    fn a_forged_handle_is_bad_not_wrong_thread() {
        // Mint first, so this thread has a number and the lookup is not
        // answering from "this thread never minted anything".
        let _live = mint(Runtime::new()).expect("mints");
        assert_eq!(
            with_runtime(1, |_| ()),
            Err(LookupError::Bad),
            "a value nothing minted is Bad — the header says WrongThread's \
             remedy is to call from the creating thread, which cannot succeed \
             for a handle that has no creating thread",
        );
    }
}

#[cfg(test)]
mod thread_exit {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    /// A runtime still in the table when its thread exits is **not dropped**.
    ///
    /// Its destructor would run `wgpu::Surface`'s at thread-exit time — on
    /// Android after `surfaceDestroyed` returned and `ANativeWindow_release`
    /// ran, which is a use-after-free of the window. The old pointer design
    /// could not do that: an unfreed handle was a `Box` nothing dropped. This
    /// is the record's open question 6, and it was held by a comment alone.
    #[test]
    fn a_runtime_left_in_an_exiting_threads_table_is_not_dropped() {
        let tag = Arc::new(AtomicUsize::new(0));
        let theirs = Arc::clone(&tag);
        let before_drops = TABLE_DROPS.load(Ordering::Relaxed);

        std::thread::spawn(move || {
            mint(Runtime::tagged(DropTag(theirs))).expect("mints");
            // Deliberately not freed: this is the host that forgot.
        })
        .join()
        .expect("the thread exits, running its TLS destructor");

        assert_eq!(
            tag.load(Ordering::Relaxed),
            0,
            "thread exit must leak the runtime, never drop it",
        );
        assert!(
            TABLE_DROPS.load(Ordering::Relaxed) > before_drops,
            "and the table's destructor must actually have run — otherwise the \
             assertion above passes on a target that skips TLS destructors, \
             which is the case `Table::drop`'s own doc names",
        );
    }

    /// And a runtime the host *does* free is dropped — so the test above
    /// measures a leak rather than a counter that never moves.
    #[test]
    fn a_freed_runtime_is_dropped() {
        let tag = Arc::new(AtomicUsize::new(0));
        let theirs = Arc::clone(&tag);

        std::thread::spawn(move || {
            let h = mint(Runtime::tagged(DropTag(theirs))).expect("mints");
            remove(h).expect("frees");
        })
        .join()
        .expect("the thread exits");

        assert_eq!(
            tag.load(Ordering::Relaxed),
            1,
            "an explicitly freed runtime is dropped",
        );
    }
}
