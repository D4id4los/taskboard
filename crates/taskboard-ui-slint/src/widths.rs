// SPDX-License-Identifier: AGPL-3.0-only

//! The stack-column width math (round-1 D17, the r4 D5–D7 divider
//! model): pure, single-source, proptestable — the `tooltip.rs`
//! precedent. All values are rendered slint pixels (winit-logical,
//! `Theme.base-scale` included — the space `window-w`, deltas and the
//! resolver share). Conversion to stored logical px happens at the
//! adapter seams; no slint types here (the §5.5 extraction seam).
//!
//! The model (r4 D5/decision 6): the row renders explicit widths that
//! sum to the row width. A stored width is a column's *basis* — the
//! resolver re-fits the row over the bases proportionally whenever the
//! row changes (window resize, stack add/remove), and a divider drag
//! moves exactly the two columns at the boundary.

/// The readability floor (r4 D7): a column never renders below this
/// many logical px — derived, not taste: it is the task card's own
/// min-content width (card padding 2·space-2 + done checkbox + row
/// spacing + `Theme.card-title-min-width`). A column narrower than
/// this would force the title-fill chain to overflow the card it
/// lives in.
pub const STACK_WIDTH_READABILITY_MIN_PX: u32 = 120;

/// The average glyph advance of the column header title
/// (`Typography.heading`: 16 px weight 600 ⇒ ~0.56 em for a humanist
/// sans). The content-minimum estimator's documented approximation:
/// an underestimate elides a few characters at the minimum (the
/// header clips + elides — graceful), an overestimate widens the
/// column slightly (harmless).
pub const HEADING_CHAR_WIDTH_PX: f64 = 9.0;

/// The header's fixed furniture reserve (logical px): the header's
/// inner padding 2·space-2 + the count badge (two digits) + the card
/// body's 2·space-2.
pub const HEADER_CHROME_PX: u32 = 64;

/// The fixed keyboard step (stack.wider / stack.narrower) — logical
/// px, scaled at the verb seam like every other width.
pub const STACK_WIDTH_STEP_PX: u32 = 32;

/// The content minimum (r4 D7): the header's estimated title width
/// plus chrome, floored by [`STACK_WIDTH_READABILITY_MIN_PX`]. All
/// values rendered (`× base_scale`).
#[must_use]
pub fn content_min_px(title_chars: usize, base_scale: f64) -> u32 {
    let scale = base_scale.max(0.0);
    let readability = f64::from(STACK_WIDTH_READABILITY_MIN_PX) * scale;
    #[allow(clippy::cast_precision_loss)] // a character count
    let heading = (title_chars as f64 * HEADING_CHAR_WIDTH_PX * scale).ceil()
        + f64::from(HEADER_CHROME_PX) * scale;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let px = readability.max(heading).round() as u32;
    px.max(STACK_WIDTH_READABILITY_MIN_PX)
}

/// The load-time heal (r2 D2, kept with the new floor): a persisted
/// width below the readable floor re-clamps to it. Applied once, where
/// widths load into the view state (`ViewState::seed_stack_widths`) —
/// never at reconcile and never at push (the gesture seams clamp their
/// own writes), and it never writes config back: healing reaches the
/// stored overlay only when the user next drags/steps.
#[must_use]
pub fn reclamp_width(px: u32) -> u32 {
    px.max(STACK_WIDTH_READABILITY_MIN_PX)
}

/// The row resolver (r4 D5): rendered widths satisfying Σ == `row_w`
/// with every column ≥ its minimum. Absent entries seed at the equal
/// share (`row_w / n` — a never-touched board resolves to the rendered
/// image of the old stretch-equal distribution); the seed is clamped
/// at its minimum first; then the residual is distributed
/// proportionally (largest-remainder rounding, deterministic
/// tie-break by index). When Σ minimums exceeds `row_w`, every column
/// resolves to its minimum and the row clips (decision 6's recorded
/// fallback — no scrollbar). An input already summing to `row_w` with
/// every column at or above its minimum resolves to itself unchanged —
/// the post-gesture stability property the drag anchoring rides on.
#[must_use]
pub fn resolve_row(row_w: u32, stored: &[Option<u32>], mins: &[u32]) -> Vec<u32> {
    let n = stored.len();
    if n == 0 {
        return Vec::new();
    }
    let min_at = |i: usize| mins.get(i).copied().unwrap_or(0);
    if n == 1 {
        return vec![row_w.max(min_at(0))];
    }
    let share = row_w / u32::try_from(n).unwrap_or(u32::MAX);
    let mut widths: Vec<u32> = stored
        .iter()
        .enumerate()
        .map(|(i, stored)| stored.unwrap_or(share).max(min_at(i)))
        .collect();
    fit_to_row(&mut widths, row_w, mins);
    widths
}

/// Fits `widths` so Σ == `row_w` by distributing the residual
/// proportionally (deficit takes respect the minimums — water-filling:
/// a column whose take exceeds its slack pins at its minimum and
/// drops out of the next round; each round either lands the residual
/// exactly or pins at least one column, so the loop is bounded by the
/// column count; the degenerate Σ mins > `row_w` case resolves to the
/// minimums exactly).
fn fit_to_row(widths: &mut [u32], row_w: u32, mins: &[u32]) {
    let min_at = |i: usize| mins.get(i).copied().unwrap_or(0);
    for _ in 0..=widths.len() {
        let sum: i64 = widths.iter().map(|&w| i64::from(w)).sum();
        let diff = i64::from(row_w) - sum;
        if diff == 0 {
            return;
        }
        let mut weights: Vec<i64> = if diff > 0 {
            widths.iter().map(|&w| i64::from(w)).collect()
        } else {
            // Deficit: only columns with slack (width above minimum)
            // can give, weighted by their current widths.
            widths
                .iter()
                .enumerate()
                .map(|(i, &w)| if w > min_at(i) { i64::from(w) } else { 0 })
                .collect()
        };
        if weights.iter().all(|&w| w == 0) {
            if diff < 0 {
                // No eligible column: the degenerate fallback — every
                // column at its minimum; the row clips.
                for (w, i) in widths.iter_mut().zip(0..) {
                    *w = min_at(i);
                }
                return;
            }
            // Surplus with no width signal (all-zero seeds): equal
            // split via unit weights.
            weights.resize(widths.len(), 1);
        }
        for (i, (w, share)) in widths
            .iter_mut()
            .zip(largest_remainder(diff, &weights))
            .enumerate()
        {
            let next = (i64::from(*w) + share).max(i64::from(min_at(i)));
            *w = u32::try_from(next).unwrap_or(u32::MAX);
        }
    }
    debug_assert!(
        widths.iter().map(|&w| i64::from(w)).sum::<i64>() == i64::from(row_w),
        "the fit converged within its round bound"
    );
}

/// Splits `units` over weights proportionally, largest-remainder
/// rounding, ties broken by index (deterministic). The returned parts
/// sum to `units` exactly.
fn largest_remainder(units: i64, weights: &[i64]) -> Vec<i64> {
    let n = weights.len();
    if n == 0 || units == 0 {
        return vec![0; n];
    }
    let total: i64 = weights.iter().sum();
    if total <= 0 {
        // No weight signal: equal split, same largest-remainder tail.
        let ones = vec![1i64; n];
        return largest_remainder(units, &ones);
    }
    let mut parts: Vec<(i64, i64)> = weights
        .iter()
        .map(|&w| {
            let exact = units * w;
            (exact.div_euclid(total), exact.rem_euclid(total))
        })
        .collect();
    let assigned: i64 = parts.iter().map(|(q, _)| *q).sum();
    let leftover = units - assigned;
    debug_assert!(
        usize::try_from(leftover.unsigned_abs()).unwrap_or(usize::MAX) <= n,
        "remainder bound"
    );
    // Hand the ±1 leftovers out by descending remainder, index ascending.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| parts[b].1.cmp(&parts[a].1).then(a.cmp(&b)));
    let each = if leftover > 0 { 1 } else { -1 };
    let tail = leftover.unsigned_abs();
    for &i in order.iter().take(usize::try_from(tail).unwrap_or(n)) {
        parts[i].0 += each;
    }
    parts.into_iter().map(|(q, _)| q).collect()
}

/// The divider drag (r4 D6): moves the boundary between `boundary`
/// (left) and `boundary + 1` (right) by `delta` (left takes +delta).
/// Sum- and others-exact by construction; δ clamps into
/// `[min_left − left, right − min_right]` — the boundary tracks the
/// pointer within the clamp and sticks at either minimum (the
/// Excel/Calc behaviour). Indices out of range degrade to the input.
#[must_use]
pub fn drag_boundary(widths: &[u32], boundary: usize, delta: i64, mins: &[u32]) -> Vec<u32> {
    let mut out = widths.to_vec();
    let Some(&left) = widths.get(boundary) else {
        return out;
    };
    let Some(&right) = widths.get(boundary + 1) else {
        return out;
    };
    let min_at = |i: usize| i64::from(mins.get(i).copied().unwrap_or(0));
    let lo = min_at(boundary) - i64::from(left);
    let hi = i64::from(right) - min_at(boundary + 1);
    // Within the clamp: left + d ≥ min(boundary) ≥ 0, right − d ≥ 0.
    let d = delta.clamp(lo.min(hi), lo.max(hi));
    out[boundary] = u32::try_from(i64::from(left) + d).unwrap_or(u32::MAX);
    out[boundary + 1] = u32::try_from(i64::from(right) - d).unwrap_or(u32::MAX);
    out
}

/// The keyboard sibling (r4 D8): ±`step` on column `index` against its
/// right neighbour — or its *left* neighbour when `index` is the last
/// column (growing the last column steals from the left). A board with
/// fewer than two columns has no boundary: the input returns
/// unchanged.
#[must_use]
pub fn step_column(
    widths: &[u32],
    index: usize,
    direction: i32,
    step: i64,
    mins: &[u32],
) -> Vec<u32> {
    let n = widths.len();
    if n < 2 || index >= n {
        return widths.to_vec();
    }
    let dir = i64::from(direction.signum());
    if index + 1 < n {
        drag_boundary(widths, index, dir * step, mins)
    } else {
        drag_boundary(widths, n - 2, -dir * step, mins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The r4 D7 floor: a short header ("a"/"b" — the golden fixture)
    /// floors at the readability constant — demonstrably smaller than
    /// the old 160 px floor (dispatch acceptance (b)).
    #[test]
    fn content_min_short_header_floors_below_the_old_floor() {
        let min = content_min_px(1, 1.0);
        assert_eq!(min, STACK_WIDTH_READABILITY_MIN_PX);
        assert!(min < 160, "the content minimum is smaller than today's 160");
    }

    /// A long header grows the minimum so the title never starts
    /// elided-by-minimum (the plan's 20-character example: 180 + 64).
    #[test]
    fn content_min_grows_with_the_title() {
        assert_eq!(content_min_px(20, 1.0), 20 * 9 + 64);
        assert!(content_min_px(40, 1.0) > content_min_px(20, 1.0));
        assert!(content_min_px(20, 2.0) > content_min_px(20, 1.0));
        // A 0-scale degenerate call still floors at the readability
        // constant (garbage-input floor).
        assert_eq!(content_min_px(1, 0.0), STACK_WIDTH_READABILITY_MIN_PX);
    }

    /// The resolver's enumerated cases (plan §3).
    #[test]
    fn resolve_row_table() {
        // n == 0: empty.
        assert_eq!(resolve_row(500, &[], &[]), Vec::<u32>::new());
        // n == 1: the single column takes the row, clamped at its min.
        assert_eq!(resolve_row(500, &[None], &[120]), [500]);
        assert_eq!(resolve_row(90, &[None], &[120]), [120], "clip case");
        // Identity: Σ stored == row_w resolves unchanged.
        assert_eq!(
            resolve_row(600, &[Some(200), Some(400)], &[100, 100]),
            [200, 400]
        );
        // Absent seeds: equal share.
        assert_eq!(resolve_row(900, &[None, None], &[100, 100]), [450, 450]);
        // Surplus: proportional to the bases.
        assert_eq!(
            resolve_row(1000, &[Some(200), Some(600)], &[100, 100]),
            [250, 750]
        );
        // Deficit with slack: proportional take, mins respected.
        assert_eq!(
            resolve_row(500, &[Some(400), Some(400)], &[100, 100]),
            [250, 250]
        );
        // Deficit into a minimum: the slack-poor column pins first.
        assert_eq!(
            resolve_row(300, &[Some(130), Some(470)], &[120, 120]),
            [120, 180]
        );
        // Degenerate: Σ mins > row_w — every column at its minimum.
        assert_eq!(
            resolve_row(200, &[Some(300), Some(300)], &[120, 130]),
            [120, 130]
        );
    }

    /// The divider drag's case table (plan §3).
    #[test]
    fn drag_boundary_table() {
        let mins = [120, 120, 120];
        let widths = [300, 400, 300];
        // Exact moves both directions; the third column bit-stable.
        assert_eq!(drag_boundary(&widths, 0, 50, &mins), [350, 350, 300]);
        assert_eq!(drag_boundary(&widths, 1, -80, &mins), [300, 320, 380]);
        // Clamp at the left minimum (the boundary sticks).
        assert_eq!(drag_boundary(&widths, 0, -10_000, &mins), [120, 580, 300]);
        // Clamp at the right minimum.
        assert_eq!(drag_boundary(&widths, 1, 10_000, &mins), [300, 580, 120]);
        // No-op at the exact minimum.
        let at_min = [120, 580, 300];
        assert_eq!(drag_boundary(&at_min, 0, -50, &mins), at_min);
        // Out-of-range boundaries degrade to the input.
        assert_eq!(drag_boundary(&widths, 2, 50, &mins), widths);
        assert_eq!(drag_boundary(&widths, 9, 50, &mins), widths);
    }

    /// The keyboard sibling's case table (plan §3 / r4 D8).
    #[test]
    fn step_column_table() {
        let mins = [120, 120, 120];
        let widths = [300, 400, 300];
        // Wider/narrower against the right neighbour.
        assert_eq!(step_column(&widths, 0, 1, 32, &mins), [332, 368, 300]);
        assert_eq!(step_column(&widths, 0, -1, 32, &mins), [268, 432, 300]);
        // The last column grows against its *left* neighbour.
        assert_eq!(step_column(&widths, 2, 1, 32, &mins), [300, 368, 332]);
        assert_eq!(step_column(&widths, 2, -1, 32, &mins), [300, 432, 268]);
        // Last-column equivalence with the mirror drag.
        assert_eq!(
            step_column(&widths, 2, 1, 32, &mins),
            drag_boundary(&widths, 1, -32, &mins)
        );
        // A one-column board is inert.
        assert_eq!(step_column(&[400], 0, 1, 32, &[120]), [400]);
        // Step into both minimums.
        assert_eq!(step_column(&[130, 130], 0, -1, 32, &[120, 120]), [120, 140]);
        assert_eq!(step_column(&[130, 130], 1, -1, 32, &[120, 120]), [140, 120]);
    }

    /// The load-time heal (r2 D2, carried over against the new floor).
    #[test]
    fn reclamp_width_heals_sub_floor_values() {
        let floor = STACK_WIDTH_READABILITY_MIN_PX;
        assert_eq!(reclamp_width(96), floor);
        assert_eq!(reclamp_width(0), floor);
        assert_eq!(reclamp_width(floor), floor);
        assert_eq!(reclamp_width(240), 240);
    }

    #[cfg(test)]
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        fn row_strategy() -> impl Strategy<Value = (u32, Vec<Option<u32>>, Vec<u32>)> {
            (1u32..2000, 1usize..7usize).prop_flat_map(|(row_w, n)| {
                (
                    Just(row_w),
                    proptest::collection::vec(proptest::option::of(1u32..1000), n),
                    proptest::collection::vec(1u32..200, n),
                )
            })
        }

        proptest! {
            /// The resolver's load-bearing set (plan §3): Σ == row_w
            /// whenever the minimums fit; every column ≥ its minimum;
            /// deterministic; degenerate returns the minimums.
            #[test]
            fn resolve_row_preserves_sum_mins_and_stability(
                (row_w, stored, mins) in row_strategy(),
            ) {
                let mins_sum: i64 = mins.iter().map(|&m| i64::from(m)).sum();
                let out = resolve_row(row_w, &stored, &mins);
                prop_assert_eq!(out.len(), stored.len());
                for (i, &w) in out.iter().enumerate() {
                    prop_assert!(w >= mins[i], "column {i} respects its minimum");
                }
                let sum: i64 = out.iter().map(|&w| i64::from(w)).sum();
                if mins_sum <= i64::from(row_w) {
                    prop_assert_eq!(sum, i64::from(row_w), "the row fills exactly");
                    prop_assert_eq!(resolve_row(row_w, &stored, &mins), out, "stable");
                } else {
                    prop_assert_eq!(out, mins, "degenerate resolves to the minimums");
                }
            }

            /// Surplus distribution is proportional to the bases
            /// within the ±1 rounding of the largest-remainder scheme.
            #[test]
            fn resolve_row_surplus_is_proportional(
                (row_w, stored, mins) in row_strategy(),
                extra in 1i64..2000,
            ) {
                let base = resolve_row(row_w, &stored, &mins);
                let base_sum: i64 = base.iter().map(|&w| i64::from(w)).sum();
                // The grown resolve seeds at the base (the post-gesture
                // steady state): the surplus distributes proportionally.
                let bases: Vec<Option<u32>> = base.iter().map(|&w| Some(w)).collect();
                let grown = resolve_row(
                    u32::try_from(base_sum.saturating_add(extra)).unwrap_or(u32::MAX),
                    &bases,
                    &mins,
                );
                let grown_sum: i64 = grown.iter().map(|&w| i64::from(w)).sum();
                let delta = grown_sum - base_sum;
                prop_assert!(delta > 0);
                let total_w: i64 = base.iter().map(|&w| i64::from(w)).sum();
                for (i, (&before, &after)) in base.iter().zip(&grown).enumerate() {
                    let growth = i64::from(after) - i64::from(before);
                    // |share − delta·w_i/W| ≤ 1 px of rounding, scaled by W.
                    let err = (growth * total_w - delta * i64::from(before)).abs();
                    prop_assert!(err <= total_w, "column {i} grows proportionally");
                }
            }

            /// The divider drag: Σ preserved for any δ, only the pair
            /// changes, monotone in δ, double-clamp idempotent,
            /// reversing the applied delta recovers the input.
            #[test]
            fn drag_boundary_preserves_sum_and_pair_isolation(
                widths in proptest::collection::vec(1u32..1000, 2usize..6usize),
                boundary in 0usize..6usize,
                delta in -10_000i64..10_000,
                mins in proptest::collection::vec(0u32..300, 6usize),
            ) {
                prop_assume!(boundary + 1 < widths.len());
                let clamped_mins = &mins[..widths.len()];
                // Widths respect the minimums (the resolver's guarantee).
                let widths: Vec<u32> = widths.iter().enumerate()
                    .map(|(i, &w)| w.max(clamped_mins[i])).collect();
                let out = drag_boundary(&widths, boundary, delta, clamped_mins);
                prop_assert_eq!(
                    out.iter().map(|&w| i64::from(w)).sum::<i64>(),
                    widths.iter().map(|&w| i64::from(w)).sum::<i64>(),
                    "sum preserved"
                );
                for (i, (&a, &b)) in widths.iter().zip(&out).enumerate() {
                    if i != boundary && i != boundary + 1 {
                        prop_assert_eq!(a, b, "only the pair moves");
                    }
                }
                prop_assert!(out[boundary] >= clamped_mins[boundary]);
                prop_assert!(out[boundary + 1] >= clamped_mins[boundary + 1]);
                // Monotone in δ at the boundary.
                let less = drag_boundary(
                    &widths,
                    boundary,
                    delta.saturating_sub(1),
                    clamped_mins,
                );
                prop_assert!(less[boundary] <= out[boundary]);
                // Double-clamp idempotence and symmetric recovery:
                // reversing the *applied* (clamped) delta returns the
                // exact input.
                let applied = i64::from(out[boundary]) - i64::from(widths[boundary]);
                let back = drag_boundary(&out, boundary, -applied, clamped_mins);
                prop_assert_eq!(back, widths, "reversing the applied delta recovers");
            }

            /// The keyboard sibling: last-column equivalence with the
            /// mirrored boundary drag, and the one-column identity.
            #[test]
            fn step_column_matches_the_mirror_drag(
                widths in proptest::collection::vec(1u32..1000, 2usize..6usize),
                step in 1i64..200,
                mins in proptest::collection::vec(0u32..300, 6usize),
            ) {
                let n = widths.len();
                let clamped = &mins[..n];
                let last = n - 1;
                prop_assert_eq!(
                    step_column(&widths, last, 1, step, clamped),
                    drag_boundary(&widths, last - 1, -step, clamped)
                );
                prop_assert_eq!(
                    step_column(&widths, last, -1, step, clamped),
                    drag_boundary(&widths, last - 1, step, clamped)
                );
            }

            /// A one-column board has no boundary: identity for any
            /// input.
            #[test]
            fn step_column_single_board_is_inert(w in 1u32..2000, step in 1i64..200) {
                prop_assert_eq!(step_column(&[w], 0, 1, step, &[0]), [w]);
            }

            /// The estimator: at or above the scaled floor, non-
            /// decreasing in the title length, monotone in the scale.
            #[test]
            fn content_min_is_floored_monotone_and_scaled(
                chars in 0usize..200,
                scale_a in 0.1f64..4.0,
                scale_b in 0.1f64..4.0,
            ) {
                let a = content_min_px(chars, scale_a);
                let floor = (f64::from(STACK_WIDTH_READABILITY_MIN_PX) * scale_a.max(0.0))
                    .round()
                    .min(f64::from(STACK_WIDTH_READABILITY_MIN_PX));
                prop_assert!(f64::from(a) >= floor);
                prop_assert!(content_min_px(chars + 1, scale_a) >= a);
                let hi = scale_a.max(scale_b);
                let lo = scale_a.min(scale_b);
                prop_assert!(content_min_px(chars, hi) >= content_min_px(chars, lo));
            }

            /// The heal math (r2 D2, carried over): idempotent,
            /// monotone identity above the floor, floored below it.
            #[test]
            fn reclamp_width_is_idempotent_monotone_and_floored(
                a in 0u32..5000,
                b in 0u32..5000,
            ) {
                let (lo, hi) = (a.min(b), a.max(b));
                let (r_lo, r_hi) = (reclamp_width(lo), reclamp_width(hi));
                prop_assert!(r_lo <= r_hi, "monotone: {r_lo} <= {r_hi}");
                prop_assert_eq!(reclamp_width(r_lo), r_lo, "idempotent");
                prop_assert!(r_hi >= STACK_WIDTH_READABILITY_MIN_PX);
                if lo >= STACK_WIDTH_READABILITY_MIN_PX {
                    prop_assert_eq!(r_lo, lo, "identity above the floor");
                }
            }
        }
    }
}
