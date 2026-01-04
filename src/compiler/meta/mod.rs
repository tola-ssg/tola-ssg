//! Page and asset metadata types.
//!
//! This module provides metadata types for pages and assets:
//!
//! - [`PageMeta`], [`PagePaths`], [`ContentMeta`] - Page metadata and paths
//! - [`AssetMeta`], [`AssetPaths`] - Asset metadata and paths
//! - [`Pages`] - Collection of page metadata

mod asset;
mod page;

pub use asset::{AssetMeta, AssetPaths, url_from_output_path};
pub use page::{ContentMeta, PageMeta, PagePaths, Pages};

// ============================================================================
// Constants
// ============================================================================

/// Label used to identify page metadata in Typst output.
///
/// The compiler inserts a hidden element:
/// ```typst
/// #metadata(...) <tola-meta>
/// ```
/// which gets serialized to HTML as:
/// ```html
/// <span data-label="tola-meta">{"title":"...","date":"..."}</span>
/// ```
pub const TOLA_META_LABEL: &str = "tola-meta";

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert days since Unix epoch to (year, month, day).
///
/// Uses a simplified leap year calculation that's accurate for dates
/// from 1970 to ~2100.
pub fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    // Days from year 0 to 1970-01-01 (approximate, but works for our range)
    const DAYS_TO_1970: i64 = 719_468;

    let z = days + DAYS_TO_1970;
    let era = z.div_euclid(146_097); // 400-year cycles
    let doe = z.rem_euclid(146_097) as u32; // day of era [0, 146096]

    // Year of era [0, 399]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]

    // Month calculation (March = 0)
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y, m, d)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_days_to_ymd_epoch() {
        // Unix epoch: 1970-01-01
        let (y, m, d) = days_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn test_days_to_ymd_known_date() {
        // 2025-06-15 is day 20254 since epoch
        let (y, m, d) = days_to_ymd(20254);
        assert_eq!((y, m, d), (2025, 6, 15));
    }

    #[test]
    fn test_days_to_ymd_leap_year() {
        // 2024-02-29 is day 19782 since epoch (leap year)
        let (y, m, d) = days_to_ymd(19782);
        assert_eq!((y, m, d), (2024, 2, 29));
    }

    #[test]
    fn test_days_to_ymd_year_2000() {
        // 2000-01-01 is day 10957 since epoch
        let (y, m, d) = days_to_ymd(10957);
        assert_eq!((y, m, d), (2000, 1, 1));
    }

    #[test]
    fn test_days_to_ymd_end_of_year() {
        // 2024-12-31 is day 20088 since epoch
        let (y, m, d) = days_to_ymd(20088);
        assert_eq!((y, m, d), (2024, 12, 31));
    }

    #[test]
    fn test_meta_label_constant() {
        assert_eq!(TOLA_META_LABEL, "tola-meta");
    }
}
