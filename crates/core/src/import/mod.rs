//! Import of an xlsx workbook (PLAN §9): categories from «Служебный», entries from the
//! month sheets `MM.YY`, groups from merged totals columns. «Свод» is not imported: it is
//! recomputed.

mod cards;
mod check;
mod workbook;

pub use cards::{CardGuess, guess_card};
pub use check::{Compared, MonthCheck, check_sheet, rows_to_entries};
pub use workbook::{
    Expected, ExpectedColumn, Group, ImportError, ImportRow, SERVICE_SHEET, SheetData, Warning,
    Workbook, read_workbook,
};

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::dict::{ProductKind, name_key};

/// What the user confirmed in the import wizard for a card name found in the workbook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardSetup {
    pub short_name: String,
    pub bank: Option<String>,
    pub kind: Option<ProductKind>,
    pub participant: Option<String>,
}

/// A stable 64-bit FNV-1a hash, so import keys do not change between program versions.
pub(crate) fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Keys that identify imported rows, so importing the same workbook again adds nothing
/// (PLAN §9). A key is the month, the row's content and how many identical rows came before
/// it in that month — robust to rows moving within the sheet.
pub fn import_keys(sheet: &SheetData) -> Vec<String> {
    let mut seen: BTreeMap<u64, usize> = BTreeMap::new();
    sheet
        .rows
        .iter()
        .map(|row| {
            let content = format!(
                "{}|{}|{}|{}|{}|{:?}|{}|{:?}|{:?}|{}",
                row.date.map(crate::date::format_iso).unwrap_or_default(),
                name_key(&row.title),
                row.amount.get(),
                row.category.as_deref().map(name_key).unwrap_or_default(),
                row.participant.as_deref().map(name_key).unwrap_or_default(),
                row.rate.map(|r| r.0),
                row.card.as_deref().map(name_key).unwrap_or_default(),
                row.cashback.map(|c| c.get()),
                row.is_regular,
                sheet.month,
            );
            let hash = fnv1a(&content);
            let n = seen.entry(hash).or_default();
            *n += 1;
            format!("xlsx:{}:{hash:016x}:{n}", sheet.month)
        })
        .collect()
}
