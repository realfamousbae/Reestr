//! Bank products in the file (PLAN §7): credit card terms, tranches and repayments, savings
//! terms, reminders. Everything owed or due is computed by `reestr_core::products` (D28).

use std::collections::HashMap;

use reestr_core::date::{format_iso, parse_iso};
use reestr_core::products::{
    available_check, credit_debt, deadlines, new_tranche_dates, repayment_tranche,
    tranche_of_entry, tranche_states,
};
use reestr_core::{
    AvailableCheck, CreditInput, CreditTerms, CustomDeadline, Deadline, DeadlineInput, DictKind,
    Entry, EntryEdit, GraceId, GracePeriod, Kopecks, ProductError, ProductId, ProductKind,
    ProductOp, RateBp, Repayment, RepaymentInput, SavingsInput, SavingsState, SavingsTerms,
    TrancheInput, TrancheState,
};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Row as SqlRow};
use serde::Serialize;
use time::{Date, Duration};

use crate::dict::{Created, Resolver};
use crate::error::StorageError;
use crate::history::{Recorder, Row, Table};
use crate::journal::{ENTRY_COLUMNS, get_entry, read_entry, write_changes};
use crate::vault::Vault;

/// Purchases bound to nothing are listed when they are this recent.
const RECENT_DAYS: i64 = 90;

fn date_of(text: String) -> rusqlite::Result<Date> {
    parse_iso(&text).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("not a date: {text}").into(),
        )
    })
}

fn opt_date(text: Option<String>) -> Option<Date> {
    text.and_then(|t| parse_iso(&t))
}

fn all<T>(
    conn: &Connection,
    sql: &str,
    map: impl FnMut(&SqlRow<'_>) -> rusqlite::Result<T>,
) -> Result<Vec<T>, StorageError> {
    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_tranche(r: &SqlRow<'_>) -> rusqlite::Result<GracePeriod> {
    Ok(GracePeriod {
        id: r.get(0)?,
        product_id: r.get(1)?,
        start: date_of(r.get(2)?)?,
        end: date_of(r.get(3)?)?,
        debt_adjustment: Kopecks(r.get(4)?),
        notes: r.get(5)?,
    })
}

const TRANCHE_COLUMNS: &str = "id, product_id, start_date, end_date, debt_adjustment, notes";

fn tranches(conn: &Connection) -> Result<Vec<GracePeriod>, StorageError> {
    all(
        conn,
        &format!("SELECT {TRANCHE_COLUMNS} FROM grace_period ORDER BY product_id, start_date, id"),
        read_tranche,
    )
}

fn get_tranche(conn: &Connection, id: GraceId) -> Result<GracePeriod, StorageError> {
    conn.query_row(
        &format!("SELECT {TRANCHE_COLUMNS} FROM grace_period WHERE id = ?1"),
        [id],
        read_tranche,
    )
    .optional()?
    .ok_or(StorageError::UnknownId(id))
}

fn repayments(conn: &Connection) -> Result<Vec<Repayment>, StorageError> {
    all(
        conn,
        "SELECT id, grace_period_id, date, amount, notes FROM repayment ORDER BY date, id",
        |r| {
            Ok(Repayment {
                id: r.get(0)?,
                grace_period_id: r.get(1)?,
                date: date_of(r.get(2)?)?,
                amount: Kopecks(r.get(3)?),
                notes: r.get(4)?,
            })
        },
    )
}

fn credit_terms(conn: &Connection) -> Result<HashMap<ProductId, CreditTerms>, StorageError> {
    let rows = all(
        conn,
        "SELECT product_id, credit_limit, default_grace_days, manual_available,
                manual_available_on
         FROM credit_terms",
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                CreditTerms {
                    credit_limit: r.get::<_, Option<i64>>(1)?.map(Kopecks),
                    default_grace_days: r.get(2)?,
                    manual_available: r.get::<_, Option<i64>>(3)?.map(Kopecks),
                    manual_available_on: opt_date(r.get(4)?),
                },
            ))
        },
    )?;
    Ok(rows.into_iter().collect())
}

fn savings_terms(conn: &Connection) -> Result<HashMap<ProductId, SavingsTerms>, StorageError> {
    let rows = all(
        conn,
        "SELECT product_id, promo_rate_bp, promo_closed_at, reopen_after_days FROM savings_terms",
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                SavingsTerms {
                    promo_rate: r.get::<_, Option<u32>>(1)?.map(RateBp),
                    promo_closed_at: opt_date(r.get(2)?),
                    reopen_after_days: r.get(3)?,
                },
            ))
        },
    )?;
    Ok(rows.into_iter().collect())
}

fn reminders(conn: &Connection) -> Result<Vec<CustomDeadline>, StorageError> {
    all(
        conn,
        "SELECT id, product_id, date, title, is_done FROM deadline ORDER BY date, id",
        |r| {
            Ok(CustomDeadline {
                id: r.get(0)?,
                product_id: r.get(1)?,
                date: date_of(r.get(2)?)?,
                title: r.get(3)?,
                is_done: r.get(4)?,
            })
        },
    )
}

/// Products of a kind, plus those that already have rows in `table`.
fn products_of(
    conn: &Connection,
    kinds: &[ProductKind],
    tables: &[&str],
) -> Result<Vec<ProductId>, StorageError> {
    let kinds = kinds
        .iter()
        .map(|k| format!("'{}'", k.code()))
        .collect::<Vec<_>>()
        .join(", ");
    let others = tables
        .iter()
        .map(|t| format!(" OR id IN (SELECT product_id FROM {t})"))
        .collect::<String>();
    all(
        conn,
        &format!("SELECT id FROM product WHERE kind IN ({kinds}){others} ORDER BY name_key"),
        |r| r.get(0),
    )
}

/// Every journal entry paid with these cards, any month.
fn entries_of_cards(conn: &Connection, cards: &[ProductId]) -> Result<Vec<Entry>, StorageError> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {ENTRY_COLUMNS} FROM entry WHERE product_id = ?1 ORDER BY date, id"
    ))?;
    let mut entries = Vec::new();
    for card in cards {
        let rows = stmt
            .query_map([card], read_entry)?
            .collect::<Result<Vec<_>, _>>()?;
        entries.extend(rows);
    }
    Ok(entries)
}

/// A purchase on a credit card and the tranche it is in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Purchase {
    pub entry: Entry,
    pub tranche: Option<GraceId>,
    /// Bound by hand rather than by its date.
    pub manual: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditCard {
    pub product_id: ProductId,
    pub terms: CreditTerms,
    pub tranches: Vec<GracePeriod>,
    /// In the order of `tranches`.
    pub states: Vec<TrancheState>,
    pub repayments: Vec<Repayment>,
    /// Purchases in unpaid tranches, and recent ones outside any tranche.
    pub purchases: Vec<Purchase>,
    pub debt: Kopecks,
    /// The entered balance against the computed one; `None` when there is nothing to compare.
    pub available: Option<AvailableCheck>,
    /// Purchases without a tranche: before the first one, or without a date.
    pub unbound: usize,
    /// Where a new repayment goes by default (D27).
    pub repayment_tranche: Option<GraceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavingsAccount {
    pub product_id: ProductId,
    pub terms: SavingsTerms,
    pub state: SavingsState,
}

/// The data of the «Продукты» and «Сроки» sheets and of the month's right blocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductsData {
    pub credit: Vec<CreditCard>,
    pub savings: Vec<SavingsAccount>,
    /// Debt on all credit cards.
    pub debt: Kopecks,
    pub deadlines: Vec<Deadline>,
    /// Reminders, done ones too.
    pub reminders: Vec<CustomDeadline>,
}

fn opt_int(value: Option<i64>) -> Value {
    value.map_or(Value::Null, Value::Integer)
}

fn opt_text(value: Option<String>) -> Value {
    value
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
        .map_or(Value::Null, Value::Text)
}

/// The id of the terms row of a product in `table`, created empty when missing.
fn terms_row(rec: &Recorder<'_>, table: Table, product: ProductId) -> Result<i64, StorageError> {
    let found: Option<i64> = rec
        .tx()
        .query_row(
            &format!("SELECT id FROM {} WHERE product_id = ?1", table.name()),
            [product],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = found {
        return Ok(id);
    }
    let exists: bool = rec.tx().query_row(
        "SELECT EXISTS (SELECT 1 FROM product WHERE id = ?1)",
        [product],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(StorageError::UnknownId(product));
    }
    let row: Row = table
        .columns()
        .iter()
        .map(|c| {
            let value = if *c == "product_id" {
                Value::Integer(product)
            } else {
                Value::Null
            };
            (*c, value)
        })
        .collect();
    rec.insert(table, &row)
}

fn tranche_value(
    input: TrancheInput,
    tranche: &GracePeriod,
) -> Result<(&'static str, Value), StorageError> {
    Ok(match input {
        TrancheInput::Start(start) => {
            if start > tranche.end {
                return Err(ProductError::EndBeforeStart.into());
            }
            ("start_date", Value::Text(format_iso(start)))
        }
        TrancheInput::End(end) => {
            if end < tranche.start {
                return Err(ProductError::EndBeforeStart.into());
            }
            ("end_date", Value::Text(format_iso(end)))
        }
        TrancheInput::Adjustment(adjustment) => {
            ("debt_adjustment", Value::Integer(adjustment.get()))
        }
        TrancheInput::Notes(notes) => ("notes", opt_text(notes)),
    })
}

fn add_tranche(
    rec: &Recorder<'_>,
    product: ProductId,
    edits: Vec<TrancheInput>,
    today: Date,
) -> Result<GraceId, StorageError> {
    let terms = credit_terms(rec.tx())?;
    let existing = tranches(rec.tx())?;
    let (mut start, mut end) = new_tranche_dates(&existing, product, terms.get(&product), today);
    let mut adjustment = Kopecks::ZERO;
    let mut notes = None;
    for edit in edits {
        match edit {
            TrancheInput::Start(d) => {
                // Keep the length when only the start is typed.
                end = d + (end - start);
                start = d;
            }
            TrancheInput::End(d) => end = d,
            TrancheInput::Adjustment(a) => adjustment = a,
            TrancheInput::Notes(n) => notes = n,
        }
    }
    if end < start {
        return Err(ProductError::EndBeforeStart.into());
    }
    let exists: bool = rec.tx().query_row(
        "SELECT EXISTS (SELECT 1 FROM product WHERE id = ?1)",
        [product],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(StorageError::UnknownId(product));
    }
    rec.insert(
        Table::GracePeriod,
        &vec![
            ("product_id", Value::Integer(product)),
            ("start_date", Value::Text(format_iso(start))),
            ("end_date", Value::Text(format_iso(end))),
            ("debt_adjustment", Value::Integer(adjustment.get())),
            ("notes", opt_text(notes)),
        ],
    )
}

fn delete_tranche(rec: &Recorder<'_>, id: GraceId) -> Result<(), StorageError> {
    get_tranche(rec.tx(), id)?;
    let count: i64 = rec.tx().query_row(
        "SELECT (SELECT COUNT(*) FROM repayment WHERE grace_period_id = ?1)
              + (SELECT COUNT(*) FROM entry WHERE grace_period_id = ?1)",
        [id],
        |r| r.get(0),
    )?;
    if count > 0 {
        return Err(StorageError::InUse { count });
    }
    rec.delete(Table::GracePeriod, id)
}

fn add_repayment(
    rec: &Recorder<'_>,
    product: ProductId,
    edits: Vec<RepaymentInput>,
    today: Date,
) -> Result<i64, StorageError> {
    let mut date = today;
    let mut amount = None;
    let mut tranche = None;
    let mut notes = None;
    for edit in edits {
        edit.validate()?;
        match edit {
            RepaymentInput::Date(d) => date = d,
            RepaymentInput::Amount(a) => amount = Some(a),
            RepaymentInput::Tranche(t) => tranche = Some(get_tranche(rec.tx(), t)?.id),
            RepaymentInput::Notes(n) => notes = n,
        }
    }
    let amount = amount.ok_or(ProductError::AmountNotPositive)?;
    let tranche = match tranche {
        Some(t) => t,
        None => {
            let all_tranches = tranches(rec.tx())?;
            let entries = entries_of_cards(rec.tx(), &[product])?;
            let states = tranche_states(&all_tranches, &entries, &repayments(rec.tx())?, today);
            repayment_tranche(&all_tranches, &states, product).ok_or(ProductError::NoTranche)?
        }
    };
    rec.insert(
        Table::Repayment,
        &vec![
            ("grace_period_id", Value::Integer(tranche)),
            ("date", Value::Text(format_iso(date))),
            ("amount", Value::Integer(amount.get())),
            ("notes", opt_text(notes)),
        ],
    )
}

fn add_reminder(
    resolver: &mut Resolver<'_, '_>,
    edits: Vec<DeadlineInput>,
    today: Date,
) -> Result<i64, StorageError> {
    let mut date = today;
    let mut title = None;
    let mut product = None;
    let mut done = false;
    for edit in edits {
        edit.validate()?;
        match edit {
            DeadlineInput::Date(d) => date = d,
            DeadlineInput::Title(t) => title = Some(t.trim().to_owned()),
            DeadlineInput::Card(c) => {
                product = resolver.resolve(DictKind::Product, c.as_deref())?
            }
            DeadlineInput::Done(d) => done = d,
        }
    }
    let title = title.ok_or(ProductError::EmptyTitle)?;
    resolver.rec().insert(
        Table::Deadline,
        &vec![
            ("product_id", opt_int(product)),
            ("date", Value::Text(format_iso(date))),
            ("title", Value::Text(title)),
            ("is_done", Value::Integer(i64::from(done))),
        ],
    )
}

fn apply_op(
    resolver: &mut Resolver<'_, '_>,
    op: ProductOp,
    today: Date,
) -> Result<(), StorageError> {
    let rec = resolver.rec();
    match op {
        ProductOp::CreditTerms { product, edit } => {
            edit.validate()?;
            let id = terms_row(rec, Table::CreditTerms, product)?;
            match edit {
                CreditInput::CreditLimit(v) => {
                    let value = opt_int(v.map(Kopecks::get));
                    rec.update(Table::CreditTerms, id, "credit_limit", value)?;
                }
                CreditInput::GraceDays(v) => {
                    let value = opt_int(v.map(i64::from));
                    rec.update(Table::CreditTerms, id, "default_grace_days", value)?;
                }
                CreditInput::ManualAvailable { amount, on } => {
                    let value = opt_int(amount.map(Kopecks::get));
                    rec.update(Table::CreditTerms, id, "manual_available", value)?;
                    let date = on.map_or(Value::Null, |d| Value::Text(format_iso(d)));
                    rec.update(Table::CreditTerms, id, "manual_available_on", date)?;
                }
            }
        }
        ProductOp::SavingsTerms { product, edit } => {
            edit.validate()?;
            let id = terms_row(rec, Table::SavingsTerms, product)?;
            let (column, value) = match edit {
                SavingsInput::PromoRate(v) => ("promo_rate_bp", opt_int(v.map(|r| i64::from(r.0)))),
                SavingsInput::ClosedAt(v) => (
                    "promo_closed_at",
                    v.map_or(Value::Null, |d| Value::Text(format_iso(d))),
                ),
                SavingsInput::ReopenAfterDays(v) => {
                    ("reopen_after_days", opt_int(v.map(i64::from)))
                }
            };
            rec.update(Table::SavingsTerms, id, column, value)?;
        }
        ProductOp::AddTranche { product, edits } => {
            add_tranche(rec, product, edits, today)?;
        }
        ProductOp::EditTranche { id, edit } => {
            let tranche = get_tranche(rec.tx(), id)?;
            let (column, value) = tranche_value(edit, &tranche)?;
            rec.update(Table::GracePeriod, id, column, value)?;
        }
        ProductOp::DeleteTranches { ids } => {
            for id in ids {
                delete_tranche(rec, id)?;
            }
        }
        ProductOp::AddRepayment { product, edits } => {
            add_repayment(rec, product, edits, today)?;
        }
        ProductOp::EditRepayment { id, edit } => {
            edit.validate()?;
            let (column, value) = match edit {
                RepaymentInput::Date(d) => ("date", Value::Text(format_iso(d))),
                RepaymentInput::Amount(a) => ("amount", Value::Integer(a.get())),
                RepaymentInput::Tranche(t) => (
                    "grace_period_id",
                    Value::Integer(get_tranche(rec.tx(), t)?.id),
                ),
                RepaymentInput::Notes(n) => ("notes", opt_text(n)),
            };
            rec.update(Table::Repayment, id, column, value)?;
        }
        ProductOp::DeleteRepayments { ids } => {
            for id in ids {
                rec.delete(Table::Repayment, id)?;
            }
        }
        ProductOp::Bind { entry, tranche } => {
            let old = get_entry(rec.tx(), entry)?.ok_or(StorageError::UnknownId(entry))?;
            if let Some(t) = tranche {
                get_tranche(rec.tx(), t)?;
            }
            let mut new = old.clone();
            new.apply(EntryEdit::Tranche(tranche))?;
            write_changes(rec, &old, &new)?;
        }
        ProductOp::AddDeadline { edits } => {
            add_reminder(resolver, edits, today)?;
        }
        ProductOp::EditDeadline { id, edit } => {
            edit.validate()?;
            let (column, value) = match edit {
                DeadlineInput::Date(d) => ("date", Value::Text(format_iso(d))),
                DeadlineInput::Title(t) => ("title", Value::Text(t.trim().to_owned())),
                DeadlineInput::Card(c) => (
                    "product_id",
                    opt_int(resolver.resolve(DictKind::Product, c.as_deref())?),
                ),
                DeadlineInput::Done(d) => ("is_done", Value::Integer(i64::from(d))),
            };
            resolver.rec().update(Table::Deadline, id, column, value)?;
        }
        ProductOp::DeleteDeadlines { ids } => {
            for id in ids {
                rec.delete(Table::Deadline, id)?;
            }
        }
    }
    Ok(())
}

impl Vault {
    /// Credit cards, savings accounts and deadlines as of `today`.
    pub fn products(&self, today: Date) -> Result<ProductsData, StorageError> {
        let conn = &self.conn;
        let cards = products_of(
            conn,
            &[ProductKind::Credit],
            &["credit_terms", "grace_period"],
        )?;
        let all_tranches = tranches(conn)?;
        let all_repayments = repayments(conn)?;
        let entries = entries_of_cards(conn, &cards)?;
        let states = tranche_states(&all_tranches, &entries, &all_repayments, today);
        let mut terms = credit_terms(conn)?;
        let recent = today - Duration::days(RECENT_DAYS);

        let credit = cards
            .iter()
            .map(|&card| {
                let card_tranches: Vec<GracePeriod> = all_tranches
                    .iter()
                    .filter(|t| t.product_id == card)
                    .cloned()
                    .collect();
                let card_states: Vec<TrancheState> = card_tranches
                    .iter()
                    .filter_map(|t| states.iter().find(|s| s.id == t.id).copied())
                    .collect();
                let unpaid = |id: GraceId| {
                    card_states
                        .iter()
                        .any(|s| s.id == id && s.remaining > Kopecks::ZERO)
                };
                let card_entries = entries.iter().filter(|e| e.product_id == Some(card));
                let mut unbound = 0;
                let mut purchases = Vec::new();
                for e in card_entries {
                    let tranche = tranche_of_entry(&all_tranches, e);
                    if tranche.is_none() {
                        unbound += 1;
                    }
                    let show = match tranche {
                        Some(t) => unpaid(t),
                        None => e.date.is_none_or(|d| d >= recent),
                    };
                    if show {
                        purchases.push(Purchase {
                            entry: e.clone(),
                            tranche,
                            manual: e.grace_period_id.is_some(),
                        });
                    }
                }
                let card_terms = terms.remove(&card).unwrap_or_default();
                CreditCard {
                    product_id: card,
                    available: available_check(
                        card,
                        &card_terms,
                        &all_tranches,
                        &entries,
                        &all_repayments,
                    ),
                    terms: card_terms,
                    repayments: all_repayments
                        .iter()
                        .filter(|r| card_tranches.iter().any(|t| t.id == r.grace_period_id))
                        .cloned()
                        .collect(),
                    debt: credit_debt(&card_states, Some(card)),
                    repayment_tranche: repayment_tranche(&card_tranches, &card_states, card),
                    tranches: card_tranches,
                    states: card_states,
                    purchases,
                    unbound,
                }
            })
            .collect();

        let mut all_savings = savings_terms(conn)?;
        let savings: Vec<SavingsAccount> = products_of(
            conn,
            &[ProductKind::Savings, ProductKind::Deposit],
            &["savings_terms"],
        )?
        .into_iter()
        .map(|product| {
            let terms = all_savings.remove(&product).unwrap_or_default();
            SavingsAccount {
                product_id: product,
                state: terms.state(today),
                terms,
            }
        })
        .collect();

        let reminders = reminders(conn)?;
        let savings_pairs: Vec<(ProductId, SavingsTerms)> = savings
            .iter()
            .map(|s| (s.product_id, s.terms.clone()))
            .collect();
        Ok(ProductsData {
            debt: credit_debt(&states, None),
            deadlines: deadlines(&all_tranches, &states, &savings_pairs, &reminders, today),
            credit,
            savings,
            reminders,
        })
    }

    /// Changes on the «Продукты» and «Сроки» sheets as one undoable action.
    pub fn apply_products(
        &mut self,
        ops: Vec<ProductOp>,
        today: Date,
    ) -> Result<Vec<Created>, StorageError> {
        self.record("products", |rec| {
            let mut resolver = Resolver::new(rec);
            for op in ops {
                apply_op(&mut resolver, op, today)?;
            }
            Ok(resolver.created)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn products_of_finds_kinds_and_terms() {
        let dir = tempfile::TempDir::new().unwrap();
        let vault = Vault::create(&dir.path().join("p.reestr"), "pw").unwrap();
        vault
            .conn
            .execute_batch(
                "INSERT INTO product (id, kind, short_name, name_key) VALUES
                    (1, 'credit', 'Б', 'б'), (2, 'debit', 'А', 'а'), (3, NULL, 'В', 'в');
                 INSERT INTO credit_terms (product_id) VALUES (3);",
            )
            .unwrap();
        let ids = products_of(
            &vault.conn,
            &[ProductKind::Credit],
            &["credit_terms", "grace_period"],
        )
        .unwrap();
        assert_eq!(ids, [1, 3]);
    }
}
