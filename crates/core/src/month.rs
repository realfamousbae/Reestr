//! A calendar month — the unit of the journal (one sheet `MM.YY` in the source workbook).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::Date;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Month {
    year: i32,
    month: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid month: {0:?}")]
pub struct InvalidMonth(pub String);

impl Month {
    pub fn new(year: i32, month: u8) -> Result<Self, InvalidMonth> {
        if (1..=12).contains(&month) && (1900..=9999).contains(&year) {
            Ok(Self { year, month })
        } else {
            Err(InvalidMonth(format!("{year}-{month}")))
        }
    }

    pub const fn year(self) -> i32 {
        self.year
    }

    pub const fn month(self) -> u8 {
        self.month
    }

    pub fn of_date(date: Date) -> Self {
        Self {
            year: date.year(),
            month: u8::from(date.month()),
        }
    }

    /// Parses a sheet name of the workbook, `MM.YY`, e.g. `07.26` → 2026-07.
    pub fn parse_sheet_name(name: &str) -> Result<Self, InvalidMonth> {
        let invalid = || InvalidMonth(name.to_owned());
        let (mm, yy) = name.trim().split_once('.').ok_or_else(invalid)?;
        if mm.len() != 2 || yy.len() != 2 {
            return Err(invalid());
        }
        let month: u8 = mm.parse().map_err(|_| invalid())?;
        let yy: i32 = yy.parse().map_err(|_| invalid())?;
        Self::new(2000 + yy, month).map_err(|_| invalid())
    }

    /// The sheet/tab name, `MM.YY`.
    pub fn sheet_name(self) -> String {
        format!("{:02}.{:02}", self.month, self.year.rem_euclid(100))
    }

    pub fn next(self) -> Self {
        if self.month == 12 {
            Self {
                year: self.year + 1,
                month: 1,
            }
        } else {
            Self {
                month: self.month + 1,
                ..self
            }
        }
    }

    pub fn prev(self) -> Self {
        if self.month == 1 {
            Self {
                year: self.year - 1,
                month: 12,
            }
        } else {
            Self {
                month: self.month - 1,
                ..self
            }
        }
    }

    pub fn first_day(self) -> Date {
        // `new` guarantees a valid year and month.
        let month = time::Month::try_from(self.month).unwrap_or(time::Month::January);
        Date::from_calendar_date(self.year, month, 1).unwrap_or(Date::MIN)
    }

    pub fn contains(self, date: Date) -> bool {
        Self::of_date(date) == self
    }
}

impl fmt::Display for Month {
    /// ISO `YYYY-MM`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}", self.year, self.month)
    }
}

impl FromStr for Month {
    type Err = InvalidMonth;

    /// Parses ISO `YYYY-MM`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || InvalidMonth(s.to_owned());
        let (yyyy, mm) = s.split_once('-').ok_or_else(invalid)?;
        if yyyy.len() != 4 || mm.len() != 2 {
            return Err(invalid());
        }
        let year = yyyy.parse().map_err(|_| invalid())?;
        let month = mm.parse().map_err(|_| invalid())?;
        Self::new(year, month).map_err(|_| invalid())
    }
}

impl Serialize for Month {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Month {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_and_sheet_names() {
        let m: Month = "2026-07".parse().unwrap();
        assert_eq!(m.to_string(), "2026-07");
        assert_eq!(m.sheet_name(), "07.26");
        assert_eq!(Month::parse_sheet_name("07.26").unwrap(), m);
        assert_eq!(
            Month::parse_sheet_name("01.25").unwrap().to_string(),
            "2025-01"
        );
    }

    #[test]
    fn rejects_garbage() {
        for bad in ["2026-13", "2026-7", "26-07", "", "2026/07"] {
            assert!(bad.parse::<Month>().is_err(), "{bad}");
        }
        for bad in ["Свод", "Служебный", "7.26", "13.26", "07.2026"] {
            assert!(Month::parse_sheet_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn next_prev_wrap_years() {
        let dec: Month = "2025-12".parse().unwrap();
        assert_eq!(dec.next().to_string(), "2026-01");
        assert_eq!(dec.next().prev(), dec);
        assert!(dec < dec.next());
    }

    #[test]
    fn contains_dates() {
        let m: Month = "2026-02".parse().unwrap();
        assert_eq!(crate::date::format_iso(m.first_day()), "2026-02-01");
        assert!(m.contains(crate::date::parse_iso("2026-02-28").unwrap()));
        assert!(!m.contains(crate::date::parse_iso("2026-03-01").unwrap()));
    }

    #[test]
    fn serde_as_iso_string() {
        let m: Month = "2026-07".parse().unwrap();
        assert_eq!(serde_json::to_string(&m).unwrap(), "\"2026-07\"");
        assert_eq!(serde_json::from_str::<Month>("\"2026-07\"").unwrap(), m);
        assert!(serde_json::from_str::<Month>("\"07.26\"").is_err());
    }
}
