// SPDX-License-Identifier: AGPL-3.0-only

use super::Adapter;

/// The banner row at `index` (D2: the index resolves against the
/// refresh pass's row table).
pub(in crate::adapter) fn banner_row(
    adapter: &Adapter,
    index: i32,
) -> Option<crate::presentation::BannerRow> {
    adapter
        .tables
        .banner
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
}
