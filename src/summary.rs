use std::collections::BTreeMap;

use crate::aws::EmissionsResult;

/// Aggregates results into a sorted map by an arbitrary key, summing a value.
pub fn sum_by<K, F, G>(results: &[EmissionsResult], key: F, val: G) -> BTreeMap<K, f64>
where
    K: Ord,
    F: Fn(&EmissionsResult) -> K,
    G: Fn(&EmissionsResult) -> f64,
{
    let mut map: BTreeMap<K, f64> = BTreeMap::new();
    for r in results {
        *map.entry(key(r)).or_default() += val(r);
    }
    map
}

/// Returns the top-`n` entries from `map` sorted by value descending.
pub fn top_n(map: &BTreeMap<String, f64>, n: usize) -> Vec<(String, f64)> {
    let mut v: Vec<_> = map.iter().map(|(k, &v)| (k.clone(), v)).collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    v.truncate(n);
    v
}

pub fn fmt_co2(v: f64) -> String {
    format!("{:.2}", v)
}

/// Renders `v` so it fits in `width` characters; returns empty string if it
/// cannot fit any meaningful representation (or if `v` is 0).
pub fn fmt_cell(v: f64, width: usize) -> String {
    if v == 0.0 {
        return String::new();
    }
    for s in [
        format!("{:.2}", v),
        format!("{:.1}", v),
        format!("{:.0}", v),
    ] {
        if s.len() <= width {
            return s;
        }
    }
    String::new()
}

pub fn fmt_pct(v: f64, total: f64) -> String {
    if total == 0.0 {
        "  0.0%".to_string()
    } else {
        format!("{:.1}%", v / total * 100.0)
    }
}

/// Derives a title from the year range covered by `results`, falling back to
/// `fallback` when the range cannot be determined.
pub fn year_range_title(results: &[EmissionsResult], fallback: &str) -> String {
    let mut months: Vec<&str> = results.iter().map(|r| r.month.as_str()).collect();
    months.sort();
    months.dedup();
    match (months.first(), months.last()) {
        (Some(first), Some(last)) if first.len() >= 4 && last.len() >= 4 => {
            let first_year = &first[..4];
            let last_year = &last[..4];
            if first_year == last_year {
                first_year.to_string()
            } else {
                format!("{first_year}–{last_year}")
            }
        }
        _ => fallback.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aws::EmissionsResult;

    fn rec(month: &str, region: &str, service: &str, lbm: f64, mbm: f64) -> EmissionsResult {
        EmissionsResult {
            month: month.to_string(),
            region: region.to_string(),
            service: service.to_string(),
            lbm,
            mbm,
        }
    }

    // ── sum_by ────────────────────────────────────────────────────────────────

    #[test]
    fn sum_by_aggregates_values_under_same_key() {
        let results = vec![
            rec("2024-01", "us-east-1", "EC2", 1.0, 0.2),
            rec("2024-01", "us-east-1", "S3", 0.5, 0.1),
            rec("2024-02", "us-east-1", "EC2", 2.0, 0.4),
        ];
        let by_month = sum_by(&results, |r| r.month.clone(), |r| r.lbm);
        assert_eq!(by_month.len(), 2);
        assert_eq!(by_month.get("2024-01"), Some(&1.5));
        assert_eq!(by_month.get("2024-02"), Some(&2.0));
    }

    #[test]
    fn sum_by_returns_keys_in_sorted_order() {
        let results = vec![
            rec("2024-03", "x", "y", 1.0, 0.0),
            rec("2024-01", "x", "y", 1.0, 0.0),
            rec("2024-02", "x", "y", 1.0, 0.0),
        ];
        let by_month = sum_by(&results, |r| r.month.clone(), |r| r.lbm);
        let keys: Vec<_> = by_month.keys().map(String::as_str).collect();
        assert_eq!(keys, ["2024-01", "2024-02", "2024-03"]);
    }

    #[test]
    fn sum_by_with_tuple_key() {
        let results = vec![
            rec("2024-01", "us-east-1", "EC2", 1.0, 0.0),
            rec("2024-01", "eu-west-1", "EC2", 2.0, 0.0),
            rec("2024-01", "us-east-1", "EC2", 3.0, 0.0),
        ];
        let by_region_service =
            sum_by(&results, |r| (r.region.clone(), r.service.clone()), |r| r.lbm);
        assert_eq!(
            by_region_service.get(&("us-east-1".to_string(), "EC2".to_string())),
            Some(&4.0)
        );
        assert_eq!(
            by_region_service.get(&("eu-west-1".to_string(), "EC2".to_string())),
            Some(&2.0)
        );
    }

    #[test]
    fn sum_by_on_empty_input_returns_empty_map() {
        let results: Vec<EmissionsResult> = vec![];
        let by_month = sum_by(&results, |r| r.month.clone(), |r| r.lbm);
        assert!(by_month.is_empty());
    }

    // ── top_n ─────────────────────────────────────────────────────────────────

    #[test]
    fn top_n_returns_largest_entries_descending() {
        let mut map = BTreeMap::new();
        map.insert("a".to_string(), 1.0);
        map.insert("b".to_string(), 3.0);
        map.insert("c".to_string(), 2.0);
        let top = top_n(&map, 2);
        assert_eq!(top, vec![("b".to_string(), 3.0), ("c".to_string(), 2.0)]);
    }

    #[test]
    fn top_n_returns_all_when_n_exceeds_size() {
        let mut map = BTreeMap::new();
        map.insert("a".to_string(), 1.0);
        map.insert("b".to_string(), 2.0);
        let top = top_n(&map, 10);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0], ("b".to_string(), 2.0));
    }

    #[test]
    fn top_n_zero_returns_empty() {
        let mut map = BTreeMap::new();
        map.insert("a".to_string(), 1.0);
        assert!(top_n(&map, 0).is_empty());
    }

    #[test]
    fn top_n_empty_map_returns_empty() {
        let map = BTreeMap::<String, f64>::new();
        assert!(top_n(&map, 5).is_empty());
    }

    // ── fmt_co2 ───────────────────────────────────────────────────────────────

    #[test]
    fn fmt_co2_formats_to_two_decimals() {
        assert_eq!(fmt_co2(1.234), "1.23");
        assert_eq!(fmt_co2(0.0), "0.00");
        assert_eq!(fmt_co2(12.0), "12.00");
    }

    // ── fmt_cell ──────────────────────────────────────────────────────────────

    #[test]
    fn fmt_cell_zero_returns_empty_string() {
        assert_eq!(fmt_cell(0.0, 10), "");
    }

    #[test]
    fn fmt_cell_uses_two_decimals_when_room_allows() {
        assert_eq!(fmt_cell(1.234, 5), "1.23");
        assert_eq!(fmt_cell(1.234, 4), "1.23");
    }

    #[test]
    fn fmt_cell_degrades_to_one_decimal() {
        assert_eq!(fmt_cell(1.234, 3), "1.2");
    }

    #[test]
    fn fmt_cell_degrades_to_integer() {
        assert_eq!(fmt_cell(15.678, 3), "16");
    }

    #[test]
    fn fmt_cell_returns_empty_when_nothing_fits() {
        assert_eq!(fmt_cell(1234.5, 3), "");
    }

    // ── fmt_pct ───────────────────────────────────────────────────────────────

    #[test]
    fn fmt_pct_with_zero_total_returns_zero_placeholder() {
        assert_eq!(fmt_pct(1.0, 0.0), "  0.0%");
        assert_eq!(fmt_pct(0.0, 0.0), "  0.0%");
    }

    #[test]
    fn fmt_pct_computes_fraction() {
        assert_eq!(fmt_pct(0.5, 1.0), "50.0%");
        assert_eq!(fmt_pct(0.25, 1.0), "25.0%");
        assert_eq!(fmt_pct(0.1, 1.0), "10.0%");
    }

    // ── year_range_title ──────────────────────────────────────────────────────

    #[test]
    fn year_range_title_collapses_single_year() {
        let results = vec![
            rec("2024-01", "us-east-1", "EC2", 1.0, 0.0),
            rec("2024-06", "us-east-1", "EC2", 1.0, 0.0),
            rec("2024-12", "us-east-1", "EC2", 1.0, 0.0),
        ];
        assert_eq!(year_range_title(&results, "fallback"), "2024");
    }

    #[test]
    fn year_range_title_spans_multiple_years() {
        let results = vec![
            rec("2024-06", "us-east-1", "EC2", 1.0, 0.0),
            rec("2025-02", "us-east-1", "EC2", 1.0, 0.0),
        ];
        assert_eq!(year_range_title(&results, "fallback"), "2024\u{2013}2025");
    }

    #[test]
    fn year_range_title_uses_fallback_for_empty_results() {
        let results: Vec<EmissionsResult> = vec![];
        assert_eq!(year_range_title(&results, "fallback"), "fallback");
    }

    #[test]
    fn year_range_title_uses_fallback_for_malformed_month() {
        let results = vec![rec("abc", "us-east-1", "EC2", 1.0, 0.0)];
        assert_eq!(year_range_title(&results, "fallback"), "fallback");
    }
}
