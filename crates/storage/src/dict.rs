//! Dictionaries in the file: loading, resolving names to ids (creating missing items, D17),
//! editing and deleting.

use reestr_core::date::{format_iso, parse_iso};
use reestr_core::dict::{DEFAULT_CATEGORIES, RENT_CATEGORY, RENT_TITLE};
use reestr_core::{
    Bank, Category, CategoryGroup, DictInput, DictKind, DictOp, Kopecks, Participant, Product,
    ProductKind, RegularTemplate, Rounding, name_key,
};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Row as SqlRow, params};
use serde::Serialize;

use crate::error::StorageError;
use crate::history::{Recorder, Row, Table};
use crate::vault::Vault;

/// Every dictionary of the file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dictionaries {
    pub participants: Vec<Participant>,
    pub banks: Vec<Bank>,
    pub products: Vec<Product>,
    pub groups: Vec<CategoryGroup>,
    pub categories: Vec<Category>,
    pub templates: Vec<RegularTemplate>,
}

/// A dictionary item created on the fly from a typed or imported name (D17).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Created {
    pub kind: DictKind,
    pub name: String,
}

fn opt_text(value: Option<String>) -> Value {
    value.map_or(Value::Null, Value::Text)
}

fn opt_int(value: Option<i64>) -> Value {
    value.map_or(Value::Null, Value::Integer)
}

fn flag(value: bool) -> Value {
    Value::Integer(i64::from(value))
}

/// Trims and turns an empty string into `None`.
fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|v| !v.is_empty())
}

pub(crate) fn load(conn: &Connection) -> Result<Dictionaries, StorageError> {
    fn all<T>(
        conn: &Connection,
        sql: &str,
        map: impl FnMut(&SqlRow<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>, StorageError> {
        let mut stmt = conn.prepare_cached(sql)?;
        let rows = stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    Ok(Dictionaries {
        participants: all(
            conn,
            "SELECT id, name, sort_order, is_active FROM participant ORDER BY sort_order, id",
            |r| {
                Ok(Participant {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    sort_order: r.get(2)?,
                    is_active: r.get(3)?,
                })
            },
        )?,
        banks: all(
            conn,
            "SELECT id, name, short_name FROM bank ORDER BY name_key",
            |r| {
                Ok(Bank {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    short_name: r.get(2)?,
                })
            },
        )?,
        products: all(
            conn,
            "SELECT id, bank_id, participant_id, kind, short_name, full_name, cashback_rounding,
                    is_active, opened_at, closed_at, notes
             FROM product ORDER BY name_key",
            |r| {
                Ok(Product {
                    id: r.get(0)?,
                    bank_id: r.get(1)?,
                    participant_id: r.get(2)?,
                    kind: r
                        .get::<_, Option<String>>(3)?
                        .and_then(|k| ProductKind::from_code(&k)),
                    short_name: r.get(4)?,
                    full_name: r.get(5)?,
                    cashback_rounding: Rounding::from_code(&r.get::<_, String>(6)?)
                        .unwrap_or_default(),
                    is_active: r.get(7)?,
                    opened_at: r.get::<_, Option<String>>(8)?.and_then(|d| parse_iso(&d)),
                    closed_at: r.get::<_, Option<String>>(9)?.and_then(|d| parse_iso(&d)),
                    notes: r.get(10)?,
                })
            },
        )?,
        groups: all(
            conn,
            "SELECT id, name, sort_order FROM category_group ORDER BY sort_order, id",
            |r| {
                Ok(CategoryGroup {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    sort_order: r.get(2)?,
                })
            },
        )?,
        categories: all(
            conn,
            "SELECT id, name, group_id, is_regular_default, sort_order, is_active
             FROM category ORDER BY sort_order, id",
            |r| {
                Ok(Category {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    group_id: r.get(2)?,
                    is_regular_default: r.get(3)?,
                    sort_order: r.get(4)?,
                    is_active: r.get(5)?,
                })
            },
        )?,
        templates: all(
            conn,
            "SELECT id, title, category_id, participant_id, product_id, default_amount,
                    sort_order, is_active, exclude_from_rate
             FROM regular_template ORDER BY sort_order, id",
            |r| {
                Ok(RegularTemplate {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    category_id: r.get(2)?,
                    participant_id: r.get(3)?,
                    product_id: r.get(4)?,
                    default_amount: r.get::<_, Option<i64>>(5)?.map(Kopecks),
                    sort_order: r.get(6)?,
                    is_active: r.get(7)?,
                    exclude_from_rate: r.get(8)?,
                })
            },
        )?,
    })
}

/// Fills a new file with the default categories and groups. Not an undoable action: it is
/// the starting state of the file.
pub(crate) fn seed_defaults(conn: &Connection) -> Result<(), StorageError> {
    let mut groups: Vec<(&str, i64)> = Vec::new();
    for (order, (name, group)) in (1..).zip(DEFAULT_CATEGORIES) {
        let group_id = match group {
            None => None,
            Some(group) => match groups.iter().find(|(g, _)| g == group) {
                Some((_, id)) => Some(*id),
                None => {
                    let sort = i64::try_from(groups.len()).unwrap_or(0) + 1;
                    conn.execute(
                        "INSERT INTO category_group (name, name_key, sort_order) VALUES (?1, ?2, ?3)",
                        params![group, name_key(group), sort],
                    )?;
                    let id = conn.last_insert_rowid();
                    groups.push((group, id));
                    Some(id)
                }
            },
        };
        conn.execute(
            "INSERT INTO category (name, name_key, group_id, sort_order) VALUES (?1, ?2, ?3, ?4)",
            params![name, name_key(name), group_id, order],
        )?;
    }
    // Rent, row 3 of a month sheet (D16), stays out of «Ср. %» (D37).
    conn.execute(
        "INSERT INTO regular_template (title, category_id, sort_order, exclude_from_rate)
         VALUES (?1, (SELECT id FROM category WHERE name_key = ?2), 1, 1)",
        params![RENT_TITLE, name_key(RENT_CATEGORY)],
    )?;
    Ok(())
}

const fn table_of(kind: DictKind) -> Table {
    match kind {
        DictKind::Participant => Table::Participant,
        DictKind::Bank => Table::Bank,
        DictKind::Product => Table::Product,
        DictKind::Group => Table::CategoryGroup,
        DictKind::Category => Table::Category,
        DictKind::Template => Table::RegularTemplate,
    }
}

/// The column holding the display name.
const fn name_column(kind: DictKind) -> &'static str {
    match kind {
        DictKind::Product => "short_name",
        DictKind::Template => "title",
        _ => "name",
    }
}

const fn has_name_key(kind: DictKind) -> bool {
    !matches!(kind, DictKind::Template)
}

/// Resolves names to ids inside one recorded action, creating what is missing.
pub(crate) struct Resolver<'r, 'a> {
    rec: &'r Recorder<'a>,
    pub(crate) created: Vec<Created>,
}

impl<'r, 'a> Resolver<'r, 'a> {
    pub(crate) fn new(rec: &'r Recorder<'a>) -> Self {
        Self {
            rec,
            created: Vec::new(),
        }
    }

    pub(crate) fn rec(&self) -> &'r Recorder<'a> {
        self.rec
    }

    /// The id of the item with this name; created when missing. `None` for an empty name.
    pub(crate) fn resolve(
        &mut self,
        kind: DictKind,
        name: Option<&str>,
    ) -> Result<Option<i64>, StorageError> {
        let Some(name) = clean(name.map(str::to_owned)) else {
            return Ok(None);
        };
        if let Some(id) = find_by_name(self.rec.tx(), kind, &name)? {
            return Ok(Some(id));
        }
        let id = self
            .rec
            .insert(table_of(kind), &new_row(self.rec.tx(), kind, &name)?)?;
        self.created.push(Created { kind, name });
        Ok(Some(id))
    }
}

pub(crate) fn find_by_name(
    conn: &Connection,
    kind: DictKind,
    name: &str,
) -> Result<Option<i64>, StorageError> {
    if !has_name_key(kind) {
        return Ok(None);
    }
    Ok(conn
        .query_row(
            &format!(
                "SELECT id FROM {} WHERE name_key = ?1",
                table_of(kind).name()
            ),
            [name_key(name)],
            |r| r.get(0),
        )
        .optional()?)
}

pub(crate) fn next_sort_order(conn: &Connection, table: Table) -> Result<i64, StorageError> {
    Ok(conn.query_row(
        &format!(
            "SELECT COALESCE(MAX(sort_order), 0) + 1 FROM {}",
            table.name()
        ),
        [],
        |r| r.get(0),
    )?)
}

/// A new row with defaults and the given name.
fn new_row(conn: &Connection, kind: DictKind, name: &str) -> Result<Row, StorageError> {
    let text = |s: &str| Value::Text(s.to_owned());
    let key = text(&name_key(name));
    Ok(match kind {
        DictKind::Participant => vec![
            ("name", text(name)),
            ("name_key", key),
            (
                "sort_order",
                Value::Integer(next_sort_order(conn, Table::Participant)?),
            ),
            ("is_active", flag(true)),
        ],
        DictKind::Bank => vec![
            ("name", text(name)),
            ("name_key", key),
            ("short_name", Value::Null),
        ],
        DictKind::Product => vec![
            ("bank_id", Value::Null),
            ("participant_id", Value::Null),
            ("kind", Value::Null),
            ("short_name", text(name)),
            ("name_key", key),
            ("full_name", Value::Null),
            ("cashback_rounding", text(Rounding::default().code())),
            ("is_active", flag(true)),
            ("opened_at", Value::Null),
            ("closed_at", Value::Null),
            ("notes", Value::Null),
        ],
        DictKind::Group => vec![
            ("name", text(name)),
            ("name_key", key),
            (
                "sort_order",
                Value::Integer(next_sort_order(conn, Table::CategoryGroup)?),
            ),
        ],
        DictKind::Category => vec![
            ("name", text(name)),
            ("name_key", key),
            ("group_id", Value::Null),
            ("is_regular_default", flag(false)),
            (
                "sort_order",
                Value::Integer(next_sort_order(conn, Table::Category)?),
            ),
            ("is_active", flag(true)),
        ],
        DictKind::Template => vec![
            ("title", text(name)),
            ("category_id", Value::Null),
            ("participant_id", Value::Null),
            ("product_id", Value::Null),
            ("default_amount", Value::Null),
            (
                "sort_order",
                Value::Integer(next_sort_order(conn, Table::RegularTemplate)?),
            ),
            ("is_active", flag(true)),
            ("exclude_from_rate", flag(false)),
        ],
    })
}

/// Rows of other tables that refer to an item, by (table, column).
const fn references(kind: DictKind) -> &'static [(&'static str, &'static str)] {
    match kind {
        DictKind::Participant => &[
            ("entry", "participant_id"),
            ("product", "participant_id"),
            ("regular_template", "participant_id"),
            ("inflow", "participant_id"),
            ("monthly_plan", "participant_id"),
        ],
        DictKind::Bank => &[("product", "bank_id")],
        DictKind::Product => &[
            ("entry", "product_id"),
            ("regular_template", "product_id"),
            ("cashback_rule", "product_id"),
            ("credit_terms", "product_id"),
            ("grace_period", "product_id"),
            ("savings_terms", "product_id"),
            ("deadline", "product_id"),
            ("inflow", "product_id"),
        ],
        DictKind::Category => &[
            ("entry", "category_id"),
            ("regular_template", "category_id"),
            ("cashback_rule", "category_id"),
        ],
        DictKind::Group | DictKind::Template => &[],
    }
}

fn usage(conn: &Connection, kind: DictKind, id: i64) -> Result<i64, StorageError> {
    let mut count = 0;
    for (table, column) in references(kind) {
        count += conn.query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE {column} = ?1"),
            [id],
            |r| r.get::<_, i64>(0),
        )?;
    }
    Ok(count)
}

fn invalid(kind: DictKind, input: &DictInput) -> StorageError {
    StorageError::InvalidField {
        entity: kind.code(),
        field: input.field(),
    }
}

/// Applies one field edit to an existing item.
fn edit_item(
    resolver: &mut Resolver<'_, '_>,
    kind: DictKind,
    id: i64,
    input: DictInput,
) -> Result<(), StorageError> {
    let rec = resolver.rec();
    let table = table_of(kind);
    use DictInput as I;
    use DictKind as K;
    let (column, value): (&'static str, Value) = match (kind, input) {
        (_, I::Name(name)) => {
            let name = clean(Some(name)).ok_or(StorageError::EmptyName)?;
            if let Some(other) = find_by_name(rec.tx(), kind, &name)?
                && other != id
            {
                return Err(StorageError::DuplicateName(name));
            }
            if has_name_key(kind) {
                rec.update(table, id, "name_key", Value::Text(name_key(&name)))?;
            }
            (name_column(kind), Value::Text(name))
        }
        (K::Bank, I::ShortName(v)) => ("short_name", opt_text(clean(v))),
        (K::Product, I::FullName(v)) => ("full_name", opt_text(clean(v))),
        (K::Product, I::Notes(v)) => ("notes", opt_text(clean(v))),
        (K::Participant | K::Product | K::Category | K::Template, I::IsActive(v)) => {
            ("is_active", flag(v))
        }
        (K::Category, I::IsRegularDefault(v)) => ("is_regular_default", flag(v)),
        (K::Template, I::ExcludeFromRate(v)) => ("exclude_from_rate", flag(v)),
        (K::Participant | K::Group | K::Category | K::Template, I::SortOrder(v)) => {
            ("sort_order", Value::Integer(v))
        }
        (K::Category, I::Group(v)) => (
            "group_id",
            opt_int(resolver.resolve(K::Group, v.as_deref())?),
        ),
        (K::Product, I::Bank(v)) => ("bank_id", opt_int(resolver.resolve(K::Bank, v.as_deref())?)),
        (K::Product | K::Template, I::Participant(v)) => (
            "participant_id",
            opt_int(resolver.resolve(K::Participant, v.as_deref())?),
        ),
        (K::Template, I::Category(v)) => (
            "category_id",
            opt_int(resolver.resolve(K::Category, v.as_deref())?),
        ),
        (K::Template, I::Card(v)) => (
            "product_id",
            opt_int(resolver.resolve(K::Product, v.as_deref())?),
        ),
        (K::Product, I::Kind(v)) => ("kind", opt_text(v.map(|k| k.code().to_owned()))),
        (K::Product, I::Rounding(v)) => ("cashback_rounding", Value::Text(v.code().to_owned())),
        (K::Template, I::DefaultAmount(v)) => ("default_amount", opt_int(v.map(Kopecks::get))),
        (K::Product, I::OpenedAt(v)) => ("opened_at", opt_text(v.map(format_iso))),
        (K::Product, I::ClosedAt(v)) => ("closed_at", opt_text(v.map(format_iso))),
        (kind, input) => return Err(invalid(kind, &input)),
    };
    rec.update(table, id, column, value)?;
    Ok(())
}

fn delete_item(rec: &Recorder<'_>, kind: DictKind, id: i64) -> Result<(), StorageError> {
    let count = usage(rec.tx(), kind, id)?;
    if count > 0 {
        return Err(StorageError::InUse { count });
    }
    if kind == DictKind::Group {
        // Ungroup the categories first, through the log, so undo restores them.
        let members: Vec<i64> = {
            let mut stmt = rec
                .tx()
                .prepare("SELECT id FROM category WHERE group_id = ?1")?;
            stmt.query_map([id], |r| r.get(0))?
                .collect::<Result<_, _>>()?
        };
        for category in members {
            rec.update(Table::Category, category, "group_id", Value::Null)?;
        }
    }
    rec.delete(table_of(kind), id)
}

impl Vault {
    pub fn dictionaries(&self) -> Result<Dictionaries, StorageError> {
        load(&self.conn)
    }

    /// Edits a dictionary as one undoable action. Returns items created on the way, e.g. a
    /// bank typed into a product row.
    pub fn edit_dictionary(
        &mut self,
        kind: DictKind,
        ops: Vec<DictOp>,
    ) -> Result<Vec<Created>, StorageError> {
        self.record("dictionary", |rec| {
            let mut resolver = Resolver::new(rec);
            for op in ops {
                match op {
                    DictOp::Edit { id, edit } => edit_item(&mut resolver, kind, id, edit)?,
                    DictOp::Append { edits } => {
                        let name = edits
                            .iter()
                            .find_map(|e| match e {
                                DictInput::Name(n) => clean(Some(n.clone())),
                                _ => None,
                            })
                            .ok_or(StorageError::EmptyName)?;
                        if find_by_name(rec.tx(), kind, &name)?.is_some() {
                            return Err(StorageError::DuplicateName(name));
                        }
                        let id = rec.insert(table_of(kind), &new_row(rec.tx(), kind, &name)?)?;
                        for edit in edits {
                            if !matches!(edit, DictInput::Name(_)) {
                                edit_item(&mut resolver, kind, id, edit)?;
                            }
                        }
                    }
                    DictOp::Delete { ids } => {
                        for id in ids {
                            delete_item(rec, kind, id)?;
                        }
                    }
                }
            }
            Ok(resolver.created)
        })
    }
}
