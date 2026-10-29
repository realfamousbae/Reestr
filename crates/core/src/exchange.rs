//! Exchange with the phone (D39). The computer keeps the main file; the phone adds rows and
//! changes its own rows that the computer has not taken yet. Each phone row has a constant key
//! and a version number that grows with every change on the phone. The computer remembers
//! which version of each key it has taken and what the row looked like then, so a repeated
//! exchange adds nothing, a row deleted on the computer never comes back, and a row changed on
//! both sides keeps the computer's version.

use time::Date;

use crate::dict::name_key;
use crate::money::Kopecks;
use crate::month::Month;

/// The fields of an entry that travel between the phone and the computer. References are
/// names, resolved (and created when missing, D17) on the receiving side, like an import.
/// Cashback does not travel: the computer computes it by its rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneEntry {
    pub month: Month,
    pub title: String,
    pub date: Option<Date>,
    pub amount: Kopecks,
    pub category: Option<String>,
    pub participant: Option<String>,
    pub card: Option<String>,
    pub is_regular: bool,
}

impl PhoneEntry {
    /// A short digest of the content: tells whether an entry changed since it was taken.
    /// Names compare like everywhere else, without case and extra spaces.
    pub fn fingerprint(&self) -> String {
        let name = |n: &Option<String>| n.as_deref().map(name_key).unwrap_or_default();
        let content = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            self.month,
            name_key(&self.title),
            self.date.map(crate::date::format_iso).unwrap_or_default(),
            self.amount.get(),
            name(&self.category),
            name(&self.participant),
            name(&self.card),
            self.is_regular,
        );
        format!("{:016x}", crate::import::fnv1a(&content))
    }
}

/// A row made or changed on the phone that the computer may not have taken yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneChange {
    /// `phone:` and 32 hex digits; the same in every file the row reaches.
    pub key: String,
    /// Grows with every change of the row on the phone.
    pub rev: i64,
    /// `None`: the row was deleted on the phone.
    pub entry: Option<PhoneEntry>,
}

/// What the computer has taken of a key: the version and the row as it was written then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Taken {
    pub rev: i64,
    pub fingerprint: String,
}

/// What to do with a phone change on the computer's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TakeStep {
    /// This version is already taken.
    Skip,
    /// A new row: add it.
    Add,
    /// Changed on the phone and untouched on the computer: take the phone's version.
    Update,
    /// Deleted on the phone and untouched on the computer: delete it.
    Delete,
    /// Deleted on the phone and absent on the computer: only remember the version.
    Forget,
    /// Changed on both sides, or deleted on the computer: the computer's version stays.
    KeepComputer,
}

impl TakeStep {
    /// Whether the phone still has to send this change. Used on the phone when a new copy of
    /// the main file arrives: what the computer has seen is dropped, the rest moves into the
    /// new copy and waits for the next exchange.
    pub const fn still_pending(self) -> bool {
        !matches!(self, Self::Skip | Self::KeepComputer)
    }
}

/// Decides what a phone change does to the computer's file, given what the computer took of
/// this key before (`taken`) and the row with this key there now (`current`).
pub fn plan_take(
    change: &PhoneChange,
    taken: Option<&Taken>,
    current: Option<&PhoneEntry>,
) -> TakeStep {
    match taken {
        Some(t) if t.rev >= change.rev => TakeStep::Skip,
        None => match (&change.entry, current) {
            (Some(_), None) => TakeStep::Add,
            (Some(_), Some(_)) => TakeStep::Update,
            (None, None) => TakeStep::Forget,
            (None, Some(_)) => TakeStep::Delete,
        },
        Some(t) => match (&change.entry, current) {
            (None, None) => TakeStep::Forget,
            (_, None) => TakeStep::KeepComputer,
            (_, Some(row)) if row.fingerprint() != t.fingerprint => TakeStep::KeepComputer,
            (Some(_), Some(_)) => TakeStep::Update,
            (None, Some(_)) => TakeStep::Delete,
        },
    }
}

/// Keys of phone rows start with this; imported rows have `xlsx:`.
pub const PHONE_KEY_PREFIX: &str = "phone:";

#[cfg(test)]
mod tests {
    use super::*;

    fn row(title: &str, rubles: i64) -> PhoneEntry {
        PhoneEntry {
            month: "2026-11".parse().unwrap(),
            title: title.into(),
            date: crate::date::parse_iso("2026-11-03"),
            amount: Kopecks::from_rubles(rubles),
            category: Some("продукты".into()),
            participant: Some("Андрей".into()),
            card: Some("Газпром КР".into()),
            is_regular: false,
        }
    }

    fn change(rev: i64, entry: Option<PhoneEntry>) -> PhoneChange {
        PhoneChange {
            key: format!("{PHONE_KEY_PREFIX}{}", "07".repeat(16)),
            rev,
            entry,
        }
    }

    fn taken(rev: i64, entry: &PhoneEntry) -> Taken {
        Taken {
            rev,
            fingerprint: entry.fingerprint(),
        }
    }

    #[test]
    fn a_new_row_is_added_once() {
        let new = change(1, Some(row("Пятёрочка", 1200)));
        assert_eq!(plan_take(&new, None, None), TakeStep::Add);
        let as_written = row("Пятёрочка", 1200);
        let t = taken(1, &as_written);
        assert_eq!(plan_take(&new, Some(&t), Some(&as_written)), TakeStep::Skip);
    }

    #[test]
    fn a_row_deleted_on_the_computer_does_not_come_back() {
        let first = row("Пятёрочка", 1200);
        let t = taken(1, &first);
        assert_eq!(
            plan_take(&change(1, Some(first.clone())), Some(&t), None),
            TakeStep::Skip
        );
        // Changed on the phone afterwards: the computer's deletion stays.
        let step = plan_take(&change(2, Some(row("Пятёрочка", 1300))), Some(&t), None);
        assert_eq!(step, TakeStep::KeepComputer);
        assert!(!step.still_pending());
    }

    #[test]
    fn a_phone_edit_reaches_an_untouched_row() {
        let first = row("Пятёрочка", 1200);
        let t = taken(1, &first);
        let edited = change(2, Some(row("Пятёрочка", 1300)));
        assert_eq!(plan_take(&edited, Some(&t), Some(&first)), TakeStep::Update);
        assert_eq!(
            plan_take(&change(3, None), Some(&t), Some(&first)),
            TakeStep::Delete
        );
    }

    #[test]
    fn the_computer_wins_when_both_changed() {
        let first = row("Пятёрочка", 1200);
        let t = taken(1, &first);
        let on_computer = PhoneEntry {
            category: Some("еда".into()),
            ..first
        };
        let edited = change(2, Some(row("Пятёрочка", 1300)));
        assert_eq!(
            plan_take(&edited, Some(&t), Some(&on_computer)),
            TakeStep::KeepComputer
        );
        assert_eq!(
            plan_take(&change(2, None), Some(&t), Some(&on_computer)),
            TakeStep::KeepComputer
        );
    }

    #[test]
    fn deletions_of_rows_never_taken_are_remembered_and_dropped() {
        let deleted = change(2, None);
        let step = plan_take(&deleted, None, None);
        assert_eq!(step, TakeStep::Forget);
        // The phone keeps sending it until the computer has seen this version.
        assert!(step.still_pending());
        let seen = Taken {
            rev: 2,
            fingerprint: String::new(),
        };
        assert_eq!(plan_take(&deleted, Some(&seen), None), TakeStep::Skip);
    }

    #[test]
    fn fingerprints_ignore_case_and_spaces_of_names() {
        let a = row("Пятёрочка", 1200);
        let b = PhoneEntry {
            title: "  пятерочка ".into(),
            card: Some("газпром кр".into()),
            ..a.clone()
        };
        assert_eq!(a.fingerprint(), b.fingerprint());
        let c = PhoneEntry {
            amount: Kopecks(120_001),
            ..a.clone()
        };
        assert_ne!(a.fingerprint(), c.fingerprint());
        assert_eq!(a.fingerprint().len(), 16);
    }
}
