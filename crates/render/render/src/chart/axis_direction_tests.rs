#![allow(clippy::unwrap_used)]

use super::{Axis, AxisKind, Line, axis_line};
use kition_slide_color::Rgba;
use kition_slide_model::style::{ColorRef, LineEnd};

fn axis(kind: AxisKind, reversed: bool) -> Axis {
    Axis {
        kind,
        reversed,
        line: Some(Line {
            head: Some(LineEnd { kind: "diamond".into(), w: "sm".into(), len: "lg".into() }),
            tail: Some(LineEnd { kind: "triangle".into(), w: "lg".into(), len: "sm".into() }),
            ..Line::solid(ColorRef::rgb(Rgba::BLACK), 1.75)
        }),
        ..Default::default()
    }
}

fn check(axis: &Axis, along_x: bool, first_kind: &str, last_kind: &str) {
    let line = axis_line(axis, along_x).unwrap();
    assert_eq!(line.head.as_ref().unwrap().kind, first_kind);
    assert_eq!(line.tail.as_ref().unwrap().kind, last_kind);
    assert_eq!(line.width, Some(1.75));
    assert_eq!(axis.line.as_ref().unwrap().head.as_ref().unwrap().kind, "diamond", "the source axis remains immutable");
}

#[test]
fn value_axes_attach_tail_to_maximum_in_both_orientations() {
    check(&axis(AxisKind::Value, false), true, "diamond", "triangle");
    check(&axis(AxisKind::Value, false), false, "triangle", "diamond");
    check(&axis(AxisKind::Value, true), true, "triangle", "diamond");
    check(&axis(AxisKind::Value, true), false, "diamond", "triangle");
}

#[test]
fn category_axes_follow_index_order_in_columns_and_horizontal_bars() {
    // Columns: category indices run left to right unless reversed.
    check(&axis(AxisKind::Category, false), true, "diamond", "triangle");
    check(&axis(AxisKind::Category, true), true, "triangle", "diamond");
    // Horizontal bars: category indices run bottom to top unless reversed.
    check(&axis(AxisKind::Category, false), false, "triangle", "diamond");
    check(&axis(AxisKind::Category, true), false, "diamond", "triangle");
}

#[test]
fn secondary_value_axes_use_their_own_orientation() {
    let primary = axis(AxisKind::Value, false);
    let secondary = axis(AxisKind::Value, true);
    check(&primary, false, "triangle", "diamond");
    check(&secondary, false, "diamond", "triangle");
}

#[test]
fn absent_and_undecorated_axes_do_not_gain_or_copy_a_line() {
    let mut axis = Axis::default();
    assert!(axis_line(&axis, false).is_none());
    axis.line = Some(Line::solid(ColorRef::rgb(Rgba::BLACK), 1.75));
    assert!(matches!(axis_line(&axis, false), Some(std::borrow::Cow::Borrowed(_))));
}
