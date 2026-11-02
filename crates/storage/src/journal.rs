//! The journal in the file: month entries, batch edits, deletion, templates, undo/redo.

use std::collections::{BTreeSet, HashMap, HashSet};

use reestr_core::date::{format_iso, parse_iso};
use reestr_core::{
    Cashback, CashbackSource, DictKind, EditError, EditInput, Entry, EntryEdit, EntryId, JournalOp,
    Kopecks, Mcc, Month, ProductId, RateBp, Rounding, TemplateId, allocate, name_key, parse_mcc,
};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Row as SqlRow, params};
use serde::Serialize;

use crate::dict::{Created, Resolver};
use crate::error::StorageError;
use crate::history::{self, Direction, Recorder, Replayed, Row, Table, now};
use crate::vault::Vault;

/// What a batch did besides changing cells.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOutcome {
    /// Ids of rows appended by the batch, in order.
    pub appended: Vec<EntryId>,
    /// Dictionary items created from typed names (D17).
    pub created: Vec<Created>,
}

pub(crate) const ENTRY_COLUMNS: &str = "id, month, title, category_id, is_regular, \
     participant_id, date, amount, cashback_amount, cashback_rate_bp, cashback_source, \
     product_id, grace_period_id, mcc";

/// An MCC column of `r`; what was stored passed the core's check.
pub(crate) fn read_mcc(r: &SqlRow<'_>, index: usize) -> rusqlite::Result<Option<Mcc>> {
    r.get::<_, Option<String>>(index)?
        .map(|code| {
            code.parse().map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    index,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
        })
        .transpose()
}

pub(crate) fn mcc_value(mcc: Option<Mcc>) -> Value {
    mcc.map_or(Value::Null, |m| Value::Text(m.to_string()))
}

pub(crate) fn read_entry(r: &SqlRow<'_>) -> rusqlite::Result<Entry> {
    let month: Month = r.get::<_, String>(1)?.parse().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let source: Option<String> = r.get(10)?;
    let cashback = source
        .and_then(|s| CashbackSource::from_code(&s))
        .map(|source| -> rusqlite::Result<Cashback> {
            Ok(Cashback {
                amount: Kopecks(r.get::<_, Option<i64>>(8)?.unwrap_or(0)),
                rate: r
                    .get::<_, Option<i64>>(9)?
                    .and_then(|bp| u32::try_from(bp).ok())
                    .map(RateBp),
                source,
            })
        })
        .transpose()?;
    Ok(Entry {
        id: r.get(0)?,
        month,
        title: r.get(2)?,
        category_id: r.get(3)?,
        is_regular: r.get(4)?,
        participant_id: r.get(5)?,
        date: r.get::<_, Option<String>>(6)?.and_then(|d| parse_iso(&d)),
        amount: Kopecks(r.get(7)?),
        cashback,
        product_id: r.get(11)?,
        grace_period_id: r.get(12)?,
        mcc: read_mcc(r, 13)?,
    })
}

/// The columns of `entry` that edits change, as values.
fn entry_values(e: &Entry) -> [(&'static str, Value); 13] {
    let opt = |v: Option<i64>| v.map_or(Value::Null, Value::Integer);
    [
        ("title", Value::Text(e.title.clone())),
        ("title_key", Value::Text(name_key(&e.title))),
        ("category_id", opt(e.category_id)),
        ("is_regular", Value::Integer(i64::from(e.is_regular))),
        ("participant_id", opt(e.participant_id)),
        (
            "date",
            e.date.map_or(Value::Null, |d| Value::Text(format_iso(d))),
        ),
        ("amount", Value::Integer(e.amount.get())),
        ("cashback_amount", opt(e.cashback.map(|c| c.amount.get()))),
        (
            "cashback_rate_bp",
            opt(e.cashback.and_then(|c| c.rate).map(|r| i64::from(r.0))),
        ),
        (
            "cashback_source",
            e.cashback
                .map_or(Value::Null, |c| Value::Text(c.source.code().to_owned())),
        ),
        ("product_id", opt(e.product_id)),
        ("grace_period_id", opt(e.grace_period_id)),
        ("mcc", mcc_value(e.mcc)),
    ]
}

pub(crate) fn get_entry(conn: &Connection, id: EntryId) -> Result<Option<Entry>, StorageError> {
    Ok(conn
        .query_row(
            &format!("SELECT {ENTRY_COLUMNS} FROM entry WHERE id = ?1"),
            [id],
            read_entry,
        )
        .optional()?)
}

/// Entries of a month in row order.
pub(crate) fn entries_of(conn: &Connection, month: Month) -> Result<Vec<Entry>, StorageError> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {ENTRY_COLUMNS} FROM entry WHERE month = ?1 ORDER BY position, id"
    ))?;
    let entries = stmt
        .query_map([month.to_string()], read_entry)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

fn next_position(conn: &Connection, month: Month) -> Result<i64, StorageError> {
    Ok(conn.query_row(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM entry WHERE month = ?1",
        [month.to_string()],
        |r| r.get(0),
    )?)
}

/// The most recent other entry with the same title, for autofill.
fn last_with_title(
    conn: &Connection,
    title: &str,
    except: EntryId,
) -> Result<Option<Entry>, StorageError> {
    Ok(conn
        .query_row(
            &format!(
                "SELECT {ENTRY_COLUMNS} FROM entry WHERE title_key = ?1 AND id != ?2
                 ORDER BY month DESC, position DESC, id DESC LIMIT 1"
            ),
            params![name_key(title), except],
            read_entry,
        )
        .optional()?)
}

/// Turns typed names into ids, creating dictionary items when needed.
fn resolve_edit(
    resolver: &mut Resolver<'_, '_>,
    input: EditInput,
) -> Result<EntryEdit, StorageError> {
    Ok(match input {
        EditInput::Title(t) => EntryEdit::Title(t),
        EditInput::Category(c) => {
            EntryEdit::Category(resolver.resolve(DictKind::Category, c.as_deref())?)
        }
        EditInput::IsRegular(v) => EntryEdit::IsRegular(v),
        EditInput::Participant(p) => {
            EntryEdit::Participant(resolver.resolve(DictKind::Participant, p.as_deref())?)
        }
        EditInput::Date(d) => EntryEdit::Date(d),
        EditInput::Amount(a) => EntryEdit::Amount(a),
        EditInput::Rate(r) => EntryEdit::Rate(r),
        EditInput::Cashback(c) => EntryEdit::Cashback(c),
        EditInput::Card(c) => {
            EntryEdit::Product(resolver.resolve(DictKind::Product, c.as_deref())?)
        }
        EditInput::Mcc(m) => {
            EntryEdit::Mcc(parse_mcc(m.as_deref()).map_err(EditError::InvalidMcc)?)
        }
    })
}

/// Applies edits to an entry in memory with the journal rules: autofill from the last entry
/// with the same title, a category's «регулярная по умолчанию», cashback by the card's
/// rounding. Cashback is recomputed only when its inputs change (amount, card, category, MCC, a
/// cleared `%` or «Кэшбэк» cell), so an unrelated edit never moves an old cashback. A cashback
/// that was not typed then follows the month's rules; [`reallocate`] fills it in.
pub(crate) fn apply_edits(
    conn: &Connection,
    entry: &mut Entry,
    edits: Vec<EntryEdit>,
) -> Result<(), StorageError> {
    let mut title_typed = false;
    let mut category_typed = false;
    let mut cashback_inputs = false;
    for edit in edits {
        title_typed |= matches!(edit, EntryEdit::Title(_));
        category_typed |= matches!(edit, EntryEdit::Category(Some(_)));
        cashback_inputs |= matches!(
            edit,
            EntryEdit::Amount(_)
                | EntryEdit::Rate(_)
                | EntryEdit::Cashback(None)
                | EntryEdit::Product(_)
                | EntryEdit::Category(_)
                | EntryEdit::Mcc(_)
        );
        entry.apply(edit)?;
    }
    if title_typed
        && !entry.title.is_empty()
        && let Some(last) = last_with_title(conn, &entry.title, entry.id)?
    {
        let before = (entry.product_id, entry.category_id, entry.mcc);
        entry.autofill_from(&last);
        cashback_inputs |= (entry.product_id, entry.category_id, entry.mcc) != before;
    }
    if category_typed
        && !entry.is_regular
        && let Some(category) = entry.category_id
    {
        let regular: bool = conn
            .query_row(
                "SELECT is_regular_default FROM category WHERE id = ?1",
                [category],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(false);
        entry.is_regular = regular;
    }
    if cashback_inputs {
        if entry
            .cashback
            .is_none_or(|c| c.source == CashbackSource::Rule)
        {
            entry.cashback = Some(Cashback::pending_rule());
        }
        let rounding = rounding_of(conn, entry.product_id)?;
        entry.refresh_cashback(rounding);
        settle_cashback(conn, entry)?;
    }
    Ok(())
}

/// Gives an entry that follows the rules its cashback before it is written, as [`reallocate`]
/// would with the entry in its row (a new one at the bottom), so the rule placeholder never
/// reaches the file and an edit that changes nothing leaves no trace.
fn settle_cashback(conn: &Connection, entry: &mut Entry) -> Result<(), StorageError> {
    if !entry
        .cashback
        .is_some_and(|c| c.source == CashbackSource::Rule)
    {
        return Ok(());
    }
    let rules = crate::rules::rules_of(conn, entry.month)?;
    if rules.is_empty() {
        entry.cashback = None;
        return Ok(());
    }
    let mut entries = entries_of(conn, entry.month)?;
    match entries.iter().position(|e| e.id == entry.id) {
        Some(i) => entries[i] = entry.clone(),
        None => entries.push(entry.clone()),
    }
    let roundings = roundings(conn)?;
    let allocation = allocate(&entries, &rules, |p| {
        roundings.get(&p).copied().unwrap_or_default()
    });
    entry.cashback = allocation
        .cashback
        .into_iter()
        .find_map(|(id, c)| (id == entry.id).then_some(c))
        .flatten();
    Ok(())
}

/// Each card's rounding (D13).
pub(crate) fn roundings(conn: &Connection) -> Result<HashMap<ProductId, Rounding>, StorageError> {
    let mut stmt = conn.prepare_cached("SELECT id, cashback_rounding FROM product")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(id, code)| (id, Rounding::from_code(&code).unwrap_or_default()))
        .collect())
}

/// Recomputes the rule cashback of a month in journal order (D21) and writes what changed
/// through the log. Returns how many entries changed.
pub(crate) fn reallocate(rec: &Recorder<'_>, month: Month) -> Result<usize, StorageError> {
    let entries = entries_of(rec.tx(), month)?;
    let rules = crate::rules::rules_of(rec.tx(), month)?;
    let roundings = roundings(rec.tx())?;
    let allocation = allocate(&entries, &rules, |p| {
        roundings.get(&p).copied().unwrap_or_default()
    });
    let mut changed = 0;
    for (id, cashback) in allocation.cashback {
        let Some(old) = entries.iter().find(|e| e.id == id) else {
            continue;
        };
        if old.cashback != cashback {
            let new = Entry {
                cashback,
                ..old.clone()
            };
            write_changes(rec, old, &new)?;
            changed += 1;
        }
    }
    Ok(changed)
}

pub(crate) fn rounding_of(
    conn: &Connection,
    product_id: Option<i64>,
) -> Result<Rounding, StorageError> {
    let Some(id) = product_id else {
        return Ok(Rounding::default());
    };
    let code: Option<String> = conn
        .query_row(
            "SELECT cashback_rounding FROM product WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(code
        .and_then(|c| Rounding::from_code(&c))
        .unwrap_or_default())
}

/// Writes the differences between two versions of an entry through the log.
pub(crate) fn write_changes(
    rec: &Recorder<'_>,
    old: &Entry,
    new: &Entry,
) -> Result<bool, StorageError> {
    let mut changed = false;
    for ((column, old_value), (_, new_value)) in
        entry_values(old).into_iter().zip(entry_values(new))
    {
        if old_value != new_value {
            changed |= rec.update(Table::Entry, new.id, column, new_value)?;
        }
    }
    if changed {
        rec.tx().execute(
            "UPDATE entry SET updated_at = ?1 WHERE id = ?2",
            params![now(), new.id],
        )?;
    }
    Ok(changed)
}

/// Inserts a new entry at the bottom of its month.
pub(crate) fn insert_entry(
    rec: &Recorder<'_>,
    entry: &Entry,
    import_key: Option<String>,
) -> Result<EntryId, StorageError> {
    let ts = now();
    let mut row: Row = vec![
        ("month", Value::Text(entry.month.to_string())),
        (
            "position",
            Value::Integer(next_position(rec.tx(), entry.month)?),
        ),
    ];
    row.extend(entry_values(entry));
    row.push(("notes", Value::Null));
    row.push(("import_key", import_key.map_or(Value::Null, Value::Text)));
    row.push(("created_at", Value::Text(ts.clone())));
    row.push(("updated_at", Value::Text(ts)));
    rec.insert(Table::Entry, &row)
}

impl Vault {
    /// Runs `action` as one undoable user action. An action that changed nothing leaves no
    /// trace in the log and in the undo stack.
    pub(crate) fn record<T>(
        &mut self,
        label: &str,
        action: impl FnOnce(&Recorder<'_>) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let tx = self.conn.transaction()?;
        let (result, op_id) = {
            let rec = Recorder::begin(&tx, label)?;
            (action(&rec)?, rec.op_id())
        };
        let changes: i64 = tx.query_row(
            "SELECT COUNT(*) FROM change_log WHERE op_id = ?1",
            [op_id],
            |r| r.get(0),
        )?;
        if changes == 0 {
            tx.execute("DELETE FROM change_op WHERE id = ?1", [op_id])?;
        }
        tx.commit()?;
        if changes > 0 {
            self.history.push(op_id);
        }
        Ok(result)
    }

    /// Months that have entries, oldest first.
    pub fn months(&self) -> Result<Vec<Month>, StorageError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT DISTINCT month FROM entry ORDER BY month")?;
        let months = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(months.iter().filter_map(|m| m.parse().ok()).collect())
    }

    /// Entries of a month in row order.
    pub fn entries(&self, month: Month) -> Result<Vec<Entry>, StorageError> {
        entries_of(&self.conn, month)
    }

    /// Distinct titles, most recently used first: the history for autocomplete.
    pub fn title_history(&self, limit: usize) -> Result<Vec<String>, StorageError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT title, title_key FROM entry WHERE title_key != ''
             ORDER BY month DESC, position DESC, id DESC",
        )?;
        let mut seen = HashSet::new();
        let mut titles = Vec::new();
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let key: String = row.get(1)?;
            if seen.insert(key) {
                titles.push(row.get(0)?);
                if titles.len() == limit {
                    break;
                }
            }
        }
        Ok(titles)
    }

    /// Applies typed cells, pastes and fills of one month as one undoable action.
    pub fn apply(
        &mut self,
        month: Month,
        ops: Vec<JournalOp>,
    ) -> Result<ApplyOutcome, StorageError> {
        let label = if ops.iter().any(|o| matches!(o, JournalOp::Append { .. })) {
            "append"
        } else {
            "edit"
        };
        self.record(label, |rec| {
            let mut resolver = Resolver::new(rec);
            let mut appended = Vec::new();
            for op in ops {
                match op {
                    JournalOp::Edit { id, edit } => {
                        let old = get_entry(rec.tx(), id)?.ok_or(StorageError::UnknownId(id))?;
                        let mut new = old.clone();
                        let edit = resolve_edit(&mut resolver, edit)?;
                        apply_edits(rec.tx(), &mut new, vec![edit])?;
                        write_changes(rec, &old, &new)?;
                    }
                    JournalOp::Append { edits } => {
                        let mut entry = Entry::new(0, month);
                        let edits = edits
                            .into_iter()
                            .map(|e| resolve_edit(&mut resolver, e))
                            .collect::<Result<Vec<_>, _>>()?;
                        apply_edits(rec.tx(), &mut entry, edits)?;
                        appended.push(insert_entry(rec, &entry, None)?);
                    }
                }
            }
            reallocate(rec, month)?;
            Ok(ApplyOutcome {
                appended,
                created: resolver.created,
            })
        })
    }

    /// Deletes entries as one undoable action. Returns how many were deleted.
    pub fn delete_entries(&mut self, ids: &[EntryId]) -> Result<usize, StorageError> {
        self.record("delete", |rec| {
            let mut deleted = 0;
            let mut months = BTreeSet::new();
            for id in ids {
                if let Some(entry) = get_entry(rec.tx(), *id)? {
                    rec.delete(Table::Entry, *id)?;
                    months.insert(entry.month);
                    deleted += 1;
                }
            }
            // A deleted row may free a limit for the rows below it.
            for month in months {
                reallocate(rec, month)?;
            }
            Ok(deleted)
        })
    }

    /// Adds entries from regular payment templates at the bottom of the month.
    pub fn add_from_templates(
        &mut self,
        month: Month,
        ids: &[TemplateId],
    ) -> Result<Vec<EntryId>, StorageError> {
        let templates = self.dictionaries()?.templates;
        self.record("templates", |rec| {
            let mut appended = Vec::new();
            for id in ids {
                let t = templates
                    .iter()
                    .find(|t| t.id == *id)
                    .ok_or(StorageError::UnknownId(*id))?;
                let mut entry = Entry::new(0, month);
                let mut edits = vec![
                    EntryEdit::Title(t.title.clone()),
                    EntryEdit::Category(t.category_id),
                    EntryEdit::Participant(t.participant_id),
                    EntryEdit::Product(t.product_id),
                    EntryEdit::IsRegular(true),
                ];
                if let Some(amount) = t.default_amount {
                    edits.push(EntryEdit::Amount(amount));
                }
                apply_edits(rec.tx(), &mut entry, edits)?;
                appended.push(insert_entry(rec, &entry, None)?);
            }
            reallocate(rec, month)?;
            Ok(appended)
        })
    }

    pub fn can_undo(&self) -> bool {
        self.history.next(Direction::Undo).is_some()
    }

    pub fn can_redo(&self) -> bool {
        self.history.next(Direction::Redo).is_some()
    }

    /// Undoes the last action of this session; `None` when there is nothing to undo.
    pub fn undo(&mut self) -> Result<Option<Replayed>, StorageError> {
        self.replay(Direction::Undo)
    }

    /// Redoes the last undone action.
    pub fn redo(&mut self) -> Result<Option<Replayed>, StorageError> {
        self.replay(Direction::Redo)
    }

    fn replay(&mut self, direction: Direction) -> Result<Option<Replayed>, StorageError> {
        let Some(op_id) = self.history.next(direction) else {
            return Ok(None);
        };
        let tx = self.conn.transaction()?;
        let replayed = history::replay(&tx, op_id, direction)?;
        tx.commit()?;
        self.history.moved(direction);
        Ok(Some(replayed))
    }
}
