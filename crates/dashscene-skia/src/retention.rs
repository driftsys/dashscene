//! Retained group composition — the **policy** half (issue #278).
//!
//! A render-target group composites its rect range `[start, end)` offscreen
//! and blends the result at the group's alpha
//! (`docs/decisions/masks-and-group-opacity.md`). Rebuilding that composite
//! on every commit is what issue #278 records: correct, but it repeats work
//! the commit already said nothing changed in. This module holds the rule
//! for when the previous frame's composite may be blended again, and
//! nothing else.
//!
//! # Why this file names no painter type
//!
//! [`GroupCache`] is generic over `L`, one backend's handle for one
//! composited layer, and never looks inside it. `dashscene-skia` uses a
//! raster `Image` snapshotted from an offscreen surface. A GPU painter
//! would use whatever its own render-target abstraction hands back — a
//! texture view, a render-target index, an atlas slot. Neither word appears
//! below, and neither needs to: the policy is expressed entirely in rect
//! indices, which are boundary-B vocabulary that every painter already
//! reads.
//!
//! That split is the point of the module, so it is worth stating as a rule
//! rather than as a habit: **this file must not depend on `skia_safe`**.
//! `the_policy_names_no_painter_backend` below checks it.
//!
//! # The policy
//!
//! A composite is a function of the pixels its rect range produces. Three
//! facts decide whether the stored one still describes them.
//!
//! 1. **The dirty set is the only evidence of change.** A composite may be
//!    reused only for a frame whose caller supplied one. `None` means the
//!    caller has no dirty information (`Painter::paint`'s contract: a
//!    hand-built table, or a first frame), so nothing about the previous
//!    frame is known and every composite is rebuilt.
//! 2. **A dirty index inside `[start, end)` invalidates.** Every rect in
//!    the range draws into the layer, so any rect the commit reports
//!    changed changes the layer.
//! 3. **The range is the identity.** A composite stored for `[start, end)`
//!    describes that range only. A group whose range moved is a different
//!    composite, and the old one is dropped.
//!
//! Rule 2 is what "invalidated by the same group-diff" means in practice.
//! `dashscene-core`'s commit already dirties **every rect covered by a
//! group present on exactly one side of the commit** — a group forming,
//! dissolving, or changing alpha (`GroupComposite` compares by all three
//! fields, so any of those puts the group on one side only). So the
//! group-diff arrives here already expressed as dirty indices, and rule 2
//! consumes it without a second diff and without a second set of edge
//! cases.
//!
//! # What is deliberately not part of the identity
//!
//! **Alpha.** It is a parameter of the blend, not of the layer: the range
//! composites at full alpha and the group's alpha applies once, when the
//! layer is blended. A stored layer therefore stays valid across an alpha
//! change and is simply blended at the new value. Core dirties the range on
//! an alpha change anyway (see above), so this costs nothing today; it is
//! recorded because a producer that only ever animates a group's alpha is
//! exactly the case a retained painter should be good at.
//!
//! # What this policy assumes of its caller
//!
//! The stored layer holds **pixels**, so it bakes in everything the range
//! drew: paint entries, clip regions, image assets, glyph runs. Reusing it
//! is therefore only sound if the dirty set covers every rect whose
//! *rendered output* changed, not merely every rect whose entry bits
//! changed. That is the standing contract of the dirty set, and
//! `goldens/tooling/tests/dirty_oracle.rs` is the test that holds core to
//! it. A painter must also pass `None` for any frame on which it could not
//! act incrementally at all — a changed rect count, for one — because on
//! such a frame the dirty set does not describe the difference between the
//! two tables.

use dashpaint::GroupComposite;

/// One stored composite: the rect range it covers and the backend's handle
/// for the layer that range produced.
struct Entry<L> {
    start: u32,
    end: u32,
    layer: L,
}

/// The retained composites, one per render-target group, with the rule for
/// when each may be blended again instead of rebuilt.
///
/// `L` is the backend's layer handle; see the module documentation for the
/// policy and for why no painter type appears in this file.
///
/// Usage is three calls per frame, in order:
///
/// 1. [`begin_frame`](Self::begin_frame) — drop what this commit invalidated.
/// 2. [`reuse`](Self::reuse) — per group, ask for a layer to blend.
/// 3. [`store`](Self::store) — per group the caller had to rebuild.
pub struct GroupCache<L> {
    /// Live composites. At most one per group, so bounded by the scene's
    /// render-target group count — the profile's render-target budget
    /// (`dashscene_validator::RENDER_TARGET_BUDGET_PLACEHOLDER`, a warning
    /// gate rather than a hard cap, but the same order of magnitude). The
    /// linear scans below are sized for that, not for the rect table.
    entries: Vec<Entry<L>>,
    builds: u64,
}

impl<L> Default for GroupCache<L> {
    fn default() -> Self {
        Self::new()
    }
}

impl<L> GroupCache<L> {
    /// An empty cache: every group's first frame is a build.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            builds: 0,
        }
    }

    /// Applies this commit to the stored composites, dropping every one the
    /// commit invalidated. Call once per frame, before any
    /// [`reuse`](Self::reuse).
    ///
    /// `groups` is the commit's render-target groups. `dirty` is the rect
    /// indices the commit reports changed, or `None` when the caller has no
    /// usable dirty information for this frame — which drops everything,
    /// per rule 1 of the module policy.
    pub fn begin_frame(&mut self, groups: &[GroupComposite], dirty: Option<&[u32]>) {
        let Some(dirty) = dirty else {
            self.entries.clear();
            return;
        };
        self.entries.retain(|entry| {
            let still_a_group = groups
                .iter()
                .any(|group| group.start == entry.start && group.end == entry.end);
            // Not a binary search: `dirty` is documented sorted where core
            // produces it, but `Painter::paint` states no ordering, and a
            // scan over a set that is nearly always short is not worth
            // making the policy depend on an unstated one.
            let touched = dirty
                .iter()
                .any(|&index| index >= entry.start && index < entry.end);
            still_a_group && !touched
        });
    }

    /// The layer stored for `group`, if this frame may blend it again.
    ///
    /// `None` means the caller must build the composite and hand it to
    /// [`store`](Self::store).
    pub fn reuse(&self, group: &GroupComposite) -> Option<&L> {
        self.entries
            .iter()
            .find(|entry| entry.start == group.start && entry.end == group.end)
            .map(|entry| &entry.layer)
    }

    /// Records a composite the caller has just built, and counts the build.
    pub fn store(&mut self, group: &GroupComposite, layer: L) {
        self.builds += 1;
        match self
            .entries
            .iter_mut()
            .find(|entry| entry.start == group.start && entry.end == group.end)
        {
            Some(entry) => entry.layer = layer,
            None => self.entries.push(Entry {
                start: group.start,
                end: group.end,
                layer,
            }),
        }
    }

    /// How many composites have been built through this cache since it was
    /// created — one per [`store`](Self::store).
    ///
    /// This is the observable the retention is asserted on: a scene whose
    /// groups are stable across `n` frames must leave this at the number of
    /// groups, not at `n` times that. It is a plain counter, not a metric
    /// framework, and it is public because the assertion belongs in an
    /// integration test that only sees the crate's public surface.
    pub fn builds(&self) -> u64 {
        self.builds
    }
}

#[cfg(test)]
mod tests {
    //! The policy on its own, with `L = u32` — no surface, no painter, no
    //! pixels. If one of these needs a painter to express, the split this
    //! module exists for has moved.

    use super::*;

    fn group(start: u32, end: u32, alpha: f32) -> GroupComposite {
        GroupComposite { start, end, alpha }
    }

    /// A frame that changed nothing keeps the composite.
    #[test]
    fn a_clean_commit_keeps_the_composite() {
        let g = group(2, 6, 0.5);
        let mut cache = GroupCache::new();
        cache.begin_frame(&[g], None);
        cache.store(&g, 7u32);

        cache.begin_frame(&[g], Some(&[]));
        assert_eq!(cache.reuse(&g), Some(&7));
        assert_eq!(cache.builds(), 1);
    }

    /// Rule 2: a dirty rect inside the range invalidates.
    #[test]
    fn a_dirty_rect_inside_the_range_invalidates() {
        let g = group(2, 6, 0.5);
        let mut cache = GroupCache::new();
        cache.begin_frame(&[g], None);
        cache.store(&g, 7u32);

        cache.begin_frame(&[g], Some(&[4]));
        assert_eq!(cache.reuse(&g), None);
    }

    /// The range is half-open: `end` is one past the last covered rect, so
    /// a dirty rect at `end` belongs to whatever follows the group.
    #[test]
    fn the_range_is_half_open_at_both_ends() {
        let g = group(2, 6, 0.5);
        for outside in [0u32, 1, 6, 9] {
            let mut cache = GroupCache::new();
            cache.begin_frame(&[g], None);
            cache.store(&g, 7u32);
            cache.begin_frame(&[g], Some(&[outside]));
            assert_eq!(
                cache.reuse(&g),
                Some(&7),
                "rect {outside} is outside [2, 6) and must not invalidate"
            );
        }
        for inside in [2u32, 3, 4, 5] {
            let mut cache = GroupCache::new();
            cache.begin_frame(&[g], None);
            cache.store(&g, 7u32);
            cache.begin_frame(&[g], Some(&[inside]));
            assert_eq!(
                cache.reuse(&g),
                None,
                "rect {inside} is inside [2, 6) and must invalidate"
            );
        }
    }

    /// Rule 1: no dirty information means no reuse.
    #[test]
    fn a_frame_without_dirty_information_invalidates_everything() {
        let g = group(2, 6, 0.5);
        let mut cache = GroupCache::new();
        cache.begin_frame(&[g], None);
        cache.store(&g, 7u32);

        cache.begin_frame(&[g], None);
        assert_eq!(cache.reuse(&g), None);
    }

    /// Rule 3: a group whose range moved is a different composite, even
    /// when no dirty index falls in the old range.
    #[test]
    fn a_moved_range_is_a_different_composite() {
        let before = group(2, 6, 0.5);
        let after = group(2, 7, 0.5);
        let mut cache = GroupCache::new();
        cache.begin_frame(&[before], None);
        cache.store(&before, 7u32);

        cache.begin_frame(&[after], Some(&[]));
        assert_eq!(cache.reuse(&after), None);
        assert_eq!(cache.reuse(&before), None, "the old range is gone");
    }

    /// A group that dissolved leaves nothing behind.
    #[test]
    fn a_dissolved_group_drops_its_composite() {
        let g = group(2, 6, 0.5);
        let mut cache = GroupCache::new();
        cache.begin_frame(&[g], None);
        cache.store(&g, 7u32);

        cache.begin_frame(&[], Some(&[]));
        assert_eq!(cache.reuse(&g), None);
    }

    /// Alpha is a blend parameter, not part of the composite's identity —
    /// the layer is built at full alpha either way.
    #[test]
    fn alpha_alone_does_not_invalidate() {
        let before = group(2, 6, 0.5);
        let dimmer = group(2, 6, 0.25);
        let mut cache = GroupCache::new();
        cache.begin_frame(&[before], None);
        cache.store(&before, 7u32);

        cache.begin_frame(&[dimmer], Some(&[]));
        assert_eq!(cache.reuse(&dimmer), Some(&7));
    }

    /// Nested groups are independent entries: an inner rebuild does not
    /// drop the outer entry, and the caller decides what that means.
    #[test]
    fn nested_ranges_are_independent_entries() {
        let outer = group(0, 8, 0.5);
        let inner = group(3, 5, 0.5);
        let mut cache = GroupCache::new();
        cache.begin_frame(&[outer, inner], None);
        cache.store(&outer, 1u32);
        cache.store(&inner, 2u32);

        // A rect inside the inner range is inside the outer range too, so
        // both go.
        cache.begin_frame(&[outer, inner], Some(&[4]));
        assert_eq!(cache.reuse(&outer), None);
        assert_eq!(cache.reuse(&inner), None);

        // A rect the outer range covers but the inner one does not drops
        // only the outer.
        cache.store(&outer, 1u32);
        cache.store(&inner, 2u32);
        cache.begin_frame(&[outer, inner], Some(&[6]));
        assert_eq!(cache.reuse(&outer), None);
        assert_eq!(cache.reuse(&inner), Some(&2));
    }

    /// Storing the same group twice replaces the layer rather than growing
    /// the cache, so a long-running scene does not accumulate entries.
    #[test]
    fn storing_a_group_twice_replaces_its_layer() {
        let g = group(2, 6, 0.5);
        let mut cache = GroupCache::new();
        cache.store(&g, 1u32);
        cache.store(&g, 2u32);
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.reuse(&g), Some(&2));
        assert_eq!(cache.builds(), 2);
    }

    /// The design constraint of issue #278 as a test: the policy half must
    /// stay implementable against any painter's own render-target
    /// abstraction, so it must not reach for this crate's.
    ///
    /// Checked on the source text because that is what the constraint is
    /// about — a `use skia_safe::...` line here would compile perfectly
    /// well and would be exactly the regression.
    ///
    /// Only the module's own code is scanned: the text is cut at the
    /// `cfg(test)` attribute, because this test module names the backend
    /// twice by necessity (the needle below, and this sentence). Comment
    /// lines above the cut are skipped for the same reason — the module
    /// documentation states the rule and has to name what the rule
    /// forbids.
    #[test]
    fn the_policy_names_no_painter_backend() {
        let needle = concat!("sk", "ia");
        let source = include_str!("retention.rs");
        let code = source
            .split("#[cfg(test)]")
            .next()
            .expect("split always yields a first part");
        assert!(
            code.len() < source.len(),
            "the cut point moved: this test scanned the whole file, including itself"
        );
        for line in code.lines() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            assert!(
                !line.contains(needle),
                "the retention policy must not name a painter backend, found: {line}"
            );
        }
    }
}
