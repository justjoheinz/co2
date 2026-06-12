use std::fmt;
use std::str::FromStr;
use chrono::{Datelike, Local};

/// A year-month value parsed from `YYYY` or `YYYY-MM`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct YearMonth {
    pub year: i32,
    pub month: u32,
}

impl YearMonth {
    pub fn current() -> Self {
        let now = Local::now();
        Self { year: now.year(), month: now.month() }
    }

    /// First second of this month as a Unix timestamp.
    pub fn start_timestamp(self) -> i64 {
        chrono::NaiveDate::from_ymd_opt(self.year, self.month, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp()
    }

    /// First second of the month *after* this one (exclusive end for API range).
    pub fn end_timestamp(self) -> i64 {
        let (year, month) = if self.month == 12 {
            (self.year + 1, 1)
        } else {
            (self.year, self.month + 1)
        };
        chrono::NaiveDate::from_ymd_opt(year, month, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp()
    }
}

impl fmt::Display for YearMonth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{:02}", self.year, self.month)
    }
}

fn parse_year_month(s: &str, default_month: u32) -> Result<YearMonth, String> {
    match s.len() {
        4 => {
            let year = s.parse::<i32>().map_err(|_| format!("invalid year: {s}"))?;
            Ok(YearMonth { year, month: default_month })
        }
        7 if s.as_bytes()[4] == b'-' => {
            let year = s[..4].parse::<i32>().map_err(|_| format!("invalid year in: {s}"))?;
            let month = s[5..].parse::<u32>().map_err(|_| format!("invalid month in: {s}"))?;
            if !(1..=12).contains(&month) {
                return Err(format!("month must be 01–12, got: {s}"));
            }
            Ok(YearMonth { year, month })
        }
        _ => Err(format!("expected YYYY or YYYY-MM, got: {s}")),
    }
}

/// Parses `--from`: bare YYYY expands to January.
impl FromStr for YearMonth {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_year_month(s, 1)
    }
}

/// Wrapper for `--to`: bare YYYY expands to December.
#[derive(Debug, Clone, Copy)]
pub struct YearMonthEnd(pub YearMonth);

impl FromStr for YearMonthEnd {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_year_month(s, 12).map(YearMonthEnd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── YearMonth parsing ─────────────────────────────────────────────────────

    #[test]
    fn yyyy_parses_as_january() {
        let ym: YearMonth = "2024".parse().unwrap();
        assert_eq!(ym, YearMonth { year: 2024, month: 1 });
    }

    #[test]
    fn yyyy_mm_parses_explicit_month() {
        let ym: YearMonth = "2024-06".parse().unwrap();
        assert_eq!(ym, YearMonth { year: 2024, month: 6 });
    }

    #[test]
    fn rejects_month_out_of_range() {
        assert!("2024-00".parse::<YearMonth>().is_err());
        assert!("2024-13".parse::<YearMonth>().is_err());
        assert!("2024-99".parse::<YearMonth>().is_err());
    }

    #[test]
    fn rejects_malformed_input() {
        assert!("".parse::<YearMonth>().is_err());
        assert!("20".parse::<YearMonth>().is_err());
        assert!("2024-1".parse::<YearMonth>().is_err());
        assert!("2024-001".parse::<YearMonth>().is_err());
        assert!("2024/01".parse::<YearMonth>().is_err());
        assert!("abcd".parse::<YearMonth>().is_err());
        assert!("abcd-ef".parse::<YearMonth>().is_err());
    }

    // ── YearMonthEnd parsing ──────────────────────────────────────────────────

    #[test]
    fn end_yyyy_parses_as_december() {
        let yme: YearMonthEnd = "2024".parse().unwrap();
        assert_eq!(yme.0, YearMonth { year: 2024, month: 12 });
    }

    #[test]
    fn end_yyyy_mm_parses_explicit_month() {
        let yme: YearMonthEnd = "2024-03".parse().unwrap();
        assert_eq!(yme.0, YearMonth { year: 2024, month: 3 });
    }

    #[test]
    fn end_rejects_invalid_month() {
        assert!("2024-13".parse::<YearMonthEnd>().is_err());
    }

    // ── Display ───────────────────────────────────────────────────────────────

    #[test]
    fn display_zero_pads_month() {
        assert_eq!(
            YearMonth { year: 2024, month: 3 }.to_string(),
            "2024-03"
        );
        assert_eq!(
            YearMonth { year: 2024, month: 12 }.to_string(),
            "2024-12"
        );
    }

    // ── Ordering ──────────────────────────────────────────────────────────────

    #[test]
    fn ordering_is_chronological() {
        let jan_2024 = YearMonth { year: 2024, month: 1 };
        let dec_2024 = YearMonth { year: 2024, month: 12 };
        let jan_2025 = YearMonth { year: 2025, month: 1 };
        assert!(jan_2024 < dec_2024);
        assert!(dec_2024 < jan_2025);
        assert!(jan_2024 < jan_2025);
    }

    // ── start_timestamp / end_timestamp ───────────────────────────────────────

    #[test]
    fn start_timestamp_is_first_second_of_month_utc() {
        // 2024-01-01 00:00:00 UTC
        assert_eq!(
            YearMonth { year: 2024, month: 1 }.start_timestamp(),
            1_704_067_200
        );
    }

    #[test]
    fn end_timestamp_is_first_second_of_next_month_utc() {
        // First second of 2024-07-01 UTC
        assert_eq!(
            YearMonth { year: 2024, month: 6 }.end_timestamp(),
            1_719_792_000
        );
    }

    #[test]
    fn end_timestamp_rolls_year_over_in_december() {
        // First second of 2025-01-01 UTC
        assert_eq!(
            YearMonth { year: 2024, month: 12 }.end_timestamp(),
            1_735_689_600
        );
    }

    #[test]
    fn end_timestamp_strictly_after_start_timestamp() {
        for month in 1..=12 {
            let ym = YearMonth { year: 2024, month };
            assert!(ym.end_timestamp() > ym.start_timestamp(), "month {month}");
        }
    }
}
