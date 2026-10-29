//! Exchange with the phone (D39). The computer writes a copy of the main file for the phone
//! into the exchange folder (Yandex Disk); the phone works on that copy, adds rows and writes
//! its file back; «Забрать с телефона» takes the phone's rows into the main file. What happens
//! to each row is decided by [`reestr_core::exchange::plan_take`].
//!
//! A copy is the encrypted file byte for byte, so it opens with the same key: the main file
//! reads the phone's file with `ATTACH` and no password while the password is the same.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use reestr_core::date::parse_iso;
use reestr_core::exchange::{
    PHONE_KEY_PREFIX, PhoneChange, PhoneEntry, TakeStep, Taken, plan_take,
};
use reestr_core::{DictKind, Entry, EntryEdit, EntryId, Kopecks, Month};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;

use crate::dict::{Created, Resolver};
use crate::error::StorageError;
use crate::history::{Recorder, Table};
use crate::journal::{
    ENTRY_COLUMNS, apply_edits, insert_entry, read_entry, reallocate, write_changes,
};
use crate::vault::Vault;

/// What a file is for (D39). Stored as `meta.role`; a file without it is a main file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// The family's file on the computer.
    Main,
    /// A copy of the main file written for the phone.
    Snapshot,
    /// The phone's working file.
    Phone,
}

impl Role {
    const fn code(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Snapshot => "snapshot",
            Self::Phone => "phone",
        }
    }

    fn from_code(code: Option<&str>) -> Self {
        match code {
            Some("snapshot") => Self::Snapshot,
            Some("phone") => Self::Phone,
            _ => Self::Main,
        }
    }

    /// Whether an undoable action with this label may run in a file of this role. The phone
    /// adds and changes rows only (D40); a copy for the phone is not edited at all.
    pub(crate) fn allows(self, label: &str) -> Result<(), StorageError> {
        match self {
            Self::Main => Ok(()),
            Self::Phone
                if matches!(
                    label,
                    "append" | "edit" | "delete" | "templates" | PHONE_START
                ) =>
            {
                Ok(())
            }
            Self::Phone => Err(StorageError::ComputerOnly),
            Self::Snapshot => Err(StorageError::SnapshotFile),
        }
    }
}

/// The label of the action that moves the phone's rows into a new copy. Its changes are
/// already counted, so they are not marked as new phone changes.
pub(crate) const PHONE_START: &str = "phone-start";

/// The schema an exchange file is attached as.
const OTHER: &str = "other";

fn meta(conn: &Connection, schema: &str, key: &str) -> Result<Option<String>, StorageError> {
    debug_assert!(schema == "main" || schema == OTHER);
    Ok(conn
        .query_row(
            &format!("SELECT value FROM {schema}.meta WHERE key = ?1"),
            [key],
            |r| r.get(0),
        )
        .optional()?)
}

fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<(), StorageError> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub(crate) fn read_role(conn: &Connection) -> Result<Role, StorageError> {
    Ok(Role::from_code(meta(conn, "main", "role")?.as_deref()))
}

/// How many copies for the phone the main file has written; a copy carries its number.
fn snapshot_seq(conn: &Connection, schema: &str) -> Result<i64, StorageError> {
    Ok(meta(conn, schema, "snapshot_seq")?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0))
}

/// The two exchange files of a main file in the exchange folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExchangePaths {
    /// «Семья — для телефона.reestr»: the copy the computer writes.
    pub for_phone: PathBuf,
    /// «Семья — с телефона.reestr»: the file the phone writes.
    pub from_phone: PathBuf,
}

/// The exchange files of `main` in `dir`, named after it.
pub fn exchange_paths(main: &Path, dir: &Path) -> Option<ExchangePaths> {
    let stem = main.file_stem()?.to_str()?;
    Some(ExchangePaths {
        for_phone: dir.join(format!("{stem} — для телефона.reestr")),
        from_phone: dir.join(format!("{stem} — с телефона.reestr")),
    })
}

/// Copies a file so that the target is either the old or the whole new file, never a part:
/// the exchange folder is synced while it is written.
fn copy_whole(
    from: &Path,
    to: &Path,
    prepare: impl FnOnce(&Path) -> Result<(), StorageError>,
) -> Result<(), StorageError> {
    let name = to
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(StorageError::NotFound)?;
    let part = to.with_file_name(format!(".{name}.part"));
    let result = fs::copy(from, &part)
        .map_err(StorageError::from)
        .and_then(|_| prepare(&part))
        .and_then(|()| Ok(fs::rename(&part, to)?));
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

/// Attaches `path` as [`OTHER`] for `read`, then detaches it. Without a password the file
/// must have the key of `conn`: a copy of the same file with the same password.
fn with_attached<T>(
    conn: &Connection,
    path: &Path,
    password: Option<&str>,
    read: impl FnOnce(&Connection) -> Result<T, StorageError>,
) -> Result<T, StorageError> {
    if !path.is_file() {
        return Err(StorageError::NotFound);
    }
    let name = path.to_str().ok_or(StorageError::NotFound)?;
    match password {
        None => conn.execute("ATTACH DATABASE ?1 AS other", [name])?,
        Some(password) => conn.execute(
            "ATTACH DATABASE ?1 AS other KEY ?2",
            params![name, password],
        )?,
    };
    let result = conn
        .query_row("SELECT count(*) FROM other.sqlite_schema", [], |r| {
            r.get::<_, i64>(0)
        })
        .map_err(StorageError::from)
        .and_then(|_| read(conn));
    conn.execute_batch("DETACH DATABASE other")?;
    result
}

/// Checks that the attached file is a `role` file of the same main file as `conn`.
fn check_other(conn: &Connection, role: Role, wrong: StorageError) -> Result<(), StorageError> {
    if meta(conn, OTHER, "format")?.as_deref() != Some("reestr")
        || Role::from_code(meta(conn, OTHER, "role")?.as_deref()) != role
    {
        return Err(wrong);
    }
    if meta(conn, OTHER, "file_id")? != meta(conn, "main", "file_id")? {
        return Err(StorageError::OtherFile);
    }
    Ok(())
}

/// The phone's changes in `schema`, in the order they were first made.
fn phone_changes(conn: &Connection, schema: &str) -> Result<Vec<PhoneChange>, StorageError> {
    debug_assert!(schema == "main" || schema == OTHER);
    let mut stmt = conn.prepare(&format!(
        "SELECT c.key, c.rev, c.deleted, e.month, e.title, e.date, e.amount, k.name, p.name,
                d.short_name, e.is_regular
         FROM {schema}.phone_change c
         LEFT JOIN {schema}.entry e ON e.import_key = c.key
         LEFT JOIN {schema}.category k ON k.id = e.category_id
         LEFT JOIN {schema}.participant p ON p.id = e.participant_id
         LEFT JOIN {schema}.product d ON d.id = e.product_id
         ORDER BY c.id"
    ))?;
    let rows = stmt.query_map([], |r| {
        let deleted: bool = r.get(2)?;
        let month: Option<String> = r.get(3)?;
        let entry = match month.and_then(|m| m.parse::<Month>().ok()) {
            Some(month) if !deleted => Some(PhoneEntry {
                month,
                title: r.get(4)?,
                date: r.get::<_, Option<String>>(5)?.and_then(|d| parse_iso(&d)),
                amount: Kopecks(r.get(6)?),
                category: r.get(7)?,
                participant: r.get(8)?,
                card: r.get(9)?,
                is_regular: r.get(10)?,
            }),
            _ => None,
        };
        Ok(PhoneChange {
            key: r.get(0)?,
            rev: r.get(1)?,
            entry,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// What the file took from the phone, by key.
fn taken_of(conn: &Connection) -> Result<HashMap<String, (i64, Taken)>, StorageError> {
    let mut stmt = conn.prepare("SELECT key, id, rev, fingerprint FROM phone_taken")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            (
                r.get::<_, i64>(1)?,
                Taken {
                    rev: r.get(2)?,
                    fingerprint: r.get(3)?,
                },
            ),
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn name_of(conn: &Connection, sql: &str, id: Option<i64>) -> Result<Option<String>, StorageError> {
    let Some(id) = id else {
        return Ok(None);
    };
    Ok(conn.query_row(sql, [id], |r| r.get(0)).optional()?)
}

/// The row with this key in the file, as an entry and as it travels.
fn entry_by_key(conn: &Connection, key: &str) -> Result<Option<(Entry, PhoneEntry)>, StorageError> {
    let Some(entry) = conn
        .query_row(
            &format!("SELECT {ENTRY_COLUMNS} FROM entry WHERE import_key = ?1"),
            [key],
            read_entry,
        )
        .optional()?
    else {
        return Ok(None);
    };
    let row = PhoneEntry {
        month: entry.month,
        title: entry.title.clone(),
        date: entry.date,
        amount: entry.amount,
        category: name_of(
            conn,
            "SELECT name FROM category WHERE id = ?1",
            entry.category_id,
        )?,
        participant: name_of(
            conn,
            "SELECT name FROM participant WHERE id = ?1",
            entry.participant_id,
        )?,
        card: name_of(
            conn,
            "SELECT short_name FROM product WHERE id = ?1",
            entry.product_id,
        )?,
        is_regular: entry.is_regular,
    };
    Ok(Some((entry, row)))
}

/// Gives `entry` the content of a phone row. Names are resolved like typed ones (D17); the
/// cashback follows the month's rules of this file, as for a row typed here.
fn fill(
    resolver: &mut Resolver<'_, '_>,
    entry: &mut Entry,
    row: &PhoneEntry,
) -> Result<(), StorageError> {
    let edits = vec![
        EntryEdit::Category(resolver.resolve(DictKind::Category, row.category.as_deref())?),
        EntryEdit::Participant(
            resolver.resolve(DictKind::Participant, row.participant.as_deref())?,
        ),
        EntryEdit::Product(resolver.resolve(DictKind::Product, row.card.as_deref())?),
        EntryEdit::Date(row.date),
        EntryEdit::Amount(row.amount),
    ];
    apply_edits(resolver.rec().tx(), entry, edits)?;
    // Exactly as on the other side: no autofill from the history of this file.
    entry.title = row.title.trim().to_owned();
    entry.is_regular = row.is_regular;
    Ok(())
}

/// Applies one step of the plan to the file of `resolver`. Returns the month of the row.
fn carry_out(
    resolver: &mut Resolver<'_, '_>,
    step: TakeStep,
    change: &PhoneChange,
    current: Option<&Entry>,
) -> Result<Option<Month>, StorageError> {
    let rec = resolver.rec();
    match (step, &change.entry, current) {
        (TakeStep::Add, Some(row), _) => {
            let mut entry = Entry::new(0, row.month);
            fill(resolver, &mut entry, row)?;
            insert_entry(rec, &entry, Some(change.key.clone()))?;
            Ok(Some(row.month))
        }
        (TakeStep::Update, Some(row), Some(old)) => {
            let mut new = old.clone();
            fill(resolver, &mut new, row)?;
            write_changes(rec, old, &new)?;
            Ok(Some(old.month))
        }
        (TakeStep::Delete, _, Some(old)) => {
            rec.delete(Table::Entry, old.id)?;
            Ok(Some(old.month))
        }
        _ => Ok(None),
    }
}

/// What «Забрать с телефона» did, or would do.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TakeReport {
    pub added: usize,
    pub changed: usize,
    pub deleted: usize,
    /// Rows changed on both sides, or deleted on the computer: the computer's version stays.
    pub kept: usize,
    /// Months whose rows changed.
    pub months: Vec<Month>,
    /// Dictionary items created from the phone's names (D17).
    pub created: Vec<Created>,
}

impl TakeReport {
    fn count(&mut self, step: TakeStep) {
        match step {
            TakeStep::Add => self.added += 1,
            TakeStep::Update => self.changed += 1,
            TakeStep::Delete => self.deleted += 1,
            TakeStep::KeepComputer => self.kept += 1,
            TakeStep::Skip | TakeStep::Forget => {}
        }
    }

    /// Whether there is anything to tell the user about.
    pub fn is_empty(&self) -> bool {
        self.added + self.changed + self.deleted + self.kept == 0
    }
}

/// Remembers the version of a key the computer has seen, and the row as it is now. A row
/// kept as on the computer keeps the old fingerprint: it stays the computer's row.
fn remember(
    rec: &Recorder<'_>,
    known: Option<i64>,
    change: &PhoneChange,
    step: TakeStep,
) -> Result<(), StorageError> {
    let fingerprint = match step {
        TakeStep::Skip => return Ok(()),
        TakeStep::KeepComputer => None,
        TakeStep::Add | TakeStep::Update => Some(
            entry_by_key(rec.tx(), &change.key)?
                .map(|(_, row)| row.fingerprint())
                .unwrap_or_default(),
        ),
        TakeStep::Delete | TakeStep::Forget => Some(String::new()),
    };
    match known {
        Some(id) => {
            rec.update(Table::PhoneTaken, id, "rev", Value::Integer(change.rev))?;
            if let Some(fingerprint) = fingerprint {
                rec.update(
                    Table::PhoneTaken,
                    id,
                    "fingerprint",
                    Value::Text(fingerprint),
                )?;
            }
        }
        None => {
            rec.insert(
                Table::PhoneTaken,
                &vec![
                    ("key", Value::Text(change.key.clone())),
                    ("rev", Value::Integer(change.rev)),
                    ("fingerprint", Value::Text(fingerprint.unwrap_or_default())),
                ],
            )?;
        }
    }
    Ok(())
}

/// On the phone: a fresh key for a new row; on the computer rows get none.
pub(crate) fn new_phone_key(
    tx: &Transaction<'_>,
    role: Role,
) -> Result<Option<String>, StorageError> {
    if role != Role::Phone {
        return Ok(None);
    }
    let hex: String = tx.query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?;
    Ok(Some(format!("{PHONE_KEY_PREFIX}{hex}")))
}

/// On the phone, only its own rows that wait for the computer can be changed (D40).
pub(crate) fn ensure_phone_rows(tx: &Transaction<'_>, ids: &[EntryId]) -> Result<(), StorageError> {
    let mut stmt = tx.prepare_cached(
        "SELECT 1 FROM entry e JOIN phone_change c ON c.key = e.import_key AND c.deleted = 0
         WHERE e.id = ?1",
    )?;
    for id in ids {
        if !stmt.exists([id])? {
            return Err(StorageError::NotPhoneRow);
        }
    }
    Ok(())
}

/// After an action or its undo on the phone: every row it changed gets a new version, and a
/// row it removed becomes a deletion for the computer. Cashback changes do not count, it does
/// not travel.
pub(crate) fn mark_phone_changes(tx: &Transaction<'_>, op_id: i64) -> Result<(), StorageError> {
    let ids: Vec<i64> = tx
        .prepare(
            "SELECT DISTINCT entity_id FROM change_log
             WHERE op_id = ?1 AND entity = 'entry' AND field IN ('*', 'title', 'category_id',
                   'is_regular', 'participant_id', 'date', 'amount', 'product_id')",
        )?
        .query_map([op_id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for id in ids {
        let key: Option<Option<String>> = tx
            .query_row("SELECT import_key FROM entry WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        match key {
            Some(Some(key)) if key.starts_with(PHONE_KEY_PREFIX) => {
                tx.execute(
                    "INSERT INTO phone_change (key, entry_id, rev, deleted) VALUES (?1, ?2, 1, 0)
                     ON CONFLICT (key) DO UPDATE
                     SET rev = rev + 1, deleted = 0, entry_id = excluded.entry_id",
                    params![key, id],
                )?;
            }
            None => {
                tx.execute(
                    "UPDATE phone_change SET rev = rev + 1, deleted = 1
                     WHERE entry_id = ?1 AND deleted = 0",
                    [id],
                )?;
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// What the phone did with its rows when a new copy of the main file arrived.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneStart {
    /// Rows still waiting for the computer, moved into the new copy.
    pub carried: usize,
    /// Rows the computer changed or deleted meanwhile: its version stays.
    pub kept: usize,
}

/// How a copy for the phone in the exchange folder compares with the phone's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotStatus {
    Newer,
    Same,
    Older,
}

impl Vault {
    fn require_main(&self) -> Result<(), StorageError> {
        match self.role {
            Role::Main => Ok(()),
            Role::Phone => Err(StorageError::PhoneFile),
            Role::Snapshot => Err(StorageError::SnapshotFile),
        }
    }

    /// The computer: writes a copy of the main file for the phone to `target` (D39).
    pub fn write_for_phone(&mut self, target: &Path) -> Result<(), StorageError> {
        self.require_main()?;
        let seq = snapshot_seq(&self.conn, "main")? + 1;
        set_meta(&self.conn, "snapshot_seq", &seq.to_string())?;
        let conn = &self.conn;
        copy_whole(self.path(), target, |part| {
            with_attached(conn, part, None, |conn| {
                conn.execute(
                    "INSERT INTO other.meta (key, value) VALUES ('role', ?1)
                     ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                    [Role::Snapshot.code()],
                )?;
                Ok(())
            })
        })
    }

    /// The computer: writes an empty file from the phone to `target`, so the phone opens an
    /// existing file for writing instead of creating one on the disk (D39). Taking it changes
    /// nothing.
    pub fn write_empty_phone_file(&mut self, target: &Path) -> Result<(), StorageError> {
        self.require_main()?;
        let conn = &self.conn;
        copy_whole(self.path(), target, |part| {
            with_attached(conn, part, None, |conn| {
                conn.execute(
                    "INSERT INTO other.meta (key, value) VALUES ('role', ?1)
                     ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                    [Role::Phone.code()],
                )?;
                conn.execute("DELETE FROM other.phone_change", [])?;
                Ok(())
            })
        })
    }

    /// The phone: its rows waiting for the computer, which it may still change (D40).
    pub fn phone_rows(&self) -> Result<Vec<EntryId>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id FROM entry e JOIN phone_change c ON c.key = e.import_key
             WHERE c.deleted = 0 ORDER BY e.id",
        )?;
        let ids = stmt.query_map([], |r| r.get(0))?;
        Ok(ids.collect::<Result<_, _>>()?)
    }

    /// The computer: what «Забрать с телефона» would do with the phone's file at `path`.
    /// `password` is needed only when the phone's file has another password (changed since).
    pub fn phone_preview(
        &self,
        path: &Path,
        password: Option<&str>,
    ) -> Result<TakeReport, StorageError> {
        let changes = self.read_phone_file(path, password)?;
        let taken = taken_of(&self.conn)?;
        let mut report = TakeReport::default();
        for change in &changes {
            let current = entry_by_key(&self.conn, &change.key)?;
            report.count(plan_take(
                change,
                taken.get(&change.key).map(|(_, t)| t),
                current.as_ref().map(|(_, row)| row),
            ));
        }
        Ok(report)
    }

    /// The computer: takes the phone's rows from its file at `path` as one undoable action
    /// (D39).
    pub fn take_from_phone(
        &mut self,
        path: &Path,
        password: Option<&str>,
    ) -> Result<TakeReport, StorageError> {
        let changes = self.read_phone_file(path, password)?;
        self.record("phone", |rec| {
            let taken = taken_of(rec.tx())?;
            let mut resolver = Resolver::new(rec);
            let mut report = TakeReport::default();
            let mut months = BTreeSet::new();
            for change in &changes {
                let known = taken.get(&change.key);
                let current = entry_by_key(rec.tx(), &change.key)?;
                let step = plan_take(
                    change,
                    known.map(|(_, t)| t),
                    current.as_ref().map(|(_, row)| row),
                );
                report.count(step);
                months.extend(carry_out(
                    &mut resolver,
                    step,
                    change,
                    current.as_ref().map(|(e, _)| e),
                )?);
                remember(rec, known.map(|(id, _)| *id), change, step)?;
            }
            for month in &months {
                reallocate(rec, *month)?;
            }
            report.months = months.into_iter().collect();
            report.created = resolver.created;
            Ok(report)
        })
    }

    fn read_phone_file(
        &self,
        path: &Path,
        password: Option<&str>,
    ) -> Result<Vec<PhoneChange>, StorageError> {
        self.require_main()?;
        with_attached(&self.conn, path, password, |conn| {
            check_other(conn, Role::Phone, StorageError::NotFromPhone)?;
            phone_changes(conn, OTHER)
        })
    }

    /// The phone: how the copy for the phone at `snapshot` compares with this file.
    pub fn snapshot_status(
        &self,
        snapshot: &Path,
        password: Option<&str>,
    ) -> Result<SnapshotStatus, StorageError> {
        let ours = snapshot_seq(&self.conn, "main")?;
        let theirs = with_attached(&self.conn, snapshot, password, |conn| {
            check_other(conn, Role::Snapshot, StorageError::NotForPhone)?;
            snapshot_seq(conn, OTHER)
        })?;
        Ok(match theirs.cmp(&ours) {
            std::cmp::Ordering::Greater => SnapshotStatus::Newer,
            std::cmp::Ordering::Equal => SnapshotStatus::Same,
            std::cmp::Ordering::Less => SnapshotStatus::Older,
        })
    }

    /// The phone: makes the copy for the phone at `snapshot` its working file at `working`,
    /// moving over the rows the computer has not taken yet (D39). `working_password` is needed
    /// only when the working file has another password than the copy.
    pub fn start_on_phone(
        snapshot: &Path,
        working: &Path,
        password: &str,
        working_password: Option<&str>,
    ) -> Result<(Self, PhoneStart), StorageError> {
        let name = working
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(StorageError::NotFound)?;
        let part = working.with_file_name(format!(".{name}.next"));
        fs::copy(snapshot, &part)?;
        let result = (|| {
            let mut next = Self::open(&part, password)?;
            if next.role != Role::Snapshot {
                return Err(StorageError::NotForPhone);
            }
            let (changes, base) = if working.exists() {
                with_attached(&next.conn, working, working_password, |conn| {
                    check_other(conn, Role::Phone, StorageError::NotFromPhone)?;
                    Ok((phone_changes(conn, OTHER)?, snapshot_seq(conn, OTHER)?))
                })?
            } else {
                (Vec::new(), 0)
            };
            if snapshot_seq(&next.conn, "main")? < base {
                return Err(StorageError::OlderCopy);
            }
            set_meta(&next.conn, "role", Role::Phone.code())?;
            next.role = Role::Phone;
            let report = next.record(PHONE_START, |rec| {
                let taken = taken_of(rec.tx())?;
                let mut resolver = Resolver::new(rec);
                let mut report = PhoneStart::default();
                let mut months = BTreeSet::new();
                for change in &changes {
                    let current = entry_by_key(rec.tx(), &change.key)?;
                    let step = plan_take(
                        change,
                        taken.get(&change.key).map(|(_, t)| t),
                        current.as_ref().map(|(_, row)| row),
                    );
                    if !step.still_pending() {
                        report.kept += usize::from(step == TakeStep::KeepComputer);
                        continue;
                    }
                    months.extend(carry_out(
                        &mut resolver,
                        step,
                        change,
                        current.as_ref().map(|(e, _)| e),
                    )?);
                    let entry_id = entry_by_key(rec.tx(), &change.key)?.map_or(0, |(e, _)| e.id);
                    rec.tx().execute(
                        "INSERT INTO phone_change (key, entry_id, rev, deleted)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![change.key, entry_id, change.rev, change.entry.is_none()],
                    )?;
                    report.carried += 1;
                }
                for month in &months {
                    reallocate(rec, *month)?;
                }
                Ok(report)
            })?;
            next.close()?;
            Ok(report)
        })();
        let report = match result {
            Ok(report) => report,
            Err(e) => {
                let _ = fs::remove_file(&part);
                return Err(e);
            }
        };
        fs::rename(&part, working)?;
        Ok((Self::open(working, password)?, report))
    }

    /// The phone: writes its file for the computer to `target` (D39).
    pub fn write_for_computer(&self, target: &Path) -> Result<(), StorageError> {
        if self.role != Role::Phone {
            return Err(StorageError::NotFromPhone);
        }
        copy_whole(self.path(), target, |_| Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exchange_files_are_named_after_the_main_file() {
        let paths = exchange_paths(Path::new("docs/Семья.reestr"), Path::new("disk")).unwrap();
        assert_eq!(
            paths.for_phone,
            Path::new("disk").join("Семья — для телефона.reestr")
        );
        assert_eq!(
            paths.from_phone,
            Path::new("disk").join("Семья — с телефона.reestr")
        );
    }

    #[test]
    fn the_phone_edits_rows_only() {
        assert!(Role::Phone.allows("append").is_ok());
        assert!(Role::Phone.allows("delete").is_ok());
        assert!(matches!(
            Role::Phone.allows("rules"),
            Err(StorageError::ComputerOnly)
        ));
        assert!(matches!(
            Role::Snapshot.allows("append"),
            Err(StorageError::SnapshotFile)
        ));
        assert!(Role::Main.allows("import").is_ok());
    }
}
