//! Shapes sent to the UI. Field names are camelCase; money is integer kopecks.

use std::collections::{BTreeSet, HashMap};

use reestr_core::date::format_iso;
use reestr_core::{
    AvailableCheck, DeadlineSource, GraceId, Inflow, InflowId, MonthBalance, ProductId,
    RepaymentId, SavingsState, TrancheStatus, Urgency,
};
use reestr_core::{
    CardSuggestion, CashbackRule, CashbackSource, Entry, EntryId, Kopecks, Month, MonthTotals,
    RuleId, RuleUsage, month_totals,
};
use reestr_storage::{
    COPIES_KEPT, CipherSetting, Created, Dictionaries, FileInfo, ImportReport, ProductsData,
    StorageError, Vault, copies_dir,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfoDto {
    pub path: String,
    pub file_name: String,
    pub schema_version: u32,
    pub created_at: String,
    pub created_with: String,
    pub cipher_version: String,
    /// The copy of the day could not be made when the file was opened (D32).
    pub copy_failed: bool,
}

impl From<FileInfo> for FileInfoDto {
    fn from(info: FileInfo) -> Self {
        Self {
            file_name: info
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: info.path.to_string_lossy().into_owned(),
            schema_version: info.schema_version,
            created_at: info.created_at,
            created_with: info.created_with,
            cipher_version: info.cipher_version,
            copy_failed: false,
        }
    }
}

/// A copy of the file in its copies folder.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyView {
    pub file_name: String,
    /// "YYYY-MM-DD".
    pub date: String,
    pub size: u64,
}

/// The encryption of the file, as SQLCipher reports it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CipherView {
    pub version: String,
    pub kdf_iterations: String,
    pub kdf_algorithm: String,
    pub hmac_algorithm: String,
    pub page_size: String,
    pub provider: String,
}

/// The «О файле и пароле» page.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutView {
    pub file: FileInfoDto,
    pub size: u64,
    pub cipher: CipherView,
    /// `None` for a file opened from a copies folder: it makes no copies.
    pub copies_dir: Option<String>,
    /// Oldest first.
    pub copies: Vec<CopyView>,
    pub copies_kept: usize,
}

impl AboutView {
    pub fn build(vault: &Vault) -> Result<Self, StorageError> {
        let cipher = |s| vault.cipher_setting(s);
        Ok(Self {
            file: vault.info()?.into(),
            size: std::fs::metadata(vault.path())?.len(),
            cipher: CipherView {
                version: cipher(CipherSetting::Version)?,
                kdf_iterations: cipher(CipherSetting::KdfIterations)?,
                kdf_algorithm: cipher(CipherSetting::KdfAlgorithm)?,
                hmac_algorithm: cipher(CipherSetting::HmacAlgorithm)?,
                page_size: cipher(CipherSetting::PageSize)?,
                provider: cipher(CipherSetting::Provider)?,
            },
            copies_dir: copies_dir(vault.path()).map(|d| d.to_string_lossy().into_owned()),
            copies: vault
                .copies()?
                .into_iter()
                .map(|c| CopyView {
                    file_name: c
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    date: format_iso(c.date),
                    size: c.size,
                })
                .collect(),
            copies_kept: COPIES_KEPT,
        })
    }
}

/// A month tab: `title` is the sheet name `MM.YY`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthTab {
    pub month: Month,
    pub title: String,
}

impl MonthTab {
    pub fn list(months: impl IntoIterator<Item = Month>) -> Vec<Self> {
        months
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|month| Self {
                month,
                title: month.sheet_name(),
            })
            .collect()
    }
}

/// One journal row as shown in the grid: references as names.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryView {
    pub id: EntryId,
    pub title: String,
    pub category: Option<String>,
    pub is_regular: bool,
    pub participant: Option<String>,
    /// ISO `YYYY-MM-DD`.
    pub date: Option<String>,
    pub amount: Kopecks,
    /// The `%` column, in basis points: typed or derived.
    pub rate_bp: Option<i64>,
    pub cashback: Option<Kopecks>,
    pub cashback_source: Option<CashbackSource>,
    pub card: Option<String>,
    pub mcc: Option<String>,
}

/// Id → name lookups for building views.
struct Names {
    categories: HashMap<i64, String>,
    participants: HashMap<i64, String>,
    products: HashMap<i64, String>,
}

impl Names {
    fn new(d: &Dictionaries) -> Self {
        Self {
            categories: d
                .categories
                .iter()
                .map(|c| (c.id, c.name.clone()))
                .collect(),
            participants: d
                .participants
                .iter()
                .map(|p| (p.id, p.name.clone()))
                .collect(),
            products: d
                .products
                .iter()
                .map(|p| (p.id, p.short_name.clone()))
                .collect(),
        }
    }

    fn view(&self, e: &Entry) -> EntryView {
        let name =
            |map: &HashMap<i64, String>, id: Option<i64>| id.and_then(|i| map.get(&i).cloned());
        EntryView {
            id: e.id,
            title: e.title.clone(),
            category: name(&self.categories, e.category_id),
            is_regular: e.is_regular,
            participant: name(&self.participants, e.participant_id),
            date: e.date.map(format_iso),
            amount: e.amount,
            rate_bp: e.display_rate_bp(),
            cashback: e.cashback.map(|c| c.amount),
            cashback_source: e.cashback.map(|c| c.source),
            card: name(&self.products, e.product_id),
            mcc: e.mcc.map(String::from),
        }
    }
}

/// A cashback rule of the month with what is used of it: a row of the rules sheet and of
/// the «Лимиты по картам» block.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleView {
    pub id: RuleId,
    pub card: String,
    /// With `mcc` also `None`, the rule is «на всё».
    pub category: Option<String>,
    pub mcc: Option<String>,
    pub rate_bp: u32,
    pub spend_cap: Option<Kopecks>,
    pub cashback_cap: Option<Kopecks>,
    pub adjustment: Kopecks,
    pub notes: Option<String>,
    pub spent: Kopecks,
    pub cashback: Kopecks,
    pub entries: usize,
    pub remaining_spend: Option<Kopecks>,
    pub remaining_cashback: Option<Kopecks>,
    pub exhausted: bool,
}

impl RuleView {
    fn new(names: &Names, rule: &CashbackRule, usage: Option<&RuleUsage>) -> Self {
        Self {
            id: rule.id,
            card: names
                .products
                .get(&rule.product_id)
                .cloned()
                .unwrap_or_default(),
            category: rule
                .category_id
                .and_then(|c| names.categories.get(&c).cloned()),
            mcc: rule.mcc.map(String::from),
            rate_bp: rule.rate.0,
            spend_cap: rule.spend_cap,
            cashback_cap: rule.cashback_cap,
            adjustment: rule.spent_adjustment,
            notes: rule.notes.clone(),
            spent: usage.map_or(Kopecks::ZERO, |u| u.spent),
            cashback: usage.map_or(Kopecks::ZERO, |u| u.cashback),
            entries: usage.map_or(0, |u| u.entries),
            remaining_spend: usage.and_then(|u| u.remaining_spend),
            remaining_cashback: usage.and_then(|u| u.remaining_cashback),
            exhausted: usage.is_some_and(RuleUsage::is_exhausted),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthView {
    pub month: Month,
    pub title: String,
    pub entries: Vec<EntryView>,
    pub totals: MonthTotals,
    pub rules: Vec<RuleView>,
    /// The month the «copy rules» button copies from.
    pub copy_rules_from: Option<MonthTab>,
    /// Whether the card cell suggests cards (D6).
    pub card_suggestions: bool,
    /// «Приход / расход / дельта».
    pub balance: MonthBalance,
    pub inflows: Vec<InflowView>,
}

/// Money that came in, with names.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InflowView {
    pub id: InflowId,
    pub date: Option<String>,
    pub source: String,
    pub participant: Option<String>,
    pub card: Option<String>,
    pub amount: Kopecks,
    pub notes: Option<String>,
}

impl InflowView {
    fn new(names: &Names, i: Inflow) -> Self {
        let name =
            |map: &HashMap<i64, String>, id: Option<i64>| id.and_then(|i| map.get(&i).cloned());
        Self {
            id: i.id,
            date: i.date.map(format_iso),
            source: i.source,
            participant: name(&names.participants, i.participant_id),
            card: name(&names.products, i.product_id),
            amount: i.amount,
            notes: i.notes,
        }
    }
}

impl MonthView {
    pub fn build(vault: &Vault, month: Month) -> Result<Self, StorageError> {
        let dictionaries = vault.dictionaries()?;
        let entries = vault.entries(month)?;
        let names = Names::new(&dictionaries);
        let rules = vault.month_rules(month)?;
        Ok(Self {
            month,
            title: month.sheet_name(),
            entries: entries.iter().map(|e| names.view(e)).collect(),
            totals: month_totals(
                &entries,
                &dictionaries.categories,
                &dictionaries.groups,
                &dictionaries.templates,
            ),
            rules: rules
                .rules
                .iter()
                .map(|rule| {
                    let usage = rules.usage.iter().find(|u| u.rule_id == rule.id);
                    RuleView::new(&names, rule, usage)
                })
                .collect(),
            copy_rules_from: rules.copy_from.map(|m| MonthTab {
                month: m,
                title: m.sheet_name(),
            }),
            card_suggestions: vault.card_suggestions_enabled()?,
            balance: vault.month_balance(month)?,
            inflows: vault
                .inflows(month)?
                .into_iter()
                .map(|i| InflowView::new(&names, i))
                .collect(),
        })
    }
}

/// A card for the card cell: the best first (D6).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionView {
    pub card: String,
    pub rate_bp: u32,
    pub expected: Option<Kopecks>,
    pub exhausted: bool,
    pub remaining_spend: Option<Kopecks>,
    pub remaining_cashback: Option<Kopecks>,
}

impl SuggestionView {
    pub fn list(d: &Dictionaries, suggestions: Vec<CardSuggestion>) -> Vec<Self> {
        let names = Names::new(d);
        suggestions
            .into_iter()
            .filter_map(|s| {
                Some(Self {
                    card: names.products.get(&s.product_id)?.clone(),
                    rate_bp: s.rate.0,
                    expected: s.expected,
                    exhausted: s.exhausted,
                    remaining_spend: s.remaining_spend,
                    remaining_cashback: s.remaining_cashback,
                })
            })
            .collect()
    }
}

/// Whether Ctrl+Z / Ctrl+Y have something to do.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryState {
    pub can_undo: bool,
    pub can_redo: bool,
}

impl HistoryState {
    pub fn of(vault: &Vault) -> Self {
        Self {
            can_undo: vault.can_undo(),
            can_redo: vault.can_redo(),
        }
    }
}

/// The result of any change to the journal.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeResult {
    pub view: MonthView,
    /// Rows appended by the change, in order.
    pub appended: Vec<EntryId>,
    /// Dictionary items created from typed names (D17).
    pub created: Vec<Created>,
    pub history: HistoryState,
}

/// The result of undo/redo.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayResult {
    /// `None` when there was nothing to undo/redo.
    pub label: Option<String>,
    /// The month to show: the one the change touched, or the requested one.
    pub view: MonthView,
    pub history: HistoryState,
}

/// The result of a dictionary change.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictResult {
    pub dictionaries: Dictionaries,
    pub created: Vec<Created>,
    pub history: HistoryState,
}

/// The result of an import.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub report: ImportReport,
    pub history: HistoryState,
}

/// A tranche of a credit card with what it owes.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrancheView {
    pub id: GraceId,
    /// ISO dates.
    pub start: String,
    pub end: String,
    pub adjustment: Kopecks,
    pub notes: Option<String>,
    pub debt: Kopecks,
    pub repaid: Kopecks,
    pub remaining: Kopecks,
    pub purchases: usize,
    pub days_left: i64,
    pub status: TrancheStatus,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepaymentView {
    pub id: RepaymentId,
    pub tranche_id: GraceId,
    pub date: String,
    pub amount: Kopecks,
    pub notes: Option<String>,
}

/// A purchase on a credit card and its tranche.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseView {
    pub entry_id: EntryId,
    pub month: Month,
    pub date: Option<String>,
    pub title: String,
    pub amount: Kopecks,
    pub tranche_id: Option<GraceId>,
    /// Bound by hand rather than by date.
    pub manual: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditCardView {
    pub product_id: ProductId,
    pub card: String,
    pub bank: Option<String>,
    pub participant: Option<String>,
    pub credit_limit: Option<Kopecks>,
    pub grace_days: Option<u32>,
    pub manual_available: Option<Kopecks>,
    pub manual_available_on: Option<String>,
    pub debt: Kopecks,
    /// The entered balance against the computed one, when there is something to compare.
    pub available: Option<AvailableCheck>,
    pub unbound: usize,
    pub repayment_tranche: Option<GraceId>,
    pub tranches: Vec<TrancheView>,
    pub repayments: Vec<RepaymentView>,
    pub purchases: Vec<PurchaseView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavingsView {
    pub product_id: ProductId,
    pub card: String,
    pub bank: Option<String>,
    pub participant: Option<String>,
    pub promo_rate_bp: Option<u32>,
    pub closed_at: Option<String>,
    pub reopen_after_days: Option<u32>,
    pub state: SavingsState,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadlineView {
    pub date: String,
    pub card: Option<String>,
    pub days_left: i64,
    pub urgency: Urgency,
    pub source: DeadlineSource,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderView {
    pub id: i64,
    pub date: String,
    pub title: String,
    pub card: Option<String>,
    pub is_done: bool,
}

/// The «Продукты» and «Сроки» sheets and the month's product blocks.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductsView {
    pub credit: Vec<CreditCardView>,
    pub savings: Vec<SavingsView>,
    pub debt: Kopecks,
    pub deadlines: Vec<DeadlineView>,
    pub reminders: Vec<ReminderView>,
}

impl ProductsView {
    pub fn build(d: &Dictionaries, data: ProductsData) -> Self {
        let names = Names::new(d);
        let card = |id: ProductId| names.products.get(&id).cloned().unwrap_or_default();
        let product = |id: ProductId| d.products.iter().find(|p| p.id == id);
        let bank = |id: ProductId| {
            product(id)
                .and_then(|p| p.bank_id)
                .and_then(|b| d.banks.iter().find(|x| x.id == b))
                .map(|b| b.name.clone())
        };
        let participant = |id: ProductId| {
            product(id)
                .and_then(|p| p.participant_id)
                .and_then(|p| names.participants.get(&p).cloned())
        };
        Self {
            credit: data
                .credit
                .into_iter()
                .map(|c| CreditCardView {
                    product_id: c.product_id,
                    card: card(c.product_id),
                    bank: bank(c.product_id),
                    participant: participant(c.product_id),
                    credit_limit: c.terms.credit_limit,
                    grace_days: c.terms.default_grace_days,
                    manual_available: c.terms.manual_available,
                    manual_available_on: c.terms.manual_available_on.map(format_iso),
                    debt: c.debt,
                    available: c.available,
                    unbound: c.unbound,
                    repayment_tranche: c.repayment_tranche,
                    tranches: c
                        .tranches
                        .iter()
                        .zip(&c.states)
                        .map(|(t, s)| TrancheView {
                            id: t.id,
                            start: format_iso(t.start),
                            end: format_iso(t.end),
                            adjustment: t.debt_adjustment,
                            notes: t.notes.clone(),
                            debt: s.debt,
                            repaid: s.repaid,
                            remaining: s.remaining,
                            purchases: s.purchases,
                            days_left: s.days_left,
                            status: s.status,
                        })
                        .collect(),
                    repayments: c
                        .repayments
                        .into_iter()
                        .map(|r| RepaymentView {
                            id: r.id,
                            tranche_id: r.grace_period_id,
                            date: format_iso(r.date),
                            amount: r.amount,
                            notes: r.notes,
                        })
                        .collect(),
                    purchases: c
                        .purchases
                        .into_iter()
                        .map(|p| PurchaseView {
                            entry_id: p.entry.id,
                            month: p.entry.month,
                            date: p.entry.date.map(format_iso),
                            title: p.entry.title,
                            amount: p.entry.amount,
                            tranche_id: p.tranche,
                            manual: p.manual,
                        })
                        .collect(),
                })
                .collect(),
            savings: data
                .savings
                .into_iter()
                .map(|s| SavingsView {
                    product_id: s.product_id,
                    card: card(s.product_id),
                    bank: bank(s.product_id),
                    participant: participant(s.product_id),
                    promo_rate_bp: s.terms.promo_rate.map(|r| r.0),
                    closed_at: s.terms.promo_closed_at.map(format_iso),
                    reopen_after_days: s.terms.reopen_after_days,
                    state: s.state,
                })
                .collect(),
            debt: data.debt,
            deadlines: data
                .deadlines
                .into_iter()
                .map(|x| DeadlineView {
                    date: format_iso(x.date),
                    card: x.product_id.map(card),
                    days_left: x.days_left,
                    urgency: x.urgency,
                    source: x.source,
                })
                .collect(),
            reminders: data
                .reminders
                .into_iter()
                .map(|r| ReminderView {
                    id: r.id,
                    date: format_iso(r.date),
                    title: r.title,
                    card: r.product_id.map(card),
                    is_done: r.is_done,
                })
                .collect(),
        }
    }
}

/// The result of a change on the «Продукты» or «Сроки» sheet.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductsResult {
    pub view: ProductsView,
    pub created: Vec<Created>,
    pub history: HistoryState,
}
