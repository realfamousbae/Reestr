//! Bank products (PLAN §7): credit card tranches with their grace periods, repayments, savings
//! accounts with promo windows, and the deadlines that come out of them.
//!
//! Banks count grace periods differently, so nothing here derives the dates: the user enters
//! the start and the end of every tranche. What is derived is which tranche a purchase belongs
//! to, what is owed, and what is due when (D28). `today` always comes from the caller.

use serde::{Deserialize, Serialize};
use time::{Date, Duration};

use crate::cashback::RateBp;
use crate::dict::ProductId;
use crate::journal::Entry;
use crate::money::Kopecks;

pub type GraceId = i64;
pub type RepaymentId = i64;
pub type DeadlineId = i64;

/// A deadline this close or closer is shown in color (PLAN §7: ≤ 7 days).
pub const SOON_DAYS: i64 = 7;

/// The grace period of a new tranche when neither the card terms nor an earlier tranche
/// suggest one; the dates stay editable.
pub const FALLBACK_GRACE_DAYS: i64 = 55;

/// Terms of a credit card.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditTerms {
    pub credit_limit: Option<Kopecks>,
    /// Only a hint for the end date of a new tranche.
    pub default_grace_days: Option<u32>,
    /// The available balance the bank showed, entered by hand; set together with its date.
    pub manual_available: Option<Kopecks>,
    /// The day [`Self::manual_available`] was valid for.
    #[serde(with = "crate::date::iso::option")]
    pub manual_available_on: Option<Date>,
}

/// The balance entered from the bank against the one Reestr computes for the same day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableCheck {
    #[serde(with = "crate::date::iso")]
    pub on: Date,
    /// What the bank showed.
    pub entered: Kopecks,
    /// The debt on that day.
    pub debt: Kopecks,
    /// Credit limit − debt; negative when the debt is above the limit.
    pub computed: Kopecks,
    /// `entered − computed`: above zero when the bank shows more than Reestr.
    pub difference: Kopecks,
}

/// One tranche («транш»): purchases from its start until the next tranche starts, to be repaid
/// by its end. A card may have several open tranches with overlapping periods.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GracePeriod {
    pub id: GraceId,
    pub product_id: ProductId,
    #[serde(with = "crate::date::iso")]
    pub start: Date,
    #[serde(with = "crate::date::iso")]
    pub end: Date,
    /// Debt that is not in the journal: a fee, a purchase before the file existed.
    pub debt_adjustment: Kopecks,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repayment {
    pub id: RepaymentId,
    pub grace_period_id: GraceId,
    #[serde(with = "crate::date::iso")]
    pub date: Date,
    pub amount: Kopecks,
    pub notes: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrancheStatus {
    /// In grace, including a fresh tranche with nothing bought yet.
    Active,
    Repaid,
    /// The end has passed and something is still owed.
    Overdue,
}

/// What a tranche owes today.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrancheState {
    pub id: GraceId,
    pub product_id: ProductId,
    /// Purchases of the tranche plus its adjustment.
    pub debt: Kopecks,
    pub repaid: Kopecks,
    pub remaining: Kopecks,
    pub purchases: usize,
    /// Negative once the end has passed.
    pub days_left: i64,
    pub status: TrancheStatus,
}

/// The tranche a purchase of `product` made on `date` goes to: the one of that card that
/// started last on or before that date (PLAN §7). `None` before the card's first tranche.
pub fn tranche_for(tranches: &[GracePeriod], product: ProductId, date: Date) -> Option<GraceId> {
    tranches
        .iter()
        .filter(|t| t.product_id == product && t.start <= date)
        .max_by_key(|t| (t.start, t.id))
        .map(|t| t.id)
}

/// The tranche of a journal entry: the one chosen by hand, else by its card and date. An
/// entry without a date is not bound (D28).
pub fn tranche_of_entry(tranches: &[GracePeriod], entry: &Entry) -> Option<GraceId> {
    match entry.grace_period_id {
        Some(id) => tranches.iter().any(|t| t.id == id).then_some(id),
        None => entry
            .product_id
            .zip(entry.date)
            .and_then(|(product, date)| tranche_for(tranches, product, date)),
    }
}

fn days_between(from: Date, to: Date) -> i64 {
    (to - from).whole_days()
}

/// The state of every tranche: `entries` are the purchases of credit cards (any month).
pub fn tranche_states(
    tranches: &[GracePeriod],
    entries: &[Entry],
    repayments: &[Repayment],
    today: Date,
) -> Vec<TrancheState> {
    tranches
        .iter()
        .map(|t| {
            let bound: Vec<&Entry> = entries
                .iter()
                .filter(|e| tranche_of_entry(tranches, e) == Some(t.id))
                .collect();
            let debt = bound.iter().map(|e| e.amount).sum::<Kopecks>() + t.debt_adjustment;
            let repaid = repayments
                .iter()
                .filter(|r| r.grace_period_id == t.id)
                .map(|r| r.amount)
                .sum::<Kopecks>();
            let remaining = debt - repaid;
            // A tranche that has just started and has nothing yet is open, not repaid.
            let empty = debt == Kopecks::ZERO && repaid == Kopecks::ZERO && t.end >= today;
            let status = if remaining <= Kopecks::ZERO && !empty {
                TrancheStatus::Repaid
            } else if remaining <= Kopecks::ZERO {
                TrancheStatus::Active
            } else if t.end < today {
                TrancheStatus::Overdue
            } else {
                TrancheStatus::Active
            };
            TrancheState {
                id: t.id,
                product_id: t.product_id,
                debt,
                repaid,
                remaining,
                purchases: bound.len(),
                days_left: days_between(today, t.end),
                status,
            }
        })
        .collect()
}

/// What is still owed on a card, or on all cards when `product` is `None`.
pub fn credit_debt(states: &[TrancheState], product: Option<ProductId>) -> Kopecks {
    states
        .iter()
        .filter(|s| product.is_none_or(|p| s.product_id == p))
        .filter(|s| s.status != TrancheStatus::Repaid)
        .map(|s| s.remaining)
        .sum()
}

/// What was owed on a card by the end of `on`, however the tranches stand today: later purchases
/// and repayments do not count, nor does the adjustment of a tranche that had not started. An
/// undated purchase is in a tranche only by hand, and counts as it does in [`credit_debt`].
pub fn credit_debt_on(
    tranches: &[GracePeriod],
    entries: &[Entry],
    repayments: &[Repayment],
    product: ProductId,
    on: Date,
) -> Kopecks {
    // Later tranches stay: a purchase of that day may be bound to one by hand.
    let tranches: Vec<GracePeriod> = tranches
        .iter()
        .cloned()
        .map(|mut t| {
            if t.start > on {
                t.debt_adjustment = Kopecks::ZERO;
            }
            t
        })
        .collect();
    let entries: Vec<Entry> = entries
        .iter()
        .filter(|e| e.date.is_none_or(|d| d <= on))
        .cloned()
        .collect();
    let repayments: Vec<Repayment> = repayments
        .iter()
        .filter(|r| r.date <= on)
        .cloned()
        .collect();
    let states = tranche_states(&tranches, &entries, &repayments, on);
    credit_debt(&states, Some(product))
}

/// The check of the balance entered for a card; `None` until the card has a limit, a value and
/// its date, as there is nothing to compare then.
pub fn available_check(
    product: ProductId,
    terms: &CreditTerms,
    tranches: &[GracePeriod],
    entries: &[Entry],
    repayments: &[Repayment],
) -> Option<AvailableCheck> {
    let limit = terms.credit_limit?;
    let entered = terms.manual_available?;
    let on = terms.manual_available_on?;
    let debt = credit_debt_on(tranches, entries, repayments, product, on);
    let computed = limit - debt;
    Some(AvailableCheck {
        on,
        entered,
        debt,
        computed,
        difference: entered - computed,
    })
}

/// Dates for «Начался новый грейс»: from `today`, as long as the card terms say, else as long
/// as the card's last tranche, else [`FALLBACK_GRACE_DAYS`].
pub fn new_tranche_dates(
    tranches: &[GracePeriod],
    product: ProductId,
    terms: Option<&CreditTerms>,
    today: Date,
) -> (Date, Date) {
    let last = tranches
        .iter()
        .filter(|t| t.product_id == product)
        .max_by_key(|t| (t.start, t.id));
    let days = terms
        .and_then(|t| t.default_grace_days)
        .map(i64::from)
        .or_else(|| last.map(|t| days_between(t.start, t.end)))
        .unwrap_or(FALLBACK_GRACE_DAYS);
    (today, today + Duration::days(days))
}

/// The tranche a new repayment of a card goes to by default (D27): the one with something
/// owed that ends first, else the card's latest.
pub fn repayment_tranche(
    tranches: &[GracePeriod],
    states: &[TrancheState],
    product: ProductId,
) -> Option<GraceId> {
    let of_card = || tranches.iter().filter(|t| t.product_id == product);
    let unpaid = of_card()
        .filter(|t| {
            states
                .iter()
                .any(|s| s.id == t.id && s.remaining > Kopecks::ZERO)
        })
        .min_by_key(|t| (t.end, t.id));
    unpaid
        .or_else(|| of_card().max_by_key(|t| (t.start, t.id)))
        .map(|t| t.id)
}

/// A savings account or a deposit with a promo rate that can be opened again only some time
/// after closing («нет НС/вкладов 90+ дней»).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavingsTerms {
    pub promo_rate: Option<RateBp>,
    #[serde(with = "crate::date::iso::option")]
    pub promo_closed_at: Option<Date>,
    pub reopen_after_days: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum SavingsState {
    /// Not closed.
    Open,
    /// Closed; the promo can be taken again from `from`.
    #[serde(rename_all = "camelCase")]
    Waiting {
        #[serde(with = "crate::date::iso")]
        from: Date,
        days_left: i64,
    },
    /// Closed long enough: can be opened now.
    CanOpen {
        #[serde(with = "crate::date::iso")]
        since: Date,
    },
}

impl SavingsTerms {
    /// «Можно открыть с».
    pub fn can_reopen_at(&self) -> Option<Date> {
        self.promo_closed_at
            .map(|closed| closed + Duration::days(i64::from(self.reopen_after_days.unwrap_or(0))))
    }

    pub fn state(&self, today: Date) -> SavingsState {
        match self.can_reopen_at() {
            None => SavingsState::Open,
            Some(from) if from > today => SavingsState::Waiting {
                from,
                days_left: days_between(today, from),
            },
            Some(since) => SavingsState::CanOpen { since },
        }
    }
}

/// A reminder the user wrote: «закрыть вклад», «перевыпустить карту».
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomDeadline {
    pub id: DeadlineId,
    pub product_id: Option<ProductId>,
    #[serde(with = "crate::date::iso")]
    pub date: Date,
    pub title: String,
    pub is_done: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Urgency {
    /// The date has passed and it still matters.
    Overdue,
    /// Within [`SOON_DAYS`].
    Soon,
    Later,
}

impl Urgency {
    fn of(days_left: i64) -> Self {
        if days_left < 0 {
            Self::Overdue
        } else if days_left <= SOON_DAYS {
            Self::Soon
        } else {
            Self::Later
        }
    }
}

/// Where a deadline comes from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DeadlineSource {
    /// The end of an unpaid tranche.
    #[serde(rename_all = "camelCase")]
    GraceEnd {
        grace_period_id: GraceId,
        remaining: Kopecks,
    },
    /// The promo of a closed savings account can be taken again.
    SavingsReopen,
    #[serde(rename_all = "camelCase")]
    Custom { id: DeadlineId, title: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Deadline {
    #[serde(with = "crate::date::iso")]
    pub date: Date,
    pub product_id: Option<ProductId>,
    pub days_left: i64,
    pub urgency: Urgency,
    pub source: DeadlineSource,
}

/// The «Сроки» list (PLAN §7): ends of unpaid tranches, reopening dates of savings accounts
/// still waiting, and reminders not done yet; by date.
pub fn deadlines(
    tranches: &[GracePeriod],
    states: &[TrancheState],
    savings: &[(ProductId, SavingsTerms)],
    custom: &[CustomDeadline],
    today: Date,
) -> Vec<Deadline> {
    let mut list = Vec::new();
    for t in tranches {
        let Some(state) = states.iter().find(|s| s.id == t.id) else {
            continue;
        };
        // Nothing owed (repaid, or nothing bought yet): nothing is due.
        if state.remaining <= Kopecks::ZERO {
            continue;
        }
        list.push(Deadline {
            date: t.end,
            product_id: Some(t.product_id),
            days_left: state.days_left,
            urgency: Urgency::of(state.days_left),
            source: DeadlineSource::GraceEnd {
                grace_period_id: t.id,
                remaining: state.remaining,
            },
        });
    }
    for (product, terms) in savings {
        if let SavingsState::Waiting { from, days_left } = terms.state(today) {
            list.push(Deadline {
                date: from,
                product_id: Some(*product),
                days_left,
                urgency: Urgency::of(days_left),
                source: DeadlineSource::SavingsReopen,
            });
        }
    }
    for d in custom.iter().filter(|d| !d.is_done) {
        let days_left = days_between(today, d.date);
        list.push(Deadline {
            date: d.date,
            product_id: d.product_id,
            days_left,
            urgency: Urgency::of(days_left),
            source: DeadlineSource::Custom {
                id: d.id,
                title: d.title.clone(),
            },
        });
    }
    list.sort_by_key(|d| d.date);
    list
}

/// A change of a credit card's terms.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum CreditInput {
    CreditLimit(Option<Kopecks>),
    GraceDays(Option<u32>),
    /// The balance from the bank with its date: both, or neither to clear them.
    ManualAvailable {
        amount: Option<Kopecks>,
        #[serde(with = "crate::date::iso::option")]
        on: Option<Date>,
    },
}

/// A change of a savings account's terms.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum SavingsInput {
    PromoRate(Option<RateBp>),
    ClosedAt(#[serde(with = "crate::date::iso::option")] Option<Date>),
    ReopenAfterDays(Option<u32>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum TrancheInput {
    Start(#[serde(with = "crate::date::iso")] Date),
    End(#[serde(with = "crate::date::iso")] Date),
    Adjustment(Kopecks),
    Notes(Option<String>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum RepaymentInput {
    Date(#[serde(with = "crate::date::iso")] Date),
    Amount(Kopecks),
    Tranche(GraceId),
    Notes(Option<String>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum DeadlineInput {
    Date(#[serde(with = "crate::date::iso")] Date),
    Title(String),
    /// A card name; missing ones are created (D17).
    Card(Option<String>),
    Done(bool),
}

/// One change on the «Продукты» and «Сроки» sheets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum ProductOp {
    CreditTerms {
        product: ProductId,
        edit: CreditInput,
    },
    SavingsTerms {
        product: ProductId,
        edit: SavingsInput,
    },
    /// A new tranche; missing dates come from [`new_tranche_dates`] («Начался новый грейс»).
    AddTranche {
        product: ProductId,
        edits: Vec<TrancheInput>,
    },
    EditTranche {
        id: GraceId,
        edit: TrancheInput,
    },
    DeleteTranches {
        ids: Vec<GraceId>,
    },
    /// A repayment of a card; without a tranche it goes to [`repayment_tranche`] (D27), without
    /// a date it is dated today.
    AddRepayment {
        product: ProductId,
        edits: Vec<RepaymentInput>,
    },
    EditRepayment {
        id: RepaymentId,
        edit: RepaymentInput,
    },
    DeleteRepayments {
        ids: Vec<RepaymentId>,
    },
    /// Moves a purchase to a tranche by hand, or back to the binding by date.
    Bind {
        entry: crate::journal::EntryId,
        tranche: Option<GraceId>,
    },
    AddDeadline {
        edits: Vec<DeadlineInput>,
    },
    EditDeadline {
        id: DeadlineId,
        edit: DeadlineInput,
    },
    DeleteDeadlines {
        ids: Vec<DeadlineId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProductError {
    #[error("a tranche cannot end before it starts")]
    EndBeforeStart,
    #[error("a repayment must be above zero")]
    AmountNotPositive,
    #[error("a limit cannot be negative")]
    NegativeLimit,
    #[error("rate {0} bp is above 100%")]
    RateTooHigh(u32),
    #[error("a number of days must be above zero")]
    DaysNotPositive,
    #[error("a reminder needs a title")]
    EmptyTitle,
    #[error("the card has no tranche to take a repayment")]
    NoTranche,
    #[error("an available balance and its date go together")]
    AvailableWithoutDate,
}

impl CreditInput {
    pub fn validate(&self) -> Result<(), ProductError> {
        match self {
            Self::CreditLimit(Some(limit)) if *limit < Kopecks::ZERO => {
                Err(ProductError::NegativeLimit)
            }
            Self::GraceDays(Some(0)) => Err(ProductError::DaysNotPositive),
            Self::ManualAvailable { amount, on } if amount.is_some() != on.is_some() => {
                Err(ProductError::AvailableWithoutDate)
            }
            _ => Ok(()),
        }
    }
}

impl SavingsInput {
    pub fn validate(&self) -> Result<(), ProductError> {
        match self {
            Self::PromoRate(Some(rate)) if *rate > RateBp::MAX => {
                Err(ProductError::RateTooHigh(rate.0))
            }
            _ => Ok(()),
        }
    }
}

impl RepaymentInput {
    pub fn validate(&self) -> Result<(), ProductError> {
        match self {
            Self::Amount(amount) if *amount <= Kopecks::ZERO => {
                Err(ProductError::AmountNotPositive)
            }
            _ => Ok(()),
        }
    }
}

impl DeadlineInput {
    pub fn validate(&self) -> Result<(), ProductError> {
        match self {
            Self::Title(title) if title.trim().is_empty() => Err(ProductError::EmptyTitle),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::date::parse_iso;
    use crate::month::Month;

    const CARD: ProductId = 1;
    const OTHER: ProductId = 2;

    fn d(s: &str) -> Date {
        parse_iso(s).unwrap()
    }

    fn rub(r: i64) -> Kopecks {
        Kopecks::from_rubles(r)
    }

    fn tranche(id: GraceId, product: ProductId, start: &str, end: &str) -> GracePeriod {
        GracePeriod {
            id,
            product_id: product,
            start: d(start),
            end: d(end),
            debt_adjustment: Kopecks::ZERO,
            notes: None,
        }
    }

    fn purchase(id: i64, product: ProductId, date: Option<&str>, amount: i64) -> Entry {
        let month: Month = "2026-07".parse().unwrap();
        let mut e = Entry::new(id, month);
        e.product_id = Some(product);
        e.date = date.map(d);
        e.amount = rub(amount);
        e
    }

    fn repayment(id: RepaymentId, tranche: GraceId, date: &str, amount: i64) -> Repayment {
        Repayment {
            id,
            grace_period_id: tranche,
            date: d(date),
            amount: rub(amount),
            notes: None,
        }
    }

    fn terms(limit: Option<i64>, entered: Option<i64>, on: Option<&str>) -> CreditTerms {
        CreditTerms {
            credit_limit: limit.map(rub),
            default_grace_days: None,
            manual_available: entered.map(rub),
            manual_available_on: on.map(d),
        }
    }

    /// A card with a 100 000 limit: 30 000 bought in June, 10 000 repaid on the 20th.
    fn bank_card() -> (Vec<GracePeriod>, Vec<Entry>, Vec<Repayment>) {
        (
            vec![tranche(1, CARD, "2026-06-01", "2026-09-28")],
            vec![purchase(1, CARD, Some("2026-06-10"), 30_000)],
            vec![repayment(1, 1, "2026-06-20", 10_000)],
        )
    }

    #[test]
    fn the_entered_balance_matches_the_computed_one() {
        let (t, e, r) = bank_card();
        let terms = terms(Some(100_000), Some(80_000), Some("2026-06-25"));
        let check = available_check(CARD, &terms, &t, &e, &r).unwrap();
        assert_eq!(
            check,
            AvailableCheck {
                on: d("2026-06-25"),
                entered: rub(80_000),
                debt: rub(20_000),
                computed: rub(80_000),
                difference: Kopecks::ZERO,
            }
        );
    }

    #[test]
    fn the_difference_is_entered_minus_computed() {
        let (t, e, r) = bank_card();
        let more = terms(Some(100_000), Some(80_300), Some("2026-06-25"));
        let check = available_check(CARD, &more, &t, &e, &r).unwrap();
        assert_eq!((check.computed, check.difference), (rub(80_000), rub(300)));
        let less = terms(Some(100_000), Some(79_000), Some("2026-06-25"));
        let check = available_check(CARD, &less, &t, &e, &r).unwrap();
        assert_eq!(check.difference, rub(-1000));
    }

    #[test]
    fn only_what_happened_by_the_date_counts() {
        let (t, mut e, mut r) = bank_card();
        // On the date itself: counts. A day later: does not.
        e.push(purchase(2, CARD, Some("2026-06-25"), 1_000));
        e.push(purchase(3, CARD, Some("2026-06-26"), 7_000));
        r.push(repayment(2, 1, "2026-06-25", 500));
        r.push(repayment(3, 1, "2026-06-30", 4_000));
        assert_eq!(
            credit_debt_on(&t, &e, &r, CARD, d("2026-06-25")),
            rub(20_500)
        );
        // Before the repayment the whole purchase is owed.
        assert_eq!(
            credit_debt_on(&t, &e, &r, CARD, d("2026-06-19")),
            rub(30_000)
        );
        // Before any purchase nothing is owed.
        assert_eq!(credit_debt_on(&t, &e, &r, CARD, d("2026-06-01")), rub(0));
        let today = tranche_states(&t, &e, &r, d("2026-07-01"));
        assert_eq!(credit_debt(&today, Some(CARD)), rub(23_500));
    }

    #[test]
    fn a_later_tranche_counts_only_for_what_happened_by_the_date() {
        let mut t = vec![
            tranche(1, CARD, "2026-06-01", "2026-09-28"),
            tranche(2, CARD, "2026-07-01", "2026-10-28"),
        ];
        t[1].debt_adjustment = rub(900);
        // Bought in June, moved to the July tranche by hand: owed in June all the same.
        let mut moved = purchase(1, CARD, Some("2026-06-10"), 5_000);
        moved.grace_period_id = Some(2);
        let e = [moved];
        assert_eq!(
            credit_debt_on(&t, &e, &[], CARD, d("2026-06-25")),
            rub(5_000)
        );
        assert_eq!(
            credit_debt_on(&t, &e, &[], CARD, d("2026-07-01")),
            rub(5_900)
        );
    }

    #[test]
    fn another_cards_debt_and_undated_purchases() {
        let (mut t, mut e, r) = bank_card();
        t.push(tranche(2, OTHER, "2026-06-01", "2026-07-26"));
        e.push(purchase(2, OTHER, Some("2026-06-10"), 9_000));
        // Without a date it is in no tranche; bound by hand it is, as in today's debt.
        e.push(purchase(3, CARD, None, 700));
        assert_eq!(
            credit_debt_on(&t, &e, &r, CARD, d("2026-06-25")),
            rub(20_000)
        );
        e[2].grace_period_id = Some(1);
        assert_eq!(
            credit_debt_on(&t, &e, &r, CARD, d("2026-06-25")),
            rub(20_700)
        );
    }

    #[test]
    fn debt_above_the_limit_gives_a_negative_balance() {
        let (t, e, r) = bank_card();
        let over = terms(Some(15_000), Some(-5_000), Some("2026-06-25"));
        let check = available_check(CARD, &over, &t, &e, &r).unwrap();
        assert_eq!((check.computed, check.difference), (rub(-5_000), rub(0)));
    }

    #[test]
    fn no_comparison_without_a_limit_a_value_or_a_date() {
        let (t, e, r) = bank_card();
        for incomplete in [
            terms(None, Some(80_000), Some("2026-06-25")),
            terms(Some(100_000), None, Some("2026-06-25")),
            terms(Some(100_000), Some(80_000), None),
            CreditTerms::default(),
        ] {
            assert_eq!(available_check(CARD, &incomplete, &t, &e, &r), None);
        }
        // A limit of zero is a limit.
        let zero = terms(Some(0), Some(0), Some("2026-06-25"));
        let check = available_check(CARD, &zero, &t, &e, &r).unwrap();
        assert_eq!(check.computed, rub(-20_000));
    }

    #[test]
    fn the_balance_and_its_date_go_together() {
        let input = |amount: Option<i64>, on: Option<&str>| CreditInput::ManualAvailable {
            amount: amount.map(rub),
            on: on.map(d),
        };
        assert_eq!(
            input(Some(1), None).validate(),
            Err(ProductError::AvailableWithoutDate)
        );
        assert_eq!(
            input(None, Some("2026-06-25")).validate(),
            Err(ProductError::AvailableWithoutDate)
        );
        assert_eq!(input(None, None).validate(), Ok(()));
        // A bank can show a negative balance, and the date is not limited (like other dates).
        assert_eq!(input(Some(-1), Some("2099-01-01")).validate(), Ok(()));

        // As the UI sends them.
        let read = |json: &str| serde_json::from_str::<CreditInput>(json).unwrap();
        assert_eq!(
            read(r#"{"field":"manualAvailable","value":{"amount":100,"on":"2026-06-25"}}"#),
            CreditInput::ManualAvailable {
                amount: Some(Kopecks(100)),
                on: Some(d("2026-06-25")),
            }
        );
        assert_eq!(
            read(r#"{"field":"manualAvailable","value":{"amount":null,"on":null}}"#),
            input(None, None)
        );
    }

    /// A 120-day grace card: a new tranche every month, four alive at once.
    fn overlapping() -> Vec<GracePeriod> {
        vec![
            tranche(1, CARD, "2026-04-01", "2026-07-29"),
            tranche(2, CARD, "2026-05-01", "2026-08-28"),
            tranche(3, CARD, "2026-06-01", "2026-09-28"),
            tranche(4, CARD, "2026-07-01", "2026-10-28"),
            tranche(5, OTHER, "2026-06-15", "2026-08-09"),
        ]
    }

    #[test]
    fn purchases_go_to_the_latest_tranche_started_by_their_date() {
        let t = overlapping();
        assert_eq!(tranche_for(&t, CARD, d("2026-05-15")), Some(2));
        assert_eq!(tranche_for(&t, CARD, d("2026-06-01")), Some(3));
        assert_eq!(tranche_for(&t, CARD, d("2026-10-01")), Some(4));
        // Another card's tranches do not count.
        assert_eq!(tranche_for(&t, OTHER, d("2026-06-20")), Some(5));
        assert_eq!(tranche_for(&t, OTHER, d("2026-05-20")), None);
    }

    #[test]
    fn purchase_before_the_first_tranche_and_without_a_date() {
        let t = overlapping();
        let early = purchase(1, CARD, Some("2026-03-31"), 1000);
        let undated = purchase(2, CARD, None, 500);
        assert_eq!(tranche_of_entry(&t, &early), None);
        assert_eq!(tranche_of_entry(&t, &undated), None);
        // By hand, it can still go to a tranche; a deleted tranche no longer binds.
        let mut manual = early.clone();
        manual.grace_period_id = Some(1);
        assert_eq!(tranche_of_entry(&t, &manual), Some(1));
        manual.grace_period_id = Some(99);
        assert_eq!(tranche_of_entry(&t, &manual), None);

        let states = tranche_states(&t, &[early, undated], &[], d("2026-07-15"));
        assert!(states.iter().all(|s| s.purchases == 0));
    }

    #[test]
    fn debt_repayments_and_status() {
        let mut t = overlapping();
        t[1].debt_adjustment = rub(300);
        let entries = [
            purchase(1, CARD, Some("2026-04-10"), 4000),
            purchase(2, CARD, Some("2026-05-03"), 2500),
            purchase(3, CARD, Some("2026-05-20"), -500), // a refund
            purchase(4, CARD, Some("2026-06-11"), 1200),
            purchase(5, OTHER, Some("2026-06-20"), 800),
        ];
        let repayments = [
            repayment(1, 1, "2026-07-01", 4000),
            // Partial: 1 000 of 2 300.
            repayment(2, 2, "2026-07-10", 1000),
        ];
        let today = d("2026-08-30");
        let s = tranche_states(&t, &entries, &repayments, today);
        let by_id = |id| s.iter().find(|x| x.id == id).unwrap();

        // Paid in full before its end.
        assert_eq!(by_id(1).status, TrancheStatus::Repaid);
        assert_eq!(by_id(1).remaining, Kopecks::ZERO);
        // 2 500 − 500 + 300 = 2 300, 1 000 repaid, ended 28.08: overdue by two days.
        let second = by_id(2);
        assert_eq!(
            (second.debt, second.repaid, second.remaining),
            (rub(2300), rub(1000), rub(1300))
        );
        assert_eq!(second.purchases, 2);
        assert_eq!(second.days_left, -2);
        assert_eq!(second.status, TrancheStatus::Overdue);
        // Still in grace.
        assert_eq!(by_id(3).status, TrancheStatus::Active);
        assert_eq!(by_id(3).days_left, 29);
        // Nothing bought yet and still in grace: open, owing nothing.
        assert_eq!(by_id(4).status, TrancheStatus::Active);
        assert_eq!(by_id(4).remaining, Kopecks::ZERO);
        // The other card is overdue too (ended 09.08).
        assert_eq!(by_id(5).status, TrancheStatus::Overdue);

        assert_eq!(credit_debt(&s, Some(CARD)), rub(2500));
        assert_eq!(credit_debt(&s, None), rub(3300));
    }

    #[test]
    fn overpayment_counts_as_repaid() {
        let t = [tranche(1, CARD, "2026-07-01", "2026-08-20")];
        let entries = [purchase(1, CARD, Some("2026-07-02"), 1000)];
        let s = tranche_states(
            &t,
            &entries,
            &[repayment(1, 1, "2026-07-05", 1500)],
            d("2026-09-01"),
        );
        assert_eq!(s[0].remaining, -rub(500));
        assert_eq!(s[0].status, TrancheStatus::Repaid);
        assert_eq!(credit_debt(&s, None), Kopecks::ZERO);
    }

    #[test]
    fn new_tranche_and_default_repayment_tranche() {
        let t = overlapping();
        let today = d("2026-08-01");
        let terms = CreditTerms {
            credit_limit: Some(rub(150_000)),
            default_grace_days: Some(120),
            ..CreditTerms::default()
        };
        assert_eq!(
            new_tranche_dates(&t, CARD, Some(&terms), today),
            (today, d("2026-11-29"))
        );
        // Without terms: as long as the last tranche (01.07–28.10 = 119 days).
        assert_eq!(
            new_tranche_dates(&t, CARD, None, today),
            (today, d("2026-11-28"))
        );
        assert_eq!(
            new_tranche_dates(&[], CARD, None, today),
            (today, d("2026-09-25"))
        );

        let entries = [
            purchase(1, CARD, Some("2026-04-10"), 4000),
            purchase(2, CARD, Some("2026-06-03"), 2500),
        ];
        let s = tranche_states(&t, &entries, &[repayment(1, 1, "2026-07-01", 4000)], today);
        // Tranche 1 is repaid, tranche 2 has nothing: the one owing that ends first is 3.
        assert_eq!(repayment_tranche(&t, &s, CARD), Some(3));
        // Nothing owed anywhere: the latest.
        let s = tranche_states(&t, &[], &[], today);
        assert_eq!(repayment_tranche(&t, &s, CARD), Some(4));
        assert_eq!(repayment_tranche(&t, &s, 77), None);
    }

    #[test]
    fn savings_reopen() {
        let today = d("2026-10-27");
        let open = SavingsTerms {
            promo_rate: Some(RateBp(1600)),
            ..Default::default()
        };
        assert_eq!(open.state(today), SavingsState::Open);
        let closed = SavingsTerms {
            promo_rate: Some(RateBp(1600)),
            promo_closed_at: Some(d("2026-09-01")),
            reopen_after_days: Some(90),
        };
        assert_eq!(closed.can_reopen_at(), Some(d("2026-11-30")));
        assert_eq!(
            closed.state(today),
            SavingsState::Waiting {
                from: d("2026-11-30"),
                days_left: 34
            }
        );
        assert_eq!(
            closed.state(d("2026-11-30")),
            SavingsState::CanOpen {
                since: d("2026-11-30")
            }
        );
        let no_wait = SavingsTerms {
            promo_closed_at: Some(d("2026-10-01")),
            ..Default::default()
        };
        assert_eq!(
            no_wait.state(today),
            SavingsState::CanOpen {
                since: d("2026-10-01")
            }
        );
    }

    #[test]
    fn savings_state_on_the_wire() {
        let json = serde_json::to_value(SavingsState::Waiting {
            from: d("2026-10-31"),
            days_left: 4,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"state": "waiting", "from": "2026-10-31", "daysLeft": 4})
        );
        let source = serde_json::to_value(DeadlineSource::GraceEnd {
            grace_period_id: 2,
            remaining: rub(10),
        })
        .unwrap();
        assert_eq!(
            source,
            serde_json::json!({"kind": "graceEnd", "gracePeriodId": 2, "remaining": 1000})
        );
    }

    #[test]
    fn deadlines_by_date_with_urgency() {
        let t = overlapping();
        let today = d("2026-08-25");
        let entries = [
            purchase(1, CARD, Some("2026-05-10"), 2000),
            purchase(2, CARD, Some("2026-06-10"), 3000),
            purchase(3, OTHER, Some("2026-06-20"), 500),
        ];
        let s = tranche_states(&t, &entries, &[], today);
        let savings = [
            (
                7,
                SavingsTerms {
                    promo_rate: None,
                    promo_closed_at: Some(d("2026-08-01")),
                    reopen_after_days: Some(30),
                },
            ),
            (
                8,
                SavingsTerms {
                    promo_rate: None,
                    promo_closed_at: Some(d("2026-05-01")),
                    reopen_after_days: Some(30),
                },
            ),
        ];
        let custom = [
            CustomDeadline {
                id: 1,
                product_id: None,
                date: d("2026-12-01"),
                title: "Перевыпустить карту".into(),
                is_done: false,
            },
            CustomDeadline {
                id: 2,
                product_id: None,
                date: d("2026-08-20"),
                title: "Уже сделано".into(),
                is_done: true,
            },
        ];
        let list = deadlines(&t, &s, &savings, &custom, today);
        let short: Vec<_> = list
            .iter()
            .map(|x| (crate::date::format_iso(x.date), x.urgency, x.days_left))
            .collect();
        assert_eq!(
            short,
            [
                // ОTHER's tranche ended 09.08 with 500 ₽ unpaid.
                ("2026-08-09".to_owned(), Urgency::Overdue, -16),
                // Tranche 2 (May purchase) ends in 3 days.
                ("2026-08-28".to_owned(), Urgency::Soon, 3),
                ("2026-08-31".to_owned(), Urgency::Soon, 6),
                ("2026-09-28".to_owned(), Urgency::Later, 34),
                ("2026-12-01".to_owned(), Urgency::Later, 98),
            ]
        );
        assert!(matches!(
            list[1].source,
            DeadlineSource::GraceEnd {
                grace_period_id: 2,
                remaining
            } if remaining == rub(2000)
        ));
        assert_eq!(list[2].source, DeadlineSource::SavingsReopen);
        assert_eq!(list[2].product_id, Some(7));
        // Tranches 1 and 4 have nothing to pay; product 8 can be opened already; the done
        // reminder is gone.
        assert_eq!(list.len(), 5);
    }

    #[test]
    fn inputs_are_checked_and_read_from_the_wire() {
        assert_eq!(
            RepaymentInput::Amount(Kopecks::ZERO).validate(),
            Err(ProductError::AmountNotPositive)
        );
        assert_eq!(
            CreditInput::CreditLimit(Some(-rub(1))).validate(),
            Err(ProductError::NegativeLimit)
        );
        assert_eq!(
            CreditInput::GraceDays(Some(0)).validate(),
            Err(ProductError::DaysNotPositive)
        );
        assert_eq!(
            SavingsInput::PromoRate(Some(RateBp(10_001))).validate(),
            Err(ProductError::RateTooHigh(10_001))
        );
        assert_eq!(
            DeadlineInput::Title("  ".into()).validate(),
            Err(ProductError::EmptyTitle)
        );
        let op: ProductOp = serde_json::from_str(
            r#"{"op":"addRepayment","product":3,"edits":[{"field":"amount","value":150000},{"field":"date","value":"2026-07-10"}]}"#,
        )
        .unwrap();
        assert_eq!(
            op,
            ProductOp::AddRepayment {
                product: 3,
                edits: vec![
                    RepaymentInput::Amount(rub(1500)),
                    RepaymentInput::Date(d("2026-07-10"))
                ]
            }
        );
        let op: ProductOp =
            serde_json::from_str(r#"{"op":"bind","entry":9,"tranche":null}"#).unwrap();
        assert_eq!(
            op,
            ProductOp::Bind {
                entry: 9,
                tranche: None
            }
        );
    }
}
