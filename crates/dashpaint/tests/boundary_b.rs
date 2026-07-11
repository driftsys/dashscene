//! Boundary-B contract tests against hand-built fixtures (issue #3):
//! no dashscene-core, no dashbuf — dashpaint's public API only.

use dashpaint::{Color, PaintKind, PaintTable};

const RED: Color = Color {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
const HALF_BLUE: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 1.0,
    a: 0.5,
};

#[test]
fn paint_table_push_returns_sequential_indices_and_get_resolves_them() {
    let mut table = PaintTable::new();
    assert!(table.is_empty());

    let red = table.push(PaintKind::Solid { color: RED });
    let blue = table.push(PaintKind::Solid { color: HALF_BLUE });

    assert_eq!(red, 0);
    assert_eq!(blue, 1);
    assert_eq!(table.len(), 2);
    assert_eq!(table.get(red), Some(&PaintKind::Solid { color: RED }));
    assert_eq!(
        table.get(blue),
        Some(&PaintKind::Solid { color: HALF_BLUE })
    );
}

#[test]
fn paint_table_get_past_the_end_returns_none() {
    let mut table = PaintTable::new();
    table.push(PaintKind::Solid { color: RED });

    assert_eq!(table.get(1), None);
    assert_eq!(table.get(u32::MAX), None);
}
