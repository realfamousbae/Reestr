//! Export of a file (PLAN §9, D29–D31): an xlsx workbook laid out like the month sheets (§8), and a
//! json document with everything the file stores. Both are written from a [`Book`] that the
//! storage gathers; nothing here reads a database or the clock.

mod xlsx;

pub use xlsx::xlsx;

use serde::Serialize;

use crate::dict::{
    Bank, Category, CategoryGroup, Participant, Product, ProductId, RegularTemplate,
};
use crate::journal::Entry;
use crate::money::Kopecks;
use crate::month::Month;
use crate::products::{
    CreditTerms, CustomDeadline, GracePeriod, Repayment, SavingsTerms, TrancheState,
};
use crate::rules::{CashbackRule, RuleUsage};
use crate::summary::{Inflow, MonthBalance, YearSummary};
use crate::totals::MonthTotals;

/// The name and version of the json format: a reader checks them before anything else.
pub const JSON_FORMAT: &str = "reestr";
pub const JSON_VERSION: u32 = 1;

/// A month sheet: its entries and what the month computes from them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookMonth {
    pub month: Month,
    pub entries: Vec<Entry>,
    pub totals: MonthTotals,
    pub balance: MonthBalance,
}

/// Everything an export writes: what the file stores and, for the xlsx, what is computed from
/// it. Lists are in the order the program shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    /// The schema version of the file, written into the json.
    pub schema_version: u32,
    pub participants: Vec<Participant>,
    pub banks: Vec<Bank>,
    pub products: Vec<Product>,
    pub groups: Vec<CategoryGroup>,
    pub categories: Vec<Category>,
    pub templates: Vec<RegularTemplate>,
    /// Months with entries, oldest first.
    pub months: Vec<BookMonth>,
    /// Every cashback rule, oldest month first, with its usage by the journal.
    pub rules: Vec<(CashbackRule, Option<RuleUsage>)>,
    /// Every inflow, oldest month first.
    pub inflows: Vec<Inflow>,
    /// Months where the family's income plan was set, oldest first.
    pub plans: Vec<(Month, Kopecks)>,
    pub credit_terms: Vec<(ProductId, CreditTerms)>,
    /// Every tranche with what it owes on the export day.
    pub tranches: Vec<(GracePeriod, TrancheState)>,
    pub repayments: Vec<Repayment>,
    pub savings_terms: Vec<(ProductId, SavingsTerms)>,
    pub reminders: Vec<CustomDeadline>,
    /// «Свод» of every year with entries, by groups.
    pub years: Vec<YearSummary>,
    pub card_suggestions: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExportError {
    #[error("cannot write the workbook: {0}")]
    Xlsx(String),
    #[error("cannot write the json: {0}")]
    Json(String),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanRow {
    month: Month,
    amount: Kopecks,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Terms<'a, T> {
    product_id: ProductId,
    #[serde(flatten)]
    terms: &'a T,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    card_suggestions: bool,
}

/// The json document: stored data only, without computed totals or the change history (D31).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Document<'a> {
    format: &'static str,
    version: u32,
    schema_version: u32,
    participants: &'a [Participant],
    banks: &'a [Bank],
    products: &'a [Product],
    groups: &'a [CategoryGroup],
    categories: &'a [Category],
    templates: &'a [RegularTemplate],
    entries: Vec<&'a Entry>,
    cashback_rules: Vec<&'a CashbackRule>,
    inflows: &'a [Inflow],
    income_plans: Vec<PlanRow>,
    credit_terms: Vec<Terms<'a, CreditTerms>>,
    grace_periods: Vec<&'a GracePeriod>,
    repayments: &'a [Repayment],
    savings_terms: Vec<Terms<'a, SavingsTerms>>,
    reminders: &'a [CustomDeadline],
    settings: Settings,
}

/// The json export: pretty-printed, so it can be read and compared by eye.
pub fn json(book: &Book) -> Result<String, ExportError> {
    let document = Document {
        format: JSON_FORMAT,
        version: JSON_VERSION,
        schema_version: book.schema_version,
        participants: &book.participants,
        banks: &book.banks,
        products: &book.products,
        groups: &book.groups,
        categories: &book.categories,
        templates: &book.templates,
        entries: book.months.iter().flat_map(|m| &m.entries).collect(),
        cashback_rules: book.rules.iter().map(|(r, _)| r).collect(),
        inflows: &book.inflows,
        income_plans: book
            .plans
            .iter()
            .map(|&(month, amount)| PlanRow { month, amount })
            .collect(),
        credit_terms: book
            .credit_terms
            .iter()
            .map(|(product_id, terms)| Terms {
                product_id: *product_id,
                terms,
            })
            .collect(),
        grace_periods: book.tranches.iter().map(|(t, _)| t).collect(),
        repayments: &book.repayments,
        savings_terms: book
            .savings_terms
            .iter()
            .map(|(product_id, terms)| Terms {
                product_id: *product_id,
                terms,
            })
            .collect(),
        reminders: &book.reminders,
        settings: Settings {
            card_suggestions: book.card_suggestions,
        },
    };
    serde_json::to_string_pretty(&document).map_err(|e| ExportError::Json(e.to_string()))
}
