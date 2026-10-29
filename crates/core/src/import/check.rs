//! Reconciliation: our totals of an imported sheet against the totals Excel cached in it.
//! This is the stage 1 acceptance check (PLAN §12), shown to the user after an import.

use serde::Serialize;

use crate::cashback::Rounding;
use crate::dict::{Category, name_key};
use crate::journal::{Entry, EntryEdit};
use crate::money::Kopecks;
use crate::month::Month;
use crate::totals::month_totals;

use super::workbook::{ImportRow, SheetData};

/// One compared number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Compared {
    pub label: String,
    pub expected: Kopecks,
    pub actual: Kopecks,
}

impl Compared {
    pub fn matches(&self) -> bool {
        self.expected == self.actual
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthCheck {
    pub month: Month,
    pub sheet: String,
    pub rows: usize,
    /// Everything the sheet had a cached total for.
    pub compared: Vec<Compared>,
    /// True when every compared number is equal to the kopeck.
    pub matches: bool,
}

/// Builds journal entries from sheet rows (categories by name, cashback by `rounding`).
pub fn rows_to_entries(
    month: Month,
    rows: &[ImportRow],
    categories: &[Category],
    rounding: Rounding,
) -> Vec<Entry> {
    rows.iter()
        .zip(1..)
        .map(|(row, id)| {
            let mut entry = Entry::new(id, month);
            let category_id = row.category.as_deref().and_then(|name| {
                let key = name_key(name);
                categories
                    .iter()
                    .find(|c| name_key(&c.name) == key)
                    .map(|c| c.id)
            });
            let mut edits = vec![
                EntryEdit::Title(row.title.clone()),
                EntryEdit::Category(category_id),
                EntryEdit::IsRegular(row.is_regular),
                EntryEdit::Date(row.date),
                EntryEdit::Amount(row.amount),
            ];
            if let Some(rate) = row.rate {
                edits.push(EntryEdit::Rate(Some(rate)));
            } else if let Some(cashback) = row.cashback {
                edits.push(EntryEdit::Cashback(Some(cashback)));
            }
            for edit in edits {
                // Rates of the parser are within 0–100%: applying cannot fail.
                let _ = entry.apply(edit);
            }
            entry.refresh_cashback(rounding);
            entry
        })
        .collect()
}

/// Compares our totals of a sheet with what Excel computed. Categories of the sheet are
/// matched by name (the «Служебный» list).
pub fn check_sheet(sheet: &SheetData, category_names: &[String], rounding: Rounding) -> MonthCheck {
    let categories: Vec<Category> = category_names
        .iter()
        .zip(1..)
        .map(|(name, id)| Category {
            id,
            name: name.clone(),
            group_id: None,
            is_regular_default: false,
            sort_order: id,
            is_active: true,
        })
        .collect();
    let entries = rows_to_entries(sheet.month, &sheet.rows, &categories, rounding);
    let totals = month_totals(&entries, &categories, &[], &[]);
    let sum_of = |names: &[String]| -> Kopecks {
        let keys: Vec<String> = names.iter().map(|n| name_key(n)).collect();
        totals
            .by_category
            .iter()
            .filter(|c| keys.contains(&name_key(&c.label)))
            .map(|c| c.amount)
            .sum()
    };

    let e = &sheet.expected;
    let mut compared = Vec::new();
    if let Some(total) = e.total {
        compared.push(Compared {
            label: "Итого".into(),
            expected: total,
            actual: totals.total,
        });
    }
    for column in &e.columns {
        compared.push(Compared {
            label: column.label.clone(),
            expected: column.amount,
            actual: sum_of(&column.categories),
        });
    }
    if let Some(regular) = e.regular {
        compared.push(Compared {
            label: "Регулярные".into(),
            expected: regular,
            actual: totals.regular,
        });
    }
    if let Some(cashback) = e.cashback {
        compared.push(Compared {
            label: "Кэшбэк".into(),
            expected: cashback,
            actual: totals.cashback,
        });
    }
    MonthCheck {
        month: sheet.month,
        sheet: sheet.sheet.clone(),
        rows: sheet.rows.len(),
        matches: compared.iter().all(Compared::matches),
        compared,
    }
}
