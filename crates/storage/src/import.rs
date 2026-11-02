//! Applying an imported workbook to the file (PLAN §9): dictionaries first, then entries.
//! The whole import is one undoable action; rows imported before are skipped.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use reestr_core::import::{
    CardGuess, CardSetup, MonthCheck, Workbook, check_sheet, guess_card, import_keys,
};
use reestr_core::{DictKind, Entry, EntryEdit, Month, Rounding, name_key};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::dict::{Created, Resolver, load, next_sort_order};
use crate::error::StorageError;
use crate::history::Table;
use crate::journal::{insert_entry, reallocate, rounding_of};
use crate::vault::Vault;

/// A month of the workbook as the wizard shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewMonth {
    pub month: Month,
    pub sheet: String,
    pub rows: usize,
    /// Rows imported before; they will be skipped.
    pub already: usize,
    /// Our totals against the sheet's own totals.
    pub check: MonthCheck,
}

/// A card name found in the workbook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCard {
    pub short_name: String,
    pub rows: usize,
    /// Already in «Карты и счета»: its settings stay as they are.
    pub exists: bool,
    pub guess: CardGuess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewName {
    pub name: String,
    pub is_new: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub months: Vec<PreviewMonth>,
    /// «Служебный», then categories used in rows but missing from it.
    pub categories: Vec<PreviewName>,
    pub groups: Vec<PreviewName>,
    pub participants: Vec<PreviewName>,
    pub cards: Vec<PreviewCard>,
    pub warnings: Vec<reestr_core::import::Warning>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub added: usize,
    pub skipped: usize,
    pub created: Vec<Created>,
    pub months: Vec<Month>,
}

fn key_exists(conn: &Connection, key: &str) -> Result<bool, StorageError> {
    Ok(conn
        .query_row("SELECT 1 FROM entry WHERE import_key = ?1", [key], |_| {
            Ok(())
        })
        .optional()?
        .is_some())
}

/// Names in order of first appearance, without repeats (by name key).
fn distinct<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut seen = BTreeSet::new();
    names
        .into_iter()
        .filter(|n| seen.insert(name_key(n)))
        .collect()
}

impl Vault {
    /// What an import of `book` would do, without changing anything.
    pub fn import_preview(&self, book: &Workbook) -> Result<ImportPreview, StorageError> {
        let d = load(&self.conn)?;
        let keys =
            |names: Vec<&str>| -> HashSet<String> { names.into_iter().map(name_key).collect() };
        let category_keys = keys(d.categories.iter().map(|c| c.name.as_str()).collect());
        let group_keys = keys(d.groups.iter().map(|g| g.name.as_str()).collect());
        let participant_keys = keys(d.participants.iter().map(|p| p.name.as_str()).collect());
        let product_keys = keys(d.products.iter().map(|p| p.short_name.as_str()).collect());
        let rows = || book.months.iter().flat_map(|m| m.rows.iter());

        let mut months = Vec::new();
        for sheet in &book.months {
            let mut already = 0;
            for key in import_keys(sheet) {
                if key_exists(&self.conn, &key)? {
                    already += 1;
                }
            }
            months.push(PreviewMonth {
                month: sheet.month,
                sheet: sheet.sheet.clone(),
                rows: sheet.rows.len(),
                already,
                check: check_sheet(sheet, &book.categories, Rounding::FloorRub),
            });
        }

        let category_names = distinct(
            book.categories
                .iter()
                .map(String::as_str)
                .chain(rows().filter_map(|r| r.category.as_deref())),
        );
        let categories = category_names
            .into_iter()
            .map(|n| PreviewName {
                name: n.to_owned(),
                is_new: !category_keys.contains(&name_key(n)),
            })
            .collect();
        let groups = book
            .groups
            .iter()
            .map(|g| PreviewName {
                name: g.name.clone(),
                is_new: !group_keys.contains(&name_key(&g.name)),
            })
            .collect();
        let participant_names: Vec<String> = distinct(
            d.participants
                .iter()
                .map(|p| p.name.as_str())
                .chain(rows().filter_map(|r| r.participant.as_deref())),
        )
        .into_iter()
        .map(str::to_owned)
        .collect();
        let participants = participant_names
            .iter()
            .map(|n| PreviewName {
                name: n.clone(),
                is_new: !participant_keys.contains(&name_key(n)),
            })
            .collect();

        let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
        for card in rows().filter_map(|r| r.card.as_deref()) {
            counts
                .entry(name_key(card))
                .or_insert_with(|| (card.to_owned(), 0))
                .1 += 1;
        }
        let mut cards: Vec<PreviewCard> = counts
            .into_values()
            .map(|(name, rows)| PreviewCard {
                exists: product_keys.contains(&name_key(&name)),
                guess: guess_card(&name, &participant_names),
                short_name: name,
                rows,
            })
            .collect();
        cards.sort_by(|a, b| b.rows.cmp(&a.rows).then(a.short_name.cmp(&b.short_name)));

        Ok(ImportPreview {
            months,
            categories,
            groups,
            participants,
            cards,
            warnings: book.warnings.clone(),
        })
    }

    /// Imports `book` as one undoable action. `cards` carries what the user confirmed for
    /// card names; existing cards keep their settings, new fields only fill blanks.
    pub fn import_workbook(
        &mut self,
        book: &Workbook,
        cards: &[CardSetup],
    ) -> Result<ImportReport, StorageError> {
        self.record("import", |rec| {
            let mut resolver = Resolver::new(rec);
            let mut report = ImportReport::default();

            for name in &book.categories {
                resolver.resolve(DictKind::Category, Some(name))?;
            }
            for group in &book.groups {
                let group_id = resolver.resolve(DictKind::Group, Some(&group.name))?;
                for name in &group.categories {
                    if let Some(id) = resolver.resolve(DictKind::Category, Some(name))? {
                        fill_blank(rec, Table::Category, id, "group_id", group_id)?;
                    }
                }
            }
            for card in cards {
                let Some(id) = resolver.resolve(DictKind::Product, Some(&card.short_name))? else {
                    continue;
                };
                let bank = resolver.resolve(DictKind::Bank, card.bank.as_deref())?;
                fill_blank(rec, Table::Product, id, "bank_id", bank)?;
                let participant =
                    resolver.resolve(DictKind::Participant, card.participant.as_deref())?;
                fill_blank(rec, Table::Product, id, "participant_id", participant)?;
                if let Some(kind) = card.kind {
                    let current: Option<String> = rec.tx().query_row(
                        "SELECT kind FROM product WHERE id = ?1",
                        [id],
                        |r| r.get(0),
                    )?;
                    if current.is_none() {
                        rec.update(Table::Product, id, "kind", Value::Text(kind.code().into()))?;
                    }
                }
            }
            mark_rent(&mut resolver, book)?;

            let mut months = BTreeSet::new();
            for sheet in &book.months {
                for (row, key) in sheet.rows.iter().zip(import_keys(sheet)) {
                    if key_exists(rec.tx(), &key)? {
                        report.skipped += 1;
                        continue;
                    }
                    let mut entry = Entry::new(0, sheet.month);
                    let mut edits = vec![
                        EntryEdit::Title(row.title.clone()),
                        EntryEdit::Category(
                            resolver.resolve(DictKind::Category, row.category.as_deref())?,
                        ),
                        EntryEdit::IsRegular(row.is_regular),
                        EntryEdit::Participant(
                            resolver.resolve(DictKind::Participant, row.participant.as_deref())?,
                        ),
                        EntryEdit::Date(row.date),
                        EntryEdit::Amount(row.amount),
                        EntryEdit::Product(
                            resolver.resolve(DictKind::Product, row.card.as_deref())?,
                        ),
                        EntryEdit::Mcc(row.mcc),
                    ];
                    if let Some(rate) = row.rate {
                        edits.push(EntryEdit::Rate(Some(rate)));
                    } else if let Some(cashback) = row.cashback {
                        edits.push(EntryEdit::Cashback(Some(cashback)));
                    }
                    // The sheet is taken as it is: no autofill, no category defaults.
                    for edit in edits {
                        entry.apply(edit)?;
                    }
                    entry.refresh_cashback(rounding_of(rec.tx(), entry.product_id)?);
                    insert_entry(rec, &entry, Some(key))?;
                    report.added += 1;
                    months.insert(sheet.month);
                }
            }
            for month in &months {
                reallocate(rec, *month)?;
            }
            report.months = months.into_iter().collect();
            report.created = resolver.created;
            Ok(report)
        })
    }
}

/// Sets a reference column only when it is empty: an import never overrides what the user set.
/// Row 3 of a month sheet is rent (D16): its regular payment stays out of «Ср. %» (D37). A
/// payment of that name gets the mark; without one, it is made from the row.
fn mark_rent(resolver: &mut Resolver<'_, '_>, book: &Workbook) -> Result<(), StorageError> {
    let rec = resolver.rec();
    let mut done: HashSet<String> = HashSet::new();
    for row in book.months.iter().flat_map(|m| &m.rows) {
        let key = name_key(&row.title);
        if row.sheet_row != 3 || key.is_empty() || !done.insert(key.clone()) {
            continue;
        }
        let templates: Vec<(i64, String, bool)> = {
            let mut stmt = rec
                .tx()
                .prepare("SELECT id, title, exclude_from_rate FROM regular_template")?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<_, _>>()?
        };
        match templates
            .iter()
            .find(|(_, title, _)| name_key(title) == key)
        {
            Some((_, _, true)) => {}
            Some((id, _, false)) => {
                rec.update(
                    Table::RegularTemplate,
                    *id,
                    "exclude_from_rate",
                    Value::Integer(1),
                )?;
            }
            None => {
                let id_or_null = |id: Option<i64>| id.map_or(Value::Null, Value::Integer);
                let category = resolver.resolve(DictKind::Category, row.category.as_deref())?;
                let participant =
                    resolver.resolve(DictKind::Participant, row.participant.as_deref())?;
                let card = resolver.resolve(DictKind::Product, row.card.as_deref())?;
                rec.insert(
                    Table::RegularTemplate,
                    &vec![
                        ("title", Value::Text(row.title.trim().to_owned())),
                        ("category_id", id_or_null(category)),
                        ("participant_id", id_or_null(participant)),
                        ("product_id", id_or_null(card)),
                        ("default_amount", Value::Integer(row.amount.get())),
                        (
                            "sort_order",
                            Value::Integer(next_sort_order(rec.tx(), Table::RegularTemplate)?),
                        ),
                        ("is_active", Value::Integer(1)),
                        ("exclude_from_rate", Value::Integer(1)),
                    ],
                )?;
            }
        }
    }
    Ok(())
}

fn fill_blank(
    rec: &crate::history::Recorder<'_>,
    table: Table,
    id: i64,
    column: &'static str,
    value: Option<i64>,
) -> Result<(), StorageError> {
    let Some(value) = value else {
        return Ok(());
    };
    let current: Option<i64> = rec.tx().query_row(
        &format!("SELECT {column} FROM {} WHERE id = ?1", table.name()),
        [id],
        |r| r.get(0),
    )?;
    if current.is_none() {
        rec.update(table, id, column, Value::Integer(value))?;
    }
    Ok(())
}
