//! Cashback rules in the file: a month's rules with their usage, rule edits as undoable
//! actions, the recompute question (PLAN §6.3), card suggestions and their setting (D6).

use std::collections::HashMap;

use reestr_core::rules::validate;
use reestr_core::{
    CardSuggestion, Cashback, CashbackRule, CashbackSource, CategoryId, DictKind, EntryId, Kopecks,
    Month, ProductId, RateBp, RuleId, RuleInput, RuleOp, RuleUsage, allocate, name_key,
    rule_of_entry, suggest_cards,
};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Row as SqlRow, params};
use serde::Serialize;

use crate::dict::{Created, Resolver};
use crate::error::StorageError;
use crate::history::{Recorder, Row, Table};
use crate::journal::{entries_of, get_entry, reallocate, roundings, write_changes};
use crate::vault::Vault;

const RULE_COLUMNS: &str = "id, month, product_id, category_id, rate_bp, spend_cap, cashback_cap, \
     spent_adjustment, notes";

/// `meta` key of the card suggestion setting: "on" or "off"; absent until the user chooses.
const SUGGESTIONS_KEY: &str = "card_suggestions";

fn read_rule(r: &SqlRow<'_>) -> rusqlite::Result<CashbackRule> {
    let month: Month = r.get::<_, String>(1)?.parse().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(CashbackRule {
        id: r.get(0)?,
        month,
        product_id: r.get(2)?,
        category_id: r.get(3)?,
        rate: RateBp(r.get(4)?),
        spend_cap: r.get::<_, Option<i64>>(5)?.map(Kopecks),
        cashback_cap: r.get::<_, Option<i64>>(6)?.map(Kopecks),
        spent_adjustment: Kopecks(r.get(7)?),
        notes: r.get(8)?,
    })
}

/// Rules of a month in the order they were added.
pub(crate) fn rules_of(conn: &Connection, month: Month) -> Result<Vec<CashbackRule>, StorageError> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {RULE_COLUMNS} FROM cashback_rule WHERE month = ?1 ORDER BY id"
    ))?;
    let rules = stmt
        .query_map([month.to_string()], read_rule)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rules)
}

fn get_rule(conn: &Connection, id: RuleId) -> Result<CashbackRule, StorageError> {
    conn.query_row(
        &format!("SELECT {RULE_COLUMNS} FROM cashback_rule WHERE id = ?1"),
        [id],
        read_rule,
    )
    .optional()?
    .ok_or(StorageError::UnknownId(id))
}

/// A month's rules, what is used of them, and where the «copy» button takes rules from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthRules {
    pub rules: Vec<CashbackRule>,
    /// In the order of `rules`.
    pub usage: Vec<RuleUsage>,
    /// The latest earlier month that has rules.
    pub copy_from: Option<Month>,
}

/// What a rules action did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesOutcome {
    /// Cards and categories created from typed names (D17).
    pub created: Vec<Created>,
    /// Entries whose cashback amount changed.
    pub changed: usize,
}

/// The rule of the month with this card and category, other than `except`.
fn find_rule(
    conn: &Connection,
    month: Month,
    product: ProductId,
    category: Option<CategoryId>,
    except: Option<RuleId>,
) -> Result<Option<RuleId>, StorageError> {
    Ok(conn
        .query_row(
            "SELECT id FROM cashback_rule
             WHERE month = ?1 AND product_id = ?2 AND IFNULL(category_id, 0) = IFNULL(?3, 0)
               AND id != IFNULL(?4, 0)",
            params![month.to_string(), product, category, except],
            |r| r.get(0),
        )
        .optional()?)
}

/// A typed category; empty or «на всё» means a rule for everything.
fn resolve_category(
    resolver: &mut Resolver<'_, '_>,
    name: Option<&str>,
) -> Result<Option<CategoryId>, StorageError> {
    match name {
        Some(n) if name_key(n) != "на все" => resolver.resolve(DictKind::Category, Some(n)),
        _ => Ok(None),
    }
}

fn opt_int(value: Option<Kopecks>) -> Value {
    value.map_or(Value::Null, |k| Value::Integer(k.get()))
}

fn insert_rule(rec: &Recorder<'_>, rule: &CashbackRule) -> Result<RuleId, StorageError> {
    if find_rule(
        rec.tx(),
        rule.month,
        rule.product_id,
        rule.category_id,
        None,
    )?
    .is_some()
    {
        return Err(StorageError::DuplicateRule);
    }
    let row: Row = vec![
        ("month", Value::Text(rule.month.to_string())),
        ("product_id", Value::Integer(rule.product_id)),
        (
            "category_id",
            rule.category_id.map_or(Value::Null, Value::Integer),
        ),
        ("rate_bp", Value::Integer(i64::from(rule.rate.0))),
        ("spend_cap", opt_int(rule.spend_cap)),
        ("cashback_cap", opt_int(rule.cashback_cap)),
        (
            "spent_adjustment",
            Value::Integer(rule.spent_adjustment.get()),
        ),
        ("notes", rule.notes.clone().map_or(Value::Null, Value::Text)),
    ];
    rec.insert(Table::CashbackRule, &row)
}

fn edit_rule(
    resolver: &mut Resolver<'_, '_>,
    id: RuleId,
    input: RuleInput,
) -> Result<(), StorageError> {
    validate(&input)?;
    let rec = resolver.rec();
    let rule = get_rule(rec.tx(), id)?;
    let (column, value): (&'static str, Value) = match input {
        RuleInput::Card(name) => {
            let product = resolver
                .resolve(DictKind::Product, name.as_deref())?
                .ok_or(StorageError::EmptyName)?;
            if find_rule(rec.tx(), rule.month, product, rule.category_id, Some(id))?.is_some() {
                return Err(StorageError::DuplicateRule);
            }
            ("product_id", Value::Integer(product))
        }
        RuleInput::Category(name) => {
            let category = resolve_category(resolver, name.as_deref())?;
            if find_rule(rec.tx(), rule.month, rule.product_id, category, Some(id))?.is_some() {
                return Err(StorageError::DuplicateRule);
            }
            ("category_id", category.map_or(Value::Null, Value::Integer))
        }
        RuleInput::Rate(rate) => ("rate_bp", Value::Integer(i64::from(rate.0))),
        RuleInput::SpendCap(cap) => ("spend_cap", opt_int(cap)),
        RuleInput::CashbackCap(cap) => ("cashback_cap", opt_int(cap)),
        RuleInput::Adjustment(adjustment) => ("spent_adjustment", Value::Integer(adjustment.get())),
        RuleInput::Notes(notes) => (
            "notes",
            notes
                .map(|n| n.trim().to_owned())
                .filter(|n| !n.is_empty())
                .map_or(Value::Null, Value::Text),
        ),
    };
    rec.update(Table::CashbackRule, id, column, value)?;
    Ok(())
}

fn append_rule(
    resolver: &mut Resolver<'_, '_>,
    month: Month,
    edits: Vec<RuleInput>,
) -> Result<RuleId, StorageError> {
    let card = edits.iter().find_map(|e| match e {
        RuleInput::Card(name) => Some(name.clone()),
        _ => None,
    });
    let product = resolver
        .resolve(DictKind::Product, card.flatten().as_deref())?
        .ok_or(StorageError::EmptyName)?;
    let category = match edits.iter().find_map(|e| match e {
        RuleInput::Category(name) => Some(name.clone()),
        _ => None,
    }) {
        Some(name) => resolve_category(resolver, name.as_deref())?,
        None => None,
    };
    let id = insert_rule(
        resolver.rec(),
        &CashbackRule {
            id: 0,
            month,
            product_id: product,
            category_id: category,
            rate: RateBp(0),
            spend_cap: None,
            cashback_cap: None,
            spent_adjustment: Kopecks::ZERO,
            notes: None,
        },
    )?;
    for edit in edits {
        if !matches!(edit, RuleInput::Card(_) | RuleInput::Category(_)) {
            edit_rule(resolver, id, edit)?;
        }
    }
    Ok(id)
}

/// Copies the rules of `from` that `month` does not have yet.
fn copy_rules(rec: &Recorder<'_>, from: Month, month: Month) -> Result<(), StorageError> {
    for rule in rules_of(rec.tx(), from)? {
        if find_rule(rec.tx(), month, rule.product_id, rule.category_id, None)?.is_none() {
            insert_rule(
                rec,
                &CashbackRule {
                    month,
                    spent_adjustment: Kopecks::ZERO,
                    ..rule
                },
            )?;
        }
    }
    Ok(())
}

/// Applies rule operations of a month and recomputes its cashback. With `recompute`, entries
/// follow the new rules, and blank cashback that a rule now covers is filled in; without it,
/// rule cashback that would change is kept as typed cashback (PLAN §6.3).
fn run(
    rec: &Recorder<'_>,
    month: Month,
    ops: Vec<RuleOp>,
    recompute: bool,
) -> Result<RulesOutcome, StorageError> {
    let before: HashMap<EntryId, Option<Cashback>> = entries_of(rec.tx(), month)?
        .into_iter()
        .map(|e| (e.id, e.cashback))
        .collect();
    let mut resolver = Resolver::new(rec);
    for op in ops {
        match op {
            RuleOp::Edit { id, edit } => edit_rule(&mut resolver, id, edit)?,
            RuleOp::Append { edits } => {
                append_rule(&mut resolver, month, edits)?;
            }
            RuleOp::Delete { ids } => {
                for id in ids {
                    get_rule(rec.tx(), id)?;
                    rec.delete(Table::CashbackRule, id)?;
                }
            }
            RuleOp::CopyFrom { from } => copy_rules(rec, from, month)?,
        }
    }

    if recompute {
        let rules = rules_of(rec.tx(), month)?;
        for entry in entries_of(rec.tx(), month)? {
            if entry.cashback.is_none() && rule_of_entry(&rules, &entry).is_some() {
                let mut new = entry.clone();
                new.cashback = Some(Cashback::pending_rule());
                write_changes(rec, &entry, &new)?;
            }
        }
    }
    reallocate(rec, month)?;
    if !recompute {
        for entry in entries_of(rec.tx(), month)? {
            let old = before.get(&entry.id).copied().flatten();
            if let Some(old) = old
                && old.source == CashbackSource::Rule
                && Some(old) != entry.cashback
            {
                let mut kept = entry.clone();
                kept.cashback = Some(old.frozen());
                write_changes(rec, &entry, &kept)?;
            }
        }
    }

    let amount = |c: Option<Cashback>| c.map(|c| c.amount);
    let changed = entries_of(rec.tx(), month)?
        .iter()
        .filter(|e| amount(before.get(&e.id).copied().flatten()) != amount(e.cashback))
        .count();
    Ok(RulesOutcome {
        created: resolver.created,
        changed,
    })
}

impl Vault {
    /// The rules of a month with their usage.
    pub fn month_rules(&self, month: Month) -> Result<MonthRules, StorageError> {
        let rules = rules_of(&self.conn, month)?;
        let entries = entries_of(&self.conn, month)?;
        let roundings = roundings(&self.conn)?;
        let usage = allocate(&entries, &rules, |p| {
            roundings.get(&p).copied().unwrap_or_default()
        })
        .usage;
        let copy_from: Option<String> = self.conn.query_row(
            "SELECT MAX(month) FROM cashback_rule WHERE month < ?1",
            [month.to_string()],
            |r| r.get(0),
        )?;
        Ok(MonthRules {
            rules,
            usage,
            copy_from: copy_from.and_then(|m| m.parse().ok()),
        })
    }

    /// How many entries of the month would get another cashback if `ops` were applied with
    /// recomputing. Nothing is changed.
    pub fn rules_impact(&mut self, month: Month, ops: Vec<RuleOp>) -> Result<usize, StorageError> {
        let tx = self.conn.transaction()?;
        let changed = {
            let rec = Recorder::begin(&tx, "rules")?;
            run(&rec, month, ops, true)?.changed
        };
        tx.rollback()?;
        Ok(changed)
    }

    /// Edits the rules of a month as one undoable action.
    pub fn apply_rules(
        &mut self,
        month: Month,
        ops: Vec<RuleOp>,
        recompute: bool,
    ) -> Result<RulesOutcome, StorageError> {
        self.record("rules", |rec| run(rec, month, ops, recompute))
    }

    /// Cards to pay with for an entry of the month, best first (D6): for its category and
    /// amount, with the limits the other entries left. A new row has no category yet: then the
    /// cards' «на всё» rules count.
    pub fn card_suggestions(
        &self,
        month: Month,
        entry: Option<EntryId>,
    ) -> Result<Vec<CardSuggestion>, StorageError> {
        let entry = entry
            .map(|id| get_entry(&self.conn, id))
            .transpose()?
            .flatten();
        let category = entry.as_ref().and_then(|e| e.category_id);
        let amount = entry.as_ref().map(|e| e.amount);
        self.suggest(month, entry.map(|e| e.id), category, amount)
    }

    fn suggest(
        &self,
        month: Month,
        except: Option<EntryId>,
        category: Option<CategoryId>,
        amount: Option<Kopecks>,
    ) -> Result<Vec<CardSuggestion>, StorageError> {
        let rules = rules_of(&self.conn, month)?;
        if rules.is_empty() {
            return Ok(Vec::new());
        }
        let others: Vec<_> = entries_of(&self.conn, month)?
            .into_iter()
            .filter(|e| except != Some(e.id))
            .collect();
        let roundings = roundings(&self.conn)?;
        let rounding = |p: ProductId| roundings.get(&p).copied().unwrap_or_default();
        let usage = allocate(&others, &rules, rounding).usage;
        let amount = amount.filter(|a| *a > Kopecks::ZERO);
        let inactive: Vec<ProductId> = {
            let mut stmt = self
                .conn
                .prepare_cached("SELECT id FROM product WHERE is_active = 0")?;
            stmt.query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?
        };
        Ok(
            suggest_cards(&rules, &usage, month, category, amount, rounding)
                .into_iter()
                .filter(|s| !inactive.contains(&s.product_id))
                .collect(),
        )
    }

    pub fn card_suggestions_enabled(&self) -> Result<bool, StorageError> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                [SUGGESTIONS_KEY],
                |r| r.get(0),
            )
            .optional()?;
        match value.as_deref() {
            Some(v) => Ok(v == "on"),
            None => {
                Ok(self
                    .conn
                    .query_row("SELECT EXISTS (SELECT 1 FROM cashback_rule)", [], |r| {
                        r.get(0)
                    })?)
            }
        }
    }

    /// A setting, not an action: not in the undo history.
    pub fn set_card_suggestions(&mut self, enabled: bool) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![SUGGESTIONS_KEY, if enabled { "on" } else { "off" }],
        )?;
        Ok(())
    }
}
