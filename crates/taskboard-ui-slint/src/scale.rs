// SPDX-License-Identifier: AGPL-3.0-only
//! The `[ui] scale` quantity: a finite `f64` inside the inclusive
//! `MIN..=MAX` window, minted app-side at the config boundary and at
//! the settings `SetScale` request. Lives in the UI crate (the
//! `keymap` precedent: typed values built app-side whose only consumer
//! is the presentation layer) — the domain crate stays
//! presentation-free.

/// The `[ui] scale` quantity: finite and within `MIN..=MAX`, minted
/// app-side at the config boundary and at the settings `SetScale`
/// request; carried in [`crate::UiSettings`] to the mount seam. Not
/// serde — the config's `UiSection.scale` stays the raw `f64`
/// deserialization carrier; the mint sits post-decode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiScale(f64);

/// Why a `[ui] scale` value was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UiScaleError {
    /// NaN or ±inf — fails closed before any range comparison.
    #[error("ui scale not finite")]
    NonFinite,
    /// Outside the inclusive `MIN..=MAX` window.
    #[error("ui scale out of range")]
    OutOfRange,
}

impl UiScale {
    /// `[ui] scale`'s enforced floor, inclusive (the design's "clamped
    /// 0.5–3.0", enforced — never silently clamped).
    pub const MIN: f64 = 0.5;
    /// See [`Self::MIN`] — the enforced ceiling, inclusive.
    pub const MAX: f64 = 3.0;

    /// The mint: finite first (NaN compares false both ways), then the
    /// inclusive range.
    ///
    /// # Errors
    ///
    /// [`UiScaleError::NonFinite`] for NaN/±inf,
    /// [`UiScaleError::OutOfRange`] outside `MIN..=MAX`.
    pub fn new(raw: f64) -> Result<Self, UiScaleError> {
        if !raw.is_finite() {
            return Err(UiScaleError::NonFinite);
        }
        if !(Self::MIN..=Self::MAX).contains(&raw) {
            return Err(UiScaleError::OutOfRange);
        }
        Ok(Self(raw))
    }

    /// The quantity as `f64` (TOML persistence, layout arithmetic).
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// Slint `float` properties are `f32`; the validated `0.5–3.0`
    /// window converts with precision far below any visible scale
    /// step. The one `as f32` cast in the crate.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn get_f32(self) -> f32 {
        self.0 as f32
    }
}

impl Default for UiScale {
    fn default() -> Self {
        Self(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_values_are_accepted() {
        assert_eq!(UiScale::new(UiScale::MIN), Ok(UiScale(UiScale::MIN)));
        assert_eq!(UiScale::new(UiScale::MAX), Ok(UiScale(UiScale::MAX)));
        assert_eq!(UiScale::new(1.0), Ok(UiScale(1.0)));
    }

    #[test]
    fn rejects_with_the_variant_split() {
        for out in [0.49, 3.01] {
            assert!(matches!(UiScale::new(out), Err(UiScaleError::OutOfRange)));
        }
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(matches!(UiScale::new(bad), Err(UiScaleError::NonFinite)));
        }
    }

    #[test]
    fn identity_and_default() {
        let scale = UiScale::new(1.25).unwrap();
        assert_eq!(scale.get(), 1.25);
        assert_eq!(scale.get_f32(), 1.25_f32);
        let copied = scale;
        assert_eq!(scale, copied, "Copy semantics");
        assert_eq!(UiScale::default(), UiScale::new(1.0).unwrap());
    }

    #[test]
    fn consts_pin_the_enforced_window() {
        assert_eq!(UiScale::MIN, 0.5);
        assert_eq!(UiScale::MAX, 3.0);
    }
}
