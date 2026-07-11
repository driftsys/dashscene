//! Story #2 acceptance path: a scene built by hand through the staged
//! mutation API reads back as a resolved rect table + paint table
//! (issue #2; DESIGN_1.md §5, §7.3; SCOPE_DECISIONS.md §9).

use std::mem::{align_of, size_of};

use dashscene_core::{Color, NO_PAINT, RectEntry};

#[test]
fn committed_entries_are_blittable_plain_data() {
    // Boundary B pins the rect entry as blittable plain data:
    // x, y, w, h (f32) + paint index (u32), and the solid-fill color
    // as 4xf32 RGBA (dashbuf's Color shape).
    assert_eq!(size_of::<RectEntry>(), 20);
    assert_eq!(align_of::<RectEntry>(), 4);
    assert_eq!(size_of::<Color>(), 16);
    assert_eq!(align_of::<Color>(), 4);
    assert_eq!(NO_PAINT, u32::MAX);

    let entry = RectEntry {
        x: 1.0,
        y: 2.0,
        w: 3.0,
        h: 4.0,
        paint: 0,
    };
    let copy = entry; // Copy, not a move
    assert_eq!(entry, copy);
}
