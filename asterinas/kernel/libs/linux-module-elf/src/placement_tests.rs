// SPDX-License-Identifier: MPL-2.0
use super::{Relative32BaseWindow, relative32_base_window};

#[test]
fn displacement_boundaries_are_inclusive() {
    let window = relative32_base_window(0, 0, 0, 4).unwrap();
    assert_eq!(window.min_base(), 0);
    assert_eq!(window.max_base(), -(i32::MIN as i64) as u64);
    assert!(window.contains(0));
    assert!(window.contains(-(i32::MIN as i64) as u64));
    assert!(!window.contains(-(i32::MIN as i64) as u64 + 1));

    let negative = relative32_base_window(0, 0, 4, 8).unwrap();
    assert_eq!(
        negative,
        Relative32BaseWindow::test_window(0, i32::MAX as u64 - 3)
    );
}

#[test]
fn signed_addend_and_u64_span_limits_are_checked() {
    let window = relative32_base_window(u64::MAX, i64::MIN, 4, 8).unwrap();
    for base in [window.min_base(), window.max_base()] {
        let displacement = i128::from(u64::MAX) + i128::from(i64::MIN) - i128::from(base) - 4;
        assert!((i128::from(i32::MIN)..=i128::from(i32::MAX)).contains(&displacement));
    }
    let top = relative32_base_window(u64::MAX, 0, 0, 1);
    assert!(top.is_none());
    assert!(relative32_base_window(u64::MAX, i64::MAX, 0, 1).is_none());
    assert!(relative32_base_window(0, i64::MIN, 0, 1).is_none());
}

#[test]
fn rejects_invalid_span_and_field() {
    assert!(relative32_base_window(1, 0, 0, 0).is_none());
    assert!(relative32_base_window(1, 0, 5, 8).is_none());
    assert!(relative32_base_window(1, 0, u64::MAX - 2, u64::MAX).is_none());
    assert!(relative32_base_window(1, 0, u64::MAX, u64::MAX).is_none());
}

#[test]
fn inclusive_intersections_and_restrictions_never_expand() {
    let a = Relative32BaseWindow::test_window(10, 20);
    let b = Relative32BaseWindow::test_window(15, 25);
    assert_eq!(
        a.intersect(b),
        Some(Relative32BaseWindow::test_window(15, 20))
    );
    assert!(
        a.intersect(Relative32BaseWindow::test_window(21, 30))
            .is_none()
    );
    assert_eq!(
        a.restrict(0, 12),
        Some(Relative32BaseWindow::test_window(10, 12))
    );
    assert!(a.restrict(21, 30).is_none());
    assert!(a.restrict(2, 1).is_none());
}

#[test]
fn alignment_power_of_two_and_extremes() {
    let w = Relative32BaseWindow::test_window(3, 17);
    assert_eq!(w.first_aligned(1), Some(3));
    assert_eq!(w.first_aligned(8), Some(8));
    assert_eq!(w.first_aligned(16), Some(16));
    assert!(w.first_aligned(32).is_none());
    assert!(w.first_aligned(0).is_none());
    assert!(w.first_aligned(3).is_none());
    assert_eq!(
        Relative32BaseWindow::test_window(u64::MAX, u64::MAX).first_aligned(1),
        Some(u64::MAX)
    );
    assert!(
        Relative32BaseWindow::test_window(u64::MAX, u64::MAX)
            .first_aligned(2)
            .is_none()
    );
}

#[test]
fn sampled_window_membership_matches_displacement_equation() {
    let cases = [
        (0, 0, 0, 16),
        (u64::MAX, i64::MIN, 4, 64),
        (1 << 40, -17, 12, 128),
        (i32::MAX as u64, i64::MIN, 4, 128),
    ];
    for (symbol, addend, place, span) in cases {
        let window = relative32_base_window(symbol, addend, place, span);
        for base in (0..=256).chain([u64::MAX - span, u64::MAX - span - (u64::MAX - span) % 17]) {
            let equation =
                i128::from(symbol) + i128::from(addend) - (i128::from(base) + i128::from(place));
            let expected = base.checked_add(span).is_some()
                && place.checked_add(4).is_some_and(|end| end <= span)
                && (i128::from(i32::MIN)..=i128::from(i32::MAX)).contains(&equation);
            assert_eq!(
                window.is_some_and(|w| w.contains(base)),
                expected,
                "case={symbol},{addend},{place},{span} base={base}"
            );
        }
    }
}
