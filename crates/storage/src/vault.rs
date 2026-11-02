//! Creating and opening the encrypted `.reestr` file.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use time::format_description::well_known::Rfc3339;
use time::{Date, OffsetDateTime};
use zeroize::Zeroizing;

use crate::copies::{CopyOutcome, FileCopy, copies_of, make_copy};
use crate::error::StorageError;
use crate::history::SessionHistory;
use crate::migrations;

/// The value of `meta.format` that marks a Reestr file.
const FORMAT: &str = "reestr";

/// First 16 bytes of every unencrypted SQLite database.
const PLAIN_SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// An open data file. Holds an exclusive lock on the file until dropped or closed, so a
/// second window or program cannot write to it at the same time.
#[derive(Debug)]
pub struct Vault {
    pub(crate) conn: Connection,
    path: PathBuf,
    pub(crate) history: SessionHistory,
}

/// What the app shows about an open file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    pub path: PathBuf,
    pub schema_version: u32,
    /// RFC 3339, UTC.
    pub created_at: String,
    /// App version that created the file.
    pub created_with: String,
    pub cipher_version: String,
}

impl Vault {
    /// Creates a new encrypted file. Fails if `path` already exists.
    ///
    /// The password is not recoverable: without it the file cannot be read.
    pub fn create(path: &Path, password: &str) -> Result<Self, StorageError> {
        if password.is_empty() {
            // An empty key would leave SQLCipher unencrypted.
            return Err(StorageError::EmptyPassword);
        }
        if path.exists() {
            return Err(StorageError::AlreadyExists);
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let result = Connection::open_with_flags(path, flags)
            .map_err(StorageError::from)
            .and_then(|conn| {
                let mut vault = Self::prepare(conn, path, password)?;
                migrations::migrate(&mut vault.conn)?;
                let created_at = OffsetDateTime::now_utc()
                    .format(&Rfc3339)
                    .unwrap_or_default();
                vault.conn.execute(
                    "INSERT INTO meta (key, value) VALUES
                        ('format', ?1), ('created_at', ?2), ('created_with', ?3)",
                    params![FORMAT, created_at, env!("CARGO_PKG_VERSION")],
                )?;
                crate::dict::seed_defaults(&vault.conn)?;
                Ok(vault)
            });
        if result.is_err() {
            // Do not leave a half-created file behind.
            let _ = std::fs::remove_file(path);
        }
        result
    }

    /// Opens an existing file and upgrades its schema if it is older than this build.
    pub fn open(path: &Path, password: &str) -> Result<Self, StorageError> {
        Self::open_then(path, password, |_| Ok(())).map(|(vault, ())| vault)
    }

    /// Opens the file like [`Vault::open`], copying it into its copies folder first (D32): once
    /// a day, before the schema is upgraded. A failed copy does not stop the file from opening;
    /// the outcome says what happened.
    pub fn open_with_copy(
        path: &Path,
        password: &str,
        today: Date,
    ) -> Result<(Self, CopyOutcome), StorageError> {
        Self::open_then(path, password, |path| Ok(make_copy(path, today)))
    }

    /// Opens the file and runs `before_upgrade` once the password is checked, before the
    /// migrations. Files of the old phone exchange are refused here: edits there would be
    /// lost, and the migrations would remove what tells them apart (D41).
    fn open_then<T>(
        path: &Path,
        password: &str,
        before_upgrade: impl FnOnce(&Path) -> Result<T, StorageError>,
    ) -> Result<(Self, T), StorageError> {
        if !path.is_file() {
            return Err(StorageError::NotFound);
        }
        if has_plain_sqlite_header(path)? {
            return Err(StorageError::NotEncrypted);
        }
        if password.is_empty() {
            return Err(StorageError::WrongPassword);
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(path, flags)?;
        let mut vault = Self::prepare(conn, path, password)?;

        let format: Option<String> = vault
            .conn
            .query_row("SELECT value FROM meta WHERE key = 'format'", [], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|_| StorageError::NotReestrFile)?;
        if format.as_deref() != Some(FORMAT) {
            return Err(StorageError::NotReestrFile);
        }
        if is_phone_copy(&vault.conn)? {
            return Err(StorageError::PhoneCopy);
        }
        let done = before_upgrade(path)?;
        migrations::migrate(&mut vault.conn)?;
        Ok((vault, done))
    }

    /// Keys the connection, checks the key and takes the exclusive lock.
    fn prepare(conn: Connection, path: &Path, password: &str) -> Result<Self, StorageError> {
        // SQLCipher always wipes its own key material; this extends the wiping to all memory
        // SQLite frees, such as decrypted pages with amounts and titles. The setting is global
        // for the process and cannot be turned off, so it is on before the key is set.
        conn.execute_batch("PRAGMA cipher_memory_security = ON;")?;
        // Fail at once instead of waiting when another process holds the file.
        conn.busy_timeout(Duration::ZERO)?;
        check_key(&conn, password)?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA synchronous = FULL;
             PRAGMA locking_mode = EXCLUSIVE;
             BEGIN EXCLUSIVE; COMMIT;",
        )?;
        Ok(Self {
            conn,
            path: path.to_owned(),
            history: SessionHistory::default(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Changes the password (D33). The open file is locked, so `current` is checked on a
    /// temporary copy of it; then every page is re-encrypted in one transaction. Copies made
    /// before keep the old password.
    pub fn change_password(&mut self, current: &str, new: &str) -> Result<(), StorageError> {
        if new.is_empty() {
            return Err(StorageError::EmptyPassword);
        }
        if current.is_empty() {
            return Err(StorageError::WrongPassword);
        }
        let dir = tempfile::TempDir::new()?;
        let check = dir.path().join("check.reestr");
        std::fs::copy(&self.path, &check)?;
        {
            let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
            let conn = Connection::open_with_flags(&check, flags)?;
            check_key(&conn, current)?;
        }
        self.conn.execute_batch(&keyed_pragma("rekey", new))?;
        Ok(())
    }

    /// The copies of this file, oldest first (D32).
    pub fn copies(&self) -> Result<Vec<FileCopy>, StorageError> {
        Ok(copies_of(&self.path)?)
    }

    pub fn info(&self) -> Result<FileInfo, StorageError> {
        let meta = |key: &str| -> Result<String, StorageError> {
            Ok(self
                .conn
                .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                    row.get(0)
                })
                .optional()?
                .unwrap_or_default())
        };
        Ok(FileInfo {
            path: self.path.clone(),
            schema_version: migrations::user_version(&self.conn)?,
            created_at: meta("created_at")?,
            created_with: meta("created_with")?,
            cipher_version: self.cipher_setting(CipherSetting::Version)?,
        })
    }

    /// Reads a SQLCipher setting. Used by tests and the "about file" page.
    pub fn cipher_setting(&self, setting: CipherSetting) -> Result<String, StorageError> {
        // rusqlite's pragma helpers only accept pragmas known to plain SQLite, so the
        // SQLCipher ones are queried directly; the names come from a fixed list.
        let sql = format!("PRAGMA {}", setting.pragma());
        Ok(self.conn.query_row(&sql, [], |row| row.get(0))?)
    }

    /// Closes the file and releases the lock.
    pub fn close(self) -> Result<(), StorageError> {
        self.conn.close().map_err(|(_, error)| error.into())
    }
}

/// SQLCipher settings that can be read back from an open file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CipherSetting {
    Version,
    KdfIterations,
    KdfAlgorithm,
    HmacAlgorithm,
    PageSize,
    Provider,
    /// "1" once memory wiping is on and SQLite has allocated memory since.
    MemorySecurity,
}

impl CipherSetting {
    const fn pragma(self) -> &'static str {
        match self {
            Self::Version => "cipher_version",
            Self::KdfIterations => "kdf_iter",
            Self::KdfAlgorithm => "cipher_kdf_algorithm",
            Self::HmacAlgorithm => "cipher_hmac_algorithm",
            Self::PageSize => "cipher_page_size",
            Self::Provider => "cipher_provider",
            Self::MemorySecurity => "cipher_memory_security",
        }
    }
}

/// Whether `meta.role` marks a copy for the phone or the phone's working file. The phone
/// exchange of old versions wrote these; a main file never had the key (D41).
fn is_phone_copy(conn: &Connection) -> Result<bool, StorageError> {
    let role: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key = 'role'", [], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(matches!(role.as_deref(), Some("snapshot" | "phone")))
}

/// Keys the connection and reads page 1: a wrong key fails here with SQLITE_NOTADB.
fn check_key(conn: &Connection, password: &str) -> Result<(), StorageError> {
    conn.execute_batch(&keyed_pragma("key", password))?;
    conn.query_row("SELECT count(*) FROM sqlite_schema", [], |row| {
        row.get::<_, i64>(0)
    })?;
    Ok(())
}

/// `PRAGMA key = '…'` (or `rekey`) with the password as an escaped SQL string literal. The
/// statement text holds the password, so it is wiped when dropped.
fn keyed_pragma(pragma: &str, password: &str) -> Zeroizing<String> {
    let mut sql = Zeroizing::new(String::with_capacity(password.len() * 2 + 18));
    sql.push_str("PRAGMA ");
    sql.push_str(pragma);
    sql.push_str(" = '");
    for ch in password.chars() {
        if ch == '\'' {
            sql.push('\'');
        }
        sql.push(ch);
    }
    sql.push_str("';");
    sql
}

fn has_plain_sqlite_header(path: &Path) -> Result<bool, StorageError> {
    let mut header = [0u8; 16];
    let mut file = File::open(path)?;
    match file.read_exact(&mut header) {
        Ok(()) => Ok(&header == PLAIN_SQLITE_HEADER),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use reestr_core::{EditInput, JournalOp, Month};

    use super::*;

    #[test]
    fn refuses_files_from_a_newer_version() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("new.reestr");
        let vault = Vault::create(&path, "pw").unwrap();
        vault
            .conn
            .pragma_update(None, "user_version", migrations::SCHEMA_VERSION + 1)
            .unwrap();
        vault.close().unwrap();

        let err = Vault::open(&path, "pw").unwrap_err();
        assert!(
            matches!(err, StorageError::TooNew { found, supported }
                if found == migrations::SCHEMA_VERSION + 1 && supported == migrations::SCHEMA_VERSION),
            "{err:?}"
        );
    }

    /// A file of schema 7, as an old version left it, keyed with `password`.
    fn old_file(path: &Path, password: &str, sql: &str) {
        let conn = Connection::open(path).unwrap();
        check_key(&conn, password).unwrap();
        for migration in &migrations::MIGRATIONS[..7] {
            conn.execute_batch(migration).unwrap();
        }
        conn.pragma_update(None, "user_version", 7).unwrap();
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('format', ?1)",
            [FORMAT],
        )
        .unwrap();
        conn.execute_batch(sql).unwrap();
        conn.close().unwrap();
    }

    #[test]
    fn files_of_the_phone_exchange_are_refused_before_the_upgrade() {
        let dir = tempfile::TempDir::new().unwrap();
        let today = time::macros::date!(2026 - 07 - 01);
        for role in ["snapshot", "phone"] {
            let path = dir.path().join(format!("{role}.reestr"));
            old_file(
                &path,
                "pw",
                &format!("INSERT INTO meta (key, value) VALUES ('role', '{role}')"),
            );

            let err = Vault::open(&path, "pw").unwrap_err();
            assert!(matches!(err, StorageError::PhoneCopy), "{err:?}");
            let err = Vault::open_with_copy(&path, "pw", today).unwrap_err();
            assert!(matches!(err, StorageError::PhoneCopy), "{err:?}");
            assert_eq!(copies_of(&path).unwrap().len(), 0);

            // Nothing was upgraded: the file still says what it is.
            let conn = Connection::open(&path).unwrap();
            check_key(&conn, "pw").unwrap();
            assert_eq!(migrations::user_version(&conn).unwrap(), 7);
            assert!(is_phone_copy(&conn).unwrap());
        }
    }

    #[test]
    fn a_main_file_with_rows_taken_from_the_phone_opens_and_undo_works() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("main.reestr");
        old_file(
            &path,
            "pw",
            "INSERT INTO meta (key, value) VALUES ('snapshot_seq', '3');
             INSERT INTO entry (month, position, title, amount, import_key, created_at, updated_at)
                 VALUES ('2026-07', 1, 'Обед', 45000, 'phone-1', '', '');
             INSERT INTO phone_taken (key, rev, fingerprint) VALUES ('phone-1', 1, 'abc');
             INSERT INTO change_op (ts, label) VALUES ('', 'phone');
             INSERT INTO change_log (op_id, ts, entity, entity_id, field, new_value)
                 VALUES (1, '', 'phone_taken', 1, '*', '{\"key\":\"phone-1\"}');",
        );

        let (mut vault, outcome) =
            Vault::open_with_copy(&path, "pw", time::macros::date!(2026 - 07 - 01)).unwrap();
        assert!(matches!(outcome, CopyOutcome::Made(_)), "{outcome:?}");
        let month: Month = "2026-07".parse().unwrap();
        assert_eq!(vault.entries(month).unwrap().len(), 1);

        // Undo reaches this session's actions only, never the old log rows.
        assert!(!vault.can_undo());
        assert_eq!(vault.undo().unwrap(), None);
        vault
            .apply(
                month,
                vec![JournalOp::Append {
                    edits: vec![EditInput::Title("Кофе".to_owned())],
                }],
            )
            .unwrap();
        assert_eq!(vault.entries(month).unwrap().len(), 2);
        assert_eq!(vault.undo().unwrap().unwrap().label, "append");
        assert_eq!(vault.entries(month).unwrap().len(), 1);
        assert!(!vault.can_undo());
    }

    #[test]
    fn key_pragma_escapes_quotes() {
        assert_eq!(keyed_pragma("key", "a'b").as_str(), "PRAGMA key = 'a''b';");
        assert_eq!(
            keyed_pragma("rekey", "пароль").as_str(),
            "PRAGMA rekey = 'пароль';"
        );
    }
}
