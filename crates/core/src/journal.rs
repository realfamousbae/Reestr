//! Journal entries (one row of a month sheet) and how edits apply to them.

use serde::{Deserialize, Serialize};
use time::Date;

use crate::cashback::{Cashback, CashbackSource, RateBp, Rounding};
use crate::dict::{CategoryId, ParticipantId, ProductId};
use crate::mcc::{InvalidMcc, Mcc};
use crate::money::Kopecks;
use crate::month::Month;
use crate::products::GraceId;

pub type EntryId = i64;

/// A spending entry. Columns of a month sheet, in order:
/// `№ | Наименование | Назначение | Регулярная | Участник | Дата | Сумма | % | Кэшбэк | Карта |
/// MCC`. `№` is the row position and is not stored here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: EntryId,
    pub month: Month,
    /// «Наименование».
    pub title: String,
    /// «Назначение».
    pub category_id: Option<CategoryId>,
    /// «Регулярная».
    pub is_regular: bool,
    /// «Участник».
    pub participant_id: Option<ParticipantId>,
    /// «Дата». May lie outside `month`: late records are allowed.
    #[serde(with = "crate::date::iso::option")]
    pub date: Option<Date>,
    /// «Сумма».
    pub amount: Kopecks,
    /// «%» and «Кэшбэк».
    pub cashback: Option<Cashback>,
    /// «Карта».
    pub product_id: Option<ProductId>,
    /// A credit card purchase moved to a tranche by hand; `None` binds it by its date (D28).
    pub grace_period_id: Option<GraceId>,
    /// «MCC»: chooses the rule before the category does (D20).
    pub mcc: Option<Mcc>,
}

/// A change of one field, with dictionary references already resolved to ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryEdit {
    Title(String),
    Category(Option<CategoryId>),
    IsRegular(bool),
    Participant(Option<ParticipantId>),
    Date(Option<Date>),
    Amount(Kopecks),
    /// A typed percentage: cashback is computed from it.
    Rate(Option<RateBp>),
    /// A typed cashback amount: the percentage becomes derived.
    Cashback(Option<Kopecks>),
    Product(Option<ProductId>),
    /// Binds a credit card purchase to a tranche by hand; `None` returns it to the binding by
    /// date.
    Tranche(Option<GraceId>),
    Mcc(Option<Mcc>),
}

/// A change of one field as the UI sends it: dictionary values are names, which storage
/// resolves to ids (creating missing items, D17).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum EditInput {
    Title(String),
    Category(Option<String>),
    IsRegular(bool),
    Participant(Option<String>),
    Date(#[serde(with = "crate::date::iso::option")] Option<Date>),
    Amount(Kopecks),
    Rate(Option<RateBp>),
    Cashback(Option<Kopecks>),
    Card(Option<String>),
    /// As typed; checked by [`crate::mcc::parse_mcc`].
    Mcc(Option<String>),
}

/// One operation of a batch. A paste or a fill produces many at once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum JournalOp {
    Edit {
        id: EntryId,
        edit: EditInput,
    },
    /// Appends a new row at the bottom of the month and applies `edits` to it.
    Append {
        edits: Vec<EditInput>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("rate {0} bp is above 100%")]
    RateTooHigh(u32),
    #[error("entry {0} not found")]
    NotFound(EntryId),
    #[error(transparent)]
    InvalidMcc(#[from] InvalidMcc),
}

impl Entry {
    pub fn new(id: EntryId, month: Month) -> Self {
        Self {
            id,
            month,
            title: String::new(),
            category_id: None,
            is_regular: false,
            participant_id: None,
            date: None,
            amount: Kopecks::ZERO,
            cashback: None,
            product_id: None,
            grace_period_id: None,
            mcc: None,
        }
    }

    /// Applies one edit. Call [`Entry::refresh_cashback`] afterwards: a rate-based cashback
    /// depends on the amount and on the rounding of the entry's product.
    pub fn apply(&mut self, edit: EntryEdit) -> Result<(), EditError> {
        match edit {
            EntryEdit::Title(title) => self.title = title.trim().to_owned(),
            EntryEdit::Category(id) => self.category_id = id,
            EntryEdit::IsRegular(value) => self.is_regular = value,
            EntryEdit::Participant(id) => self.participant_id = id,
            EntryEdit::Date(date) => self.date = date,
            EntryEdit::Amount(amount) => self.amount = amount,
            EntryEdit::Rate(None) | EntryEdit::Cashback(None) => self.cashback = None,
            EntryEdit::Rate(Some(rate)) => {
                if rate > RateBp::MAX {
                    return Err(EditError::RateTooHigh(rate.0));
                }
                self.cashback = Some(Cashback {
                    amount: Kopecks::ZERO,
                    rate: Some(rate),
                    source: CashbackSource::ManualPercent,
                });
            }
            EntryEdit::Cashback(Some(amount)) => {
                self.cashback = Some(Cashback::manual_amount(amount));
            }
            EntryEdit::Product(id) => {
                // A hand-picked tranche belongs to the old card.
                if id != self.product_id {
                    self.grace_period_id = None;
                }
                self.product_id = id;
            }
            EntryEdit::Tranche(id) => self.grace_period_id = id,
            EntryEdit::Mcc(mcc) => self.mcc = mcc,
        }
        Ok(())
    }

    /// Recomputes a rate-based cashback for the current amount with the product's rounding.
    /// A typed cashback amount stays as typed (PLAN §6.3).
    pub fn refresh_cashback(&mut self, rounding: Rounding) {
        self.cashback = self.cashback.map(|c| c.recomputed(self.amount, rounding));
    }

    pub fn cashback_amount(&self) -> Kopecks {
        self.cashback.map_or(Kopecks::ZERO, |c| c.amount)
    }

    /// The value of the `%` column.
    pub fn display_rate_bp(&self) -> Option<i64> {
        self.cashback.and_then(|c| c.display_rate_bp(self.amount))
    }

    /// Fills the empty «Назначение», «Участник», «Карта», «MCC» and «Регулярная» from the last
    /// entry with the same title (PLAN §8: autocomplete of «Наименование»). Returns whether anything
    /// changed.
    pub fn autofill_from(&mut self, last_same: &Entry) -> bool {
        let before = (
            self.category_id,
            self.participant_id,
            self.product_id,
            self.mcc,
            self.is_regular,
        );
        self.category_id = self.category_id.or(last_same.category_id);
        self.participant_id = self.participant_id.or(last_same.participant_id);
        self.product_id = self.product_id.or(last_same.product_id);
        self.mcc = self.mcc.or(last_same.mcc);
        self.is_regular = self.is_regular || last_same.is_regular;
        before
            != (
                self.category_id,
                self.participant_id,
                self.product_id,
                self.mcc,
                self.is_regular,
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn july() -> Month {
        "2026-07".parse().unwrap()
    }

    fn apply(e: &mut Entry, edit: EntryEdit, rounding: Rounding) {
        e.apply(edit).unwrap();
        e.refresh_cashback(rounding);
    }

    #[test]
    fn rate_follows_amount_but_typed_cashback_does_not() {
        let r = Rounding::FloorRub;
        let mut e = Entry::new(1, july());
        apply(&mut e, EntryEdit::Amount(Kopecks::from_rubles(3181)), r);
        apply(&mut e, EntryEdit::Rate(Some(RateBp(500))), r);
        assert_eq!(e.cashback_amount(), Kopecks::from_rubles(159));
        assert_eq!(e.display_rate_bp(), Some(500));

        apply(&mut e, EntryEdit::Amount(Kopecks::from_rubles(1000)), r);
        assert_eq!(e.cashback_amount(), Kopecks::from_rubles(50));

        apply(
            &mut e,
            EntryEdit::Cashback(Some(Kopecks::from_rubles(250))),
            r,
        );
        apply(&mut e, EntryEdit::Amount(Kopecks::from_rubles(900)), r);
        assert_eq!(e.cashback_amount(), Kopecks::from_rubles(250));
        assert_eq!(e.display_rate_bp(), Some(2778));
        assert_eq!(e.cashback.unwrap().source, CashbackSource::ManualAmount);

        apply(&mut e, EntryEdit::Rate(None), r);
        assert_eq!(e.cashback, None);
    }

    #[test]
    fn rounding_of_the_product_applies() {
        let mut e = Entry::new(1, july());
        apply(
            &mut e,
            EntryEdit::Amount(Kopecks(123_450)),
            Rounding::FloorRub,
        );
        apply(
            &mut e,
            EntryEdit::Rate(Some(RateBp(350))),
            Rounding::FloorRub,
        );
        assert_eq!(e.cashback_amount(), Kopecks(4300));
        // Moving the entry to a card that rounds to kopecks recomputes the cashback.
        apply(&mut e, EntryEdit::Product(Some(7)), Rounding::HalfUpKop);
        assert_eq!(e.cashback_amount(), Kopecks(4321));
    }

    #[test]
    fn rejects_rate_above_100_percent() {
        let mut e = Entry::new(1, july());
        let err = e.apply(EntryEdit::Rate(Some(RateBp(10_001)))).unwrap_err();
        assert_eq!(err, EditError::RateTooHigh(10_001));
    }

    #[test]
    fn autofill_fills_only_blanks() {
        let mut last = Entry::new(1, july());
        last.category_id = Some(10);
        last.participant_id = Some(20);
        last.product_id = Some(30);
        last.mcc = Some("0742".parse().unwrap());
        last.is_regular = true;

        let mut fresh = Entry::new(2, july());
        assert!(fresh.autofill_from(&last));
        assert_eq!(
            (
                fresh.category_id,
                fresh.participant_id,
                fresh.product_id,
                fresh.mcc.map(String::from).as_deref(),
                fresh.is_regular
            ),
            (Some(10), Some(20), Some(30), Some("0742"), true)
        );

        let mut partly = Entry::new(3, july());
        partly.participant_id = Some(21);
        partly.mcc = Some("5411".parse().unwrap());
        partly.autofill_from(&last);
        assert_eq!(partly.participant_id, Some(21));
        assert_eq!(partly.category_id, Some(10));
        assert_eq!(partly.mcc, Some("5411".parse().unwrap()));

        let mut full = fresh.clone();
        assert!(!full.autofill_from(&last));
    }

    #[test]
    fn edit_wire_format() {
        let edit: EditInput =
            serde_json::from_str(r#"{"field":"date","value":"2026-07-05"}"#).unwrap();
        assert_eq!(edit, EditInput::Date(crate::date::parse_iso("2026-07-05")));
        let edit: EditInput =
            serde_json::from_str(r#"{"field":"card","value":"Газпром КР"}"#).unwrap();
        assert_eq!(edit, EditInput::Card(Some("Газпром КР".into())));
        let edit: EditInput = serde_json::from_str(r#"{"field":"mcc","value":"0742"}"#).unwrap();
        assert_eq!(edit, EditInput::Mcc(Some("0742".into())));
        let op: JournalOp = serde_json::from_str(
            r#"{"op":"edit","id":3,"edit":{"field":"isRegular","value":true}}"#,
        )
        .unwrap();
        assert_eq!(
            op,
            JournalOp::Edit {
                id: 3,
                edit: EditInput::IsRegular(true)
            }
        );
    }
}
