//! The change log (PLAN §4.3) and undo/redo within a session.
//!
//! Every user action is one `change_op` row plus `change_log` rows for each changed field,
//! inserted row or deleted row. Undo replays the log of an op backwards with old values, redo
//! forwards with new values. Values are stored as JSON inside the encrypted file.

use rusqlite::types::Value;
use rusqlite::{OptionalExtension, Transaction, params, params_from_iter};
use serde_json::{Map, Value as Json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::error::StorageError;

/// Tables whose rows the log can restore. Column lists are the only SQL identifiers built
/// into statements, so they come from here and never from input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Table {
    Participant,
    Bank,
    Product,
    CategoryGroup,
    Category,
    Entry,
    RegularTemplate,
    CashbackRule,
    CreditTerms,
    GracePeriod,
    Repayment,
    SavingsTerms,
    Deadline,
    Inflow,
    MonthlyPlan,
}

impl Table {
    const ALL: [Self; 15] = [
        Self::Participant,
        Self::Bank,
        Self::Product,
        Self::CategoryGroup,
        Self::Category,
        Self::Entry,
        Self::RegularTemplate,
        Self::CashbackRule,
        Self::CreditTerms,
        Self::GracePeriod,
        Self::Repayment,
        Self::SavingsTerms,
        Self::Deadline,
        Self::Inflow,
        Self::MonthlyPlan,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Participant => "participant",
            Self::Bank => "bank",
            Self::Product => "product",
            Self::CategoryGroup => "category_group",
            Self::Category => "category",
            Self::Entry => "entry",
            Self::RegularTemplate => "regular_template",
            Self::CashbackRule => "cashback_rule",
            Self::CreditTerms => "credit_terms",
            Self::GracePeriod => "grace_period",
            Self::Repayment => "repayment",
            Self::SavingsTerms => "savings_terms",
            Self::Deadline => "deadline",
            Self::Inflow => "inflow",
            Self::MonthlyPlan => "monthly_plan",
        }
    }

    /// Every column except `id`.
    pub(crate) const fn columns(self) -> &'static [&'static str] {
        match self {
            Self::Participant => &["name", "name_key", "sort_order", "is_active"],
            Self::Bank => &["name", "name_key", "short_name"],
            Self::Product => &[
                "bank_id",
                "participant_id",
                "kind",
                "short_name",
                "name_key",
                "full_name",
                "cashback_rounding",
                "is_active",
                "opened_at",
                "closed_at",
                "notes",
            ],
            Self::CategoryGroup => &["name", "name_key", "sort_order"],
            Self::Category => &[
                "name",
                "name_key",
                "group_id",
                "is_regular_default",
                "sort_order",
                "is_active",
            ],
            Self::Entry => &[
                "month",
                "position",
                "title",
                "title_key",
                "category_id",
                "is_regular",
                "participant_id",
                "date",
                "amount",
                "cashback_amount",
                "cashback_rate_bp",
                "cashback_source",
                "product_id",
                "notes",
                "import_key",
                "created_at",
                "updated_at",
                "grace_period_id",
            ],
            Self::RegularTemplate => &[
                "title",
                "category_id",
                "participant_id",
                "product_id",
                "default_amount",
                "sort_order",
                "is_active",
                "exclude_from_rate",
            ],
            Self::CashbackRule => &[
                "month",
                "product_id",
                "category_id",
                "rate_bp",
                "spend_cap",
                "cashback_cap",
                "spent_adjustment",
                "notes",
            ],
            Self::CreditTerms => &["product_id", "credit_limit", "default_grace_days"],
            Self::GracePeriod => &[
                "product_id",
                "start_date",
                "end_date",
                "debt_adjustment",
                "notes",
            ],
            Self::Repayment => &["grace_period_id", "date", "amount", "notes"],
            Self::SavingsTerms => &[
                "product_id",
                "promo_rate_bp",
                "promo_closed_at",
                "reopen_after_days",
            ],
            Self::Deadline => &["product_id", "date", "title", "is_done"],
            Self::Inflow => &[
                "month",
                "date",
                "participant_id",
                "product_id",
                "source",
                "amount",
                "notes",
            ],
            Self::MonthlyPlan => &["month", "participant_id", "planned_income"],
        }
    }

    /// Rows of the table belong to a month, so undo can show that month.
    const fn has_month(self) -> bool {
        matches!(
            self,
            Self::Entry | Self::CashbackRule | Self::Inflow | Self::MonthlyPlan
        )
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }

    fn column(self, name: &str) -> Option<&'static str> {
        self.columns().iter().copied().find(|c| *c == name)
    }
}

/// A row as column → value, without `id`.
pub(crate) type Row = Vec<(&'static str, Value)>;

pub(crate) fn now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default()
}

fn to_json(value: &Value) -> Json {
    match value {
        Value::Null | Value::Blob(_) => Json::Null,
        Value::Integer(i) => Json::from(*i),
        // No REAL columns exist; money is INTEGER kopecks.
        Value::Real(_) => Json::Null,
        Value::Text(s) => Json::from(s.clone()),
    }
}

fn from_json(value: &Json) -> Value {
    match value {
        Json::Number(n) => n.as_i64().map_or(Value::Null, Value::Integer),
        Json::String(s) => Value::Text(s.clone()),
        Json::Bool(b) => Value::Integer(i64::from(*b)),
        Json::Null | Json::Array(_) | Json::Object(_) => Value::Null,
    }
}

fn row_json(row: &Row) -> String {
    let map: Map<String, Json> = row
        .iter()
        .map(|(c, v)| ((*c).to_owned(), to_json(v)))
        .collect();
    Json::Object(map).to_string()
}

/// Records the changes of one user action inside its transaction.
pub(crate) struct Recorder<'a> {
    tx: &'a Transaction<'a>,
    op_id: i64,
    ts: String,
}

impl<'a> Recorder<'a> {
    pub(crate) fn begin(tx: &'a Transaction<'a>, label: &str) -> Result<Self, StorageError> {
        let ts = now();
        tx.execute(
            "INSERT INTO change_op (ts, label) VALUES (?1, ?2)",
            params![ts, label],
        )?;
        Ok(Self {
            tx,
            op_id: tx.last_insert_rowid(),
            ts,
        })
    }

    pub(crate) fn op_id(&self) -> i64 {
        self.op_id
    }

    pub(crate) fn tx(&self) -> &Transaction<'a> {
        self.tx
    }

    fn log(
        &self,
        table: Table,
        id: i64,
        field: &str,
        old: Option<String>,
        new: Option<String>,
    ) -> Result<(), StorageError> {
        self.tx.execute(
            "INSERT INTO change_log (op_id, ts, entity, entity_id, field, old_value, new_value)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![self.op_id, self.ts, table.name(), id, field, old, new],
        )?;
        Ok(())
    }

    /// Inserts a row and returns its id.
    pub(crate) fn insert(&self, table: Table, row: &Row) -> Result<i64, StorageError> {
        insert_row(self.tx, table, None, row)?;
        let id = self.tx.last_insert_rowid();
        self.log(table, id, "*", None, Some(row_json(row)))?;
        Ok(id)
    }

    /// Sets one column; does nothing (and logs nothing) when the value is unchanged.
    pub(crate) fn update(
        &self,
        table: Table,
        id: i64,
        column: &'static str,
        value: Value,
    ) -> Result<bool, StorageError> {
        debug_assert!(table.column(column).is_some(), "{column}");
        let old: Value = self
            .tx
            .query_row(
                &format!("SELECT {column} FROM {} WHERE id = ?1", table.name()),
                [id],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(StorageError::UnknownId(id))?;
        if old == value {
            return Ok(false);
        }
        self.tx.execute(
            &format!("UPDATE {} SET {column} = ?1 WHERE id = ?2", table.name()),
            params![value, id],
        )?;
        self.log(
            table,
            id,
            column,
            Some(to_json(&old).to_string()),
            Some(to_json(&value).to_string()),
        )?;
        Ok(true)
    }

    /// Deletes a row, keeping a full copy in the log so undo can restore it.
    pub(crate) fn delete(&self, table: Table, id: i64) -> Result<(), StorageError> {
        let row = read_row(self.tx, table, id)?.ok_or(StorageError::UnknownId(id))?;
        self.tx
            .execute(&format!("DELETE FROM {} WHERE id = ?1", table.name()), [id])?;
        self.log(table, id, "*", Some(row_json(&row)), None)
    }
}

fn read_row(tx: &Transaction<'_>, table: Table, id: i64) -> Result<Option<Row>, StorageError> {
    let columns = table.columns();
    let sql = format!(
        "SELECT {} FROM {} WHERE id = ?1",
        columns.join(", "),
        table.name()
    );
    Ok(tx
        .query_row(&sql, [id], |r| {
            columns
                .iter()
                .enumerate()
                .map(|(i, c)| Ok((*c, r.get::<_, Value>(i)?)))
                .collect::<Result<Row, _>>()
        })
        .optional()?)
}

fn insert_row(
    tx: &Transaction<'_>,
    table: Table,
    id: Option<i64>,
    row: &Row,
) -> Result<(), StorageError> {
    let mut names: Vec<&str> = row.iter().map(|(c, _)| *c).collect();
    let mut values: Vec<Value> = row.iter().map(|(_, v)| v.clone()).collect();
    if let Some(id) = id {
        names.push("id");
        values.push(Value::Integer(id));
    }
    let placeholders = (1..=names.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({placeholders})",
        table.name(),
        names.join(", ")
    );
    tx.execute(&sql, params_from_iter(values))?;
    Ok(())
}

fn parse_row(table: Table, json: &str) -> Result<Row, StorageError> {
    let parsed: Json = serde_json::from_str(json).map_err(|_| StorageError::CorruptLog)?;
    let map = parsed.as_object().ok_or(StorageError::CorruptLog)?;
    map.iter()
        .map(|(k, v)| {
            table
                .column(k)
                .map(|c| (c, from_json(v)))
                .ok_or(StorageError::CorruptLog)
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Undo,
    Redo,
}

/// What an undo or redo did.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Replayed {
    /// The label of the action: "edit", "append", "delete", "templates", "dictionary", "import",
    /// "rules", "products", "inflows", "plan".
    pub label: String,
    /// Months (`YYYY-MM`) whose entries changed, so the UI can show the right sheet.
    pub months: Vec<String>,
}

/// Replays the log of `op_id` backwards (undo) or forwards (redo).
pub(crate) fn replay(
    tx: &Transaction<'_>,
    op_id: i64,
    direction: Direction,
) -> Result<Replayed, StorageError> {
    let order = if direction == Direction::Undo {
        "DESC"
    } else {
        "ASC"
    };
    let mut stmt = tx.prepare(&format!(
        "SELECT entity, entity_id, field, old_value, new_value FROM change_log
         WHERE op_id = ?1 ORDER BY id {order}"
    ))?;
    let changes = stmt
        .query_map([op_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut months: Vec<String> = Vec::new();
    for (entity, id, field, old, new) in changes {
        let table = Table::from_name(&entity).ok_or(StorageError::CorruptLog)?;
        let (from, to) = if direction == Direction::Undo {
            (new, old)
        } else {
            (old, new)
        };
        if table.has_month() {
            let month = match (&from, &to) {
                (_, Some(row)) | (Some(row), None) if field == "*" => parse_row(table, row)?
                    .into_iter()
                    .find_map(|(c, v)| match (c, v) {
                        ("month", Value::Text(m)) => Some(m),
                        _ => None,
                    }),
                _ => tx
                    .query_row(
                        &format!("SELECT month FROM {} WHERE id = ?1", table.name()),
                        [id],
                        |r| r.get(0),
                    )
                    .optional()?,
            };
            if let Some(month) = month
                && !months.contains(&month)
            {
                months.push(month);
            }
        }
        if field == "*" {
            match (from, to) {
                // The row should not exist afterwards.
                (Some(_), None) => {
                    tx.execute(&format!("DELETE FROM {} WHERE id = ?1", table.name()), [id])?;
                }
                // The row should exist afterwards, with its old id.
                (None, Some(row)) => insert_row(tx, table, Some(id), &parse_row(table, &row)?)?,
                _ => return Err(StorageError::CorruptLog),
            }
        } else {
            let column = table.column(&field).ok_or(StorageError::CorruptLog)?;
            let value: Json = to
                .as_deref()
                .map(serde_json::from_str)
                .transpose()
                .map_err(|_| StorageError::CorruptLog)?
                .unwrap_or(Json::Null);
            tx.execute(
                &format!("UPDATE {} SET {column} = ?1 WHERE id = ?2", table.name()),
                params![from_json(&value), id],
            )?;
        }
    }

    tx.execute(
        "UPDATE change_op SET undone = ?1 WHERE id = ?2",
        params![i64::from(direction == Direction::Undo), op_id],
    )?;
    let label = tx.query_row("SELECT label FROM change_op WHERE id = ?1", [op_id], |r| {
        r.get(0)
    })?;
    months.sort();
    Ok(Replayed { label, months })
}

/// What can be undone and redone in this session.
#[derive(Debug, Default)]
pub(crate) struct SessionHistory {
    undo: Vec<i64>,
    redo: Vec<i64>,
}

impl SessionHistory {
    /// A new action: it can be undone, and nothing undone before can be redone any more.
    pub(crate) fn push(&mut self, op_id: i64) {
        self.undo.push(op_id);
        self.redo.clear();
    }

    /// The op an undo (or redo) would replay. The stacks move only after the replay is
    /// committed, see [`SessionHistory::moved`].
    pub(crate) fn next(&self, direction: Direction) -> Option<i64> {
        match direction {
            Direction::Undo => self.undo.last().copied(),
            Direction::Redo => self.redo.last().copied(),
        }
    }

    pub(crate) fn moved(&mut self, direction: Direction) {
        let (from, to) = match direction {
            Direction::Undo => (&mut self.undo, &mut self.redo),
            Direction::Redo => (&mut self.redo, &mut self.undo),
        };
        if let Some(op_id) = from.pop() {
            to.push(op_id);
        }
    }
}
