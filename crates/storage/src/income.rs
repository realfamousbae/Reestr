//! Inflows, the income plan and the data of «Свод» (PLAN §5, §8; stage 3b).

use reestr_core::date::{format_iso, parse_iso};
use reestr_core::summary::{YearData, month_balance, plan_for, year_summary};
use reestr_core::{
    DictKind, Inflow, InflowId, InflowInput, InflowOp, Kopecks, Month, MonthBalance, MonthTotals,
    Plan, ProductError, YearSummary, month_totals,
};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Row as SqlRow, params};

use crate::dict::{Created, Resolver, load};
use crate::error::StorageError;
use crate::history::{Row, Table};
use crate::journal::entries_of;
use crate::vault::Vault;

const INFLOW_COLUMNS: &str = "id, month, date, participant_id, product_id, source, amount, notes";

fn read_inflow(r: &SqlRow<'_>) -> rusqlite::Result<Inflow> {
    let month: Month = r.get::<_, String>(1)?.parse().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(Inflow {
        id: r.get(0)?,
        month,
        date: r.get::<_, Option<String>>(2)?.and_then(|d| parse_iso(&d)),
        participant_id: r.get(3)?,
        product_id: r.get(4)?,
        source: r.get(5)?,
        amount: Kopecks(r.get(6)?),
        notes: r.get(7)?,
    })
}

pub(crate) fn inflows_of(conn: &Connection, month: Month) -> Result<Vec<Inflow>, StorageError> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {INFLOW_COLUMNS} FROM inflow WHERE month = ?1 ORDER BY date IS NULL, date, id"
    ))?;
    let rows = stmt
        .query_map([month.to_string()], read_inflow)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Every inflow of the file, oldest month first.
pub(crate) fn all_inflows(conn: &Connection) -> Result<Vec<Inflow>, StorageError> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {INFLOW_COLUMNS} FROM inflow ORDER BY month, date IS NULL, date, id"
    ))?;
    let rows = stmt
        .query_map([], read_inflow)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn inflow_total(conn: &Connection, month: Month) -> Result<Kopecks, StorageError> {
    Ok(Kopecks(conn.query_row(
        "SELECT COALESCE(SUM(amount), 0) FROM inflow WHERE month = ?1",
        [month.to_string()],
        |r| r.get(0),
    )?))
}

/// Months where the family plan was set, oldest first.
pub(crate) fn plans(conn: &Connection) -> Result<Vec<(Month, Kopecks)>, StorageError> {
    let mut stmt = conn.prepare_cached(
        "SELECT month, planned_income FROM monthly_plan WHERE participant_id IS NULL ORDER BY month",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(m, amount)| m.parse().ok().map(|m| (m, Kopecks(amount))))
        .collect())
}

fn opt_text(value: Option<String>) -> Value {
    value
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
        .map_or(Value::Null, Value::Text)
}

fn opt_int(value: Option<i64>) -> Value {
    value.map_or(Value::Null, Value::Integer)
}

fn inflow_value(
    resolver: &mut Resolver<'_, '_>,
    input: InflowInput,
) -> Result<(&'static str, Value), StorageError> {
    Ok(match input {
        InflowInput::Date(d) => (
            "date",
            d.map_or(Value::Null, |d| Value::Text(format_iso(d))),
        ),
        InflowInput::Source(s) => ("source", Value::Text(s.trim().to_owned())),
        InflowInput::Participant(p) => (
            "participant_id",
            opt_int(resolver.resolve(DictKind::Participant, p.as_deref())?),
        ),
        InflowInput::Card(c) => (
            "product_id",
            opt_int(resolver.resolve(DictKind::Product, c.as_deref())?),
        ),
        InflowInput::Amount(a) if a <= Kopecks::ZERO => {
            return Err(ProductError::AmountNotPositive.into());
        }
        InflowInput::Amount(a) => ("amount", Value::Integer(a.get())),
        InflowInput::Notes(n) => ("notes", opt_text(n)),
    })
}

fn append_inflow(
    resolver: &mut Resolver<'_, '_>,
    month: Month,
    edits: Vec<InflowInput>,
) -> Result<InflowId, StorageError> {
    let mut row: Row = vec![
        ("month", Value::Text(month.to_string())),
        ("date", Value::Null),
        ("participant_id", Value::Null),
        ("product_id", Value::Null),
        ("source", Value::Text(String::new())),
        ("amount", Value::Integer(0)),
        ("notes", Value::Null),
    ];
    for edit in edits {
        let (column, value) = inflow_value(resolver, edit)?;
        if let Some(cell) = row.iter_mut().find(|(c, _)| *c == column) {
            cell.1 = value;
        }
    }
    resolver.rec().insert(Table::Inflow, &row)
}

/// Every entry's totals of a year: the whole year and each month that has entries.
fn year_totals(
    conn: &Connection,
    year: i32,
) -> Result<(MonthTotals, Vec<(Month, MonthTotals)>), StorageError> {
    let d = load(conn)?;
    let mut all = Vec::new();
    let mut months = Vec::new();
    for m in 1..=12 {
        let Ok(month) = Month::new(year, m) else {
            continue;
        };
        let entries = entries_of(conn, month)?;
        if entries.is_empty() {
            continue;
        }
        months.push((
            month,
            month_totals(&entries, &d.categories, &d.groups, &d.templates),
        ));
        all.extend(entries);
    }
    Ok((
        month_totals(&all, &d.categories, &d.groups, &d.templates),
        months,
    ))
}

impl Vault {
    pub fn inflows(&self, month: Month) -> Result<Vec<Inflow>, StorageError> {
        inflows_of(&self.conn, month)
    }

    /// The month's inflows changed as one undoable action.
    pub fn apply_inflows(
        &mut self,
        month: Month,
        ops: Vec<InflowOp>,
    ) -> Result<Vec<Created>, StorageError> {
        self.record("inflows", |rec| {
            let mut resolver = Resolver::new(rec);
            for op in ops {
                match op {
                    InflowOp::Edit { id, edit } => {
                        let (column, value) = inflow_value(&mut resolver, edit)?;
                        rec.update(Table::Inflow, id, column, value)?;
                    }
                    InflowOp::Append { edits } => {
                        append_inflow(&mut resolver, month, edits)?;
                    }
                    InflowOp::Delete { ids } => {
                        for id in ids {
                            rec.delete(Table::Inflow, id)?;
                        }
                    }
                }
            }
            Ok(resolver.created)
        })
    }

    /// The month's plan: its own, or the one carried over from an earlier month.
    pub fn plan(&self, month: Month) -> Result<Option<Plan>, StorageError> {
        Ok(plan_for(&plans(&self.conn)?, month))
    }

    /// Sets the family's income plan from `month` on (it carries over until changed); `None`
    /// removes this month's own plan, so the earlier one applies again. One undoable action.
    pub fn set_plan(&mut self, month: Month, amount: Option<Kopecks>) -> Result<(), StorageError> {
        if amount.is_some_and(|a| a < Kopecks::ZERO) {
            return Err(ProductError::NegativeLimit.into());
        }
        self.record("plan", |rec| {
            let id: Option<i64> = rec
                .tx()
                .query_row(
                    "SELECT id FROM monthly_plan WHERE month = ?1 AND participant_id IS NULL",
                    params![month.to_string()],
                    |r| r.get(0),
                )
                .optional()?;
            match (id, amount) {
                (Some(id), Some(a)) => {
                    rec.update(
                        Table::MonthlyPlan,
                        id,
                        "planned_income",
                        Value::Integer(a.get()),
                    )?;
                }
                (Some(id), None) => rec.delete(Table::MonthlyPlan, id)?,
                (None, Some(a)) => {
                    rec.insert(
                        Table::MonthlyPlan,
                        &vec![
                            ("month", Value::Text(month.to_string())),
                            ("participant_id", Value::Null),
                            ("planned_income", Value::Integer(a.get())),
                        ],
                    )?;
                }
                (None, None) => {}
            }
            Ok(())
        })
    }

    /// «Приход / расход / дельта» of a month.
    pub fn month_balance(&self, month: Month) -> Result<MonthBalance, StorageError> {
        let d = load(&self.conn)?;
        let entries = entries_of(&self.conn, month)?;
        let totals = month_totals(&entries, &d.categories, &d.groups, &d.templates);
        Ok(month_balance(
            &totals,
            self.plan(month)?,
            inflow_total(&self.conn, month)?,
        ))
    }

    /// Years that have entries, oldest first.
    pub fn summary_years(&self) -> Result<Vec<i32>, StorageError> {
        let mut years: Vec<i32> = self.months()?.into_iter().map(Month::year).collect();
        years.dedup();
        Ok(years)
    }

    /// «Свод» of a year, by categories or by groups.
    pub fn year_summary(&self, year: i32, grouped: bool) -> Result<YearSummary, StorageError> {
        let (this_year, months) = year_totals(&self.conn, year)?;
        let (last_year, last_months) = year_totals(&self.conn, year - 1)?;
        let data = YearData {
            year: &this_year,
            months: &months,
        };
        let previous = YearData {
            year: &last_year,
            months: &last_months,
        };
        let plans = plans(&self.conn)?;
        let mut inflows = Vec::new();
        for m in 1..=12 {
            if let Ok(month) = Month::new(year, m) {
                inflows.push((month, inflow_total(&self.conn, month)?));
            }
        }
        Ok(year_summary(
            year,
            &data,
            (!last_months.is_empty()).then_some(&previous),
            |month| plan_for(&plans, month).map(|p| p.amount),
            |month| {
                inflows
                    .iter()
                    .find(|(m, _)| *m == month)
                    .map_or(Kopecks::ZERO, |(_, total)| *total)
            },
            grouped,
        ))
    }
}
