//! Runtime-only semantic density for explicitly opted-in native chrome.
//! Typography, icon sizes, borders, hit targets and compositor geometry are not scaled.
use super::Spacing;
use gpui::{Pixels, px};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum LayoutDensity {
    Compact,
    #[default]
    Comfortable,
}

impl LayoutDensity {
    /// Matches CyberManju DENSITY_SCALE.unit without importing its browser CSS.
    pub const fn spacing(self, step: Spacing) -> Pixels {
        let scale = match self {
            Self::Compact => 0.9,
            Self::Comfortable => 1.0,
        };
        px(step.value() * scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preserves_every_existing_native_spacing_step() {
        for step in [
            Spacing::None,
            Spacing::XXSmall,
            Spacing::XSmall,
            Spacing::Small,
            Spacing::Medium,
            Spacing::Large,
            Spacing::XLarge,
            Spacing::XXLarge,
            Spacing::XXXLarge,
        ] {
            assert_eq!(LayoutDensity::default().spacing(step), step.pixels());
        }
    }

    #[test]
    fn compact_is_bounded_monotonic_and_keeps_zero_spacing_zero() {
        let steps = [
            Spacing::None,
            Spacing::XXSmall,
            Spacing::XSmall,
            Spacing::Small,
            Spacing::Medium,
            Spacing::Large,
            Spacing::XLarge,
            Spacing::XXLarge,
            Spacing::XXXLarge,
        ];
        let mut previous = -1.0_f32;
        for step in steps {
            let actual: f32 = LayoutDensity::Compact.spacing(step).into();
            assert!(actual.is_finite() && actual >= 0.0 && actual <= step.value());
            assert!(actual > previous);
            assert!((actual - step.value() * 0.9).abs() < 0.00001);
            previous = actual;
        }
        assert_eq!(LayoutDensity::Compact.spacing(Spacing::None), px(0.0));
    }
}
