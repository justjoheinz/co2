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
