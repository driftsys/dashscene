//! Debt issue #79 acceptance: a single-child chain deep enough to
//! overflow a recursive walk or a recursive drop must still build and
//! drop cleanly. `Scene::build`'s private `add()` and `Node`'s `Drop`
//! impl both used to recurse once per tree level; a 200,000-level chain
//! reliably overflowed the stack before the fix (empirical finding
//! behind issue #79).
//!
//! Both tests run the deep-tree work on a thread with a smaller-than-
//! default stack, so the overflow (before the fix) is reached quickly
//! and deterministically rather than depending on the host's default
//! main-thread stack size.

use dashlang::{Arena, anon, scene};

/// How deep a single-child chain needs to be to overflow a recursive
/// walk or drop on a 1 MiB thread stack. Empirically well past the
/// point either recursive implementation crashes in a debug build,
/// comfortably below what the iterative fix needs (it holds the chain
/// on the heap, not the call stack).
const CHAIN_DEPTH: usize = 200_000;

/// A deliberately small stack: smaller than a typical default thread
/// stack, so a recursive implementation overflows it well before a
/// realistic corpus depth, rather than merely running slow.
const SMALL_STACK_BYTES: usize = 1 << 20;

fn deep_chain() -> dashlang::Node {
    let mut n = anon().size(1.0, 1.0);
    for _ in 0..CHAIN_DEPTH {
        n = anon().size(1.0, 1.0).child(n);
    }
    n
}

/// `Scene::build` walks the whole authored tree (`add()`) and then
/// drops the built `Scene` value (its `roots: Vec<Node>`) at the end of
/// the statement — so this single test exercises both the walk and the
/// drop of the same deep chain.
#[test]
fn deep_chain_scene_build_does_not_overflow_stack() {
    let handle = std::thread::Builder::new()
        .stack_size(SMALL_STACK_BYTES)
        .spawn(|| {
            let mut arena = Arena::new();
            let built = scene([deep_chain()]).build(&mut arena);
            assert_eq!(built.generation(), 1);
            assert_eq!(arena.committed().rects().len(), CHAIN_DEPTH + 1);
        })
        .expect("spawn the deep-chain build thread");
    handle
        .join()
        .expect("deep-chain Scene::build must not overflow its thread's stack");
}

/// A `Node` tree that is never staged into an arena still has to drop
/// cleanly — this isolates `Node`'s `Drop` impl from `add()`'s walk.
#[test]
fn deep_chain_node_drop_does_not_overflow_stack() {
    let handle = std::thread::Builder::new()
        .stack_size(SMALL_STACK_BYTES)
        .spawn(|| {
            let n = deep_chain();
            drop(n);
        })
        .expect("spawn the deep-chain drop thread");
    handle
        .join()
        .expect("dropping a deep chain must not overflow its thread's stack");
}
