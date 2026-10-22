// SPDX-License-Identifier: MIT OR Apache-2.0
//! Deck server capability flags (the D-V1 seam).
//!
//! The connector probes the server once per actor lifetime and derives a
//! typed feature-flag set via [`flags_for`]; every version-specific
//! behavior reads a flag, never a raw version comparison — this module's
//! table is the *only* version switch in the codebase. A version newer
//! than the newest known row behaves like the newest known row
//! (optimistic); anything older or unparseable, and every probe-failure
//! path, converges on [`DeckFeatures::CONSERVATIVE`] — safe by
//! construction, because the conservative wire forms work on every
//! measured server version.

/// The derived capability flags of a Deck server.
///
/// One field per ruled flag; additive rows extend the struct (and the
/// table in [`flags_for`]) together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeckFeatures {
    /// Native card `startdate` write support: `false` for Deck 1.17.x
    /// (create ignores the field, the full-send PUT silently drops it,
    /// reads lack the key — all measured) and for the unmeasured 1.18.x
    /// range; `true` from 1.19.0 (measured).
    pub card_startdate: bool,
}

impl DeckFeatures {
    /// The fallback every degradation path converges on: no native
    /// `startdate` dependence, so the wire forms are correct on every
    /// measured version. A failed capability probe, an unknown Deck
    /// section, and an unparseable or older-than-known version all
    /// land here.
    pub const CONSERVATIVE: Self = Self {
        card_startdate: false,
    };

    /// The measured capability-true arm (Deck ≥ 1.19.0): native
    /// `startdate` writes ride the card.
    pub const NATIVE: Self = Self {
        card_startdate: true,
    };
}

/// The known-row table's anchors: the newest measured row, and the
/// floor below which every version is unmeasured (today `1.16` and
/// older). Everything at or below the newest row that is not at it —
/// the measured-false `1.17.x` and the unmeasured `1.18.x` — maps to
/// the conservative arm.
const NEWEST_KNOWN: (u64, u64) = (1, 19);

/// Derives the feature flags for a Deck version string (`"<major>.
/// <minor>[.<patch>][…suffix]"` — patch and any suffix are tolerated
/// but ignored). Total and pure: an unparseable string or a version
/// older than the newest known row's measured-true point yields
/// [`DeckFeatures::CONSERVATIVE`]; a version at or beyond
/// [`NEWEST_KNOWN`] behaves like the newest known row.
#[must_use]
pub fn flags_for(version: &str) -> DeckFeatures {
    let Some((major, minor)) = parse_major_minor(version) else {
        return DeckFeatures::CONSERVATIVE;
    };
    if (major, minor) >= NEWEST_KNOWN {
        DeckFeatures::NATIVE
    } else {
        // Everything older — the measured-false 1.17.x rows, the
        // unmeasured 1.18.x range, and anything below — stays
        // conservative.
        DeckFeatures::CONSERVATIVE
    }
}

/// Parses the leading `"<major>.<minor>"` of a version string, ignoring
/// the patch component and any suffix. `None` when either component is
/// missing or non-numeric.
fn parse_major_minor(version: &str) -> Option<(u64, u64)> {
    let mut parts = version.split('.');
    let major = leading_digits(parts.next()?)?;
    let minor = leading_digits(parts.next()?)?;
    Some((major, minor))
}

/// The digits prefix of one version component (`"19"` → 19, `"5beta"`
/// → 5, `""`/`"beta"` → `None`).
fn leading_digits(part: &str) -> Option<u64> {
    let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn every_known_row_maps_its_measured_flags() {
        assert_eq!(flags_for("1.17.5"), DeckFeatures::CONSERVATIVE);
        assert_eq!(flags_for("1.17"), DeckFeatures::CONSERVATIVE);
        assert_eq!(flags_for("1.18.0"), DeckFeatures::CONSERVATIVE);
        assert_eq!(flags_for("1.19.0"), DeckFeatures::NATIVE);
    }

    #[test]
    fn patch_and_suffix_are_tolerated_but_ignored() {
        assert_eq!(flags_for("1.19.12"), DeckFeatures::NATIVE);
        assert_eq!(flags_for("1.20.3-beta.1"), DeckFeatures::NATIVE);
        assert_eq!(flags_for("1.17.5-rc2"), DeckFeatures::CONSERVATIVE);
    }

    #[test]
    fn a_version_newer_than_the_newest_known_row_behaves_like_it() {
        assert_eq!(flags_for("1.21.0"), DeckFeatures::NATIVE);
        assert_eq!(flags_for("2.0.0"), DeckFeatures::NATIVE);
        assert_eq!(flags_for("3.14.15"), DeckFeatures::NATIVE);
    }

    #[test]
    fn older_than_known_and_unparseable_degrade_conservatively() {
        assert_eq!(flags_for("1.16.0"), DeckFeatures::CONSERVATIVE);
        assert_eq!(flags_for("1.2"), DeckFeatures::CONSERVATIVE);
        assert_eq!(flags_for("0.8.4"), DeckFeatures::CONSERVATIVE);
        for unparseable in ["", "deck", "x.y.z", "1", "1.", ".9", "1.beta", "latest"] {
            assert_eq!(
                flags_for(unparseable),
                DeckFeatures::CONSERVATIVE,
                "{unparseable:?} must degrade conservatively"
            );
        }
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn newest_known_wins_for_arbitrary_newer_versions(
            major in 1_u64..,
            minor in 19_u64..,
            patch in 0_u64..1_000,
        ) {
            assert_eq!(flags_for(&format!("{major}.{minor}.{patch}")), DeckFeatures::NATIVE);
        }

        #[test]
        fn conservative_is_the_floor_for_arbitrary_garbage(raw in "\\PC{0,32}") {
            let flags = flags_for(&raw);
            if flags == DeckFeatures::NATIVE {
                let (major, minor) = parse_major_minor(&raw).unwrap_or((0, 0));
                prop_assert!((major, minor) >= NEWEST_KNOWN, "{raw:?} claimed NATIVE");
            } else {
                // Totality: a garbage input never claims a capability.
                prop_assert_eq!(flags, DeckFeatures::CONSERVATIVE);
            }
        }
    }
}
