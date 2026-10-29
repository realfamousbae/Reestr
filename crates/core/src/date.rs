//! Date helpers. Dates are exchanged as ISO `YYYY-MM-DD` strings (PLAN §5).

use time::Date;
use time::format_description::BorrowedFormatItem;
use time::macros::format_description;

/// The ISO calendar date format.
pub const ISO_DATE: &[BorrowedFormatItem<'static>] = format_description!("[year]-[month]-[day]");

time::serde::format_description!(pub iso, Date, ISO_DATE);

/// Parses `YYYY-MM-DD`.
pub fn parse_iso(value: &str) -> Option<Date> {
    Date::parse(value, ISO_DATE).ok()
}

/// Formats as `YYYY-MM-DD`.
pub fn format_iso(date: Date) -> String {
    // Formatting a `Date` with a date-only description cannot fail.
    date.format(ISO_DATE).unwrap_or_default()
}

/// The date as the program shows it: `DD.MM.YYYY`.
pub fn format_dotted(date: Date) -> String {
    format!(
        "{:02}.{:02}.{}",
        date.day(),
        u8::from(date.month()),
        date.year()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    #[test]
    fn dotted_like_the_screen() {
        let date = Date::from_calendar_date(2026, Month::July, 5).unwrap();
        assert_eq!(format_dotted(date), "05.07.2026");
    }

    #[test]
    fn iso_round_trip() {
        let date = Date::from_calendar_date(2026, Month::July, 5).unwrap();
        assert_eq!(format_iso(date), "2026-07-05");
        assert_eq!(parse_iso("2026-07-05"), Some(date));
        assert_eq!(parse_iso("05.07.2026"), None);
        assert_eq!(parse_iso("2026-02-30"), None);
    }
}
