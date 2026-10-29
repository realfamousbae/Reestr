use std::fs;
use std::path::PathBuf;

use reestr_storage::{CipherSetting, SCHEMA_VERSION, StorageError, Vault};
use tempfile::TempDir;

const PASSWORD: &str = "correct horse 'battery' \"staple\"; --";

fn file_in(dir: &TempDir) -> PathBuf {
    dir.path().join("family.reestr")
}

#[test]
fn create_then_reopen() {
    let dir = TempDir::new().unwrap();
    let path = file_in(&dir);

    let vault = Vault::create(&path, PASSWORD).unwrap();
    let created = vault.info().unwrap();
    assert_eq!(created.schema_version, SCHEMA_VERSION);
    assert_eq!(created.created_with, env!("CARGO_PKG_VERSION"));
    assert!(!created.created_at.is_empty());
    assert!(created.cipher_version.starts_with('4'), "{created:?}");
    vault.close().unwrap();

    let vault = Vault::open(&path, PASSWORD).unwrap();
    let reopened = vault.info().unwrap();
    assert_eq!(reopened.created_at, created.created_at);
    assert_eq!(vault.path(), path);
}

#[test]
fn file_on_disk_is_encrypted() {
    let dir = TempDir::new().unwrap();
    let path = file_in(&dir);
    Vault::create(&path, PASSWORD).unwrap().close().unwrap();

    let bytes = fs::read(&path).unwrap();
    assert!(bytes.len() >= 4096, "one page at least");
    assert_ne!(&bytes[..16], b"SQLite format 3\0");
    let text = String::from_utf8_lossy(&bytes);
    for plain in ["reestr", "meta", "created_at", "CREATE TABLE"] {
        assert!(!text.contains(plain), "{plain:?} is visible on disk");
    }
}

#[test]
fn uses_sqlcipher_4_defaults() {
    let dir = TempDir::new().unwrap();
    let vault = Vault::create(&file_in(&dir), PASSWORD).unwrap();
    let setting = |s| vault.cipher_setting(s).unwrap();
    // PLAN §4.1: AES-256, PBKDF2-HMAC-SHA512, 256 000 iterations.
    assert_eq!(setting(CipherSetting::KdfIterations), "256000");
    assert_eq!(setting(CipherSetting::KdfAlgorithm), "PBKDF2_HMAC_SHA512");
    assert_eq!(setting(CipherSetting::HmacAlgorithm), "HMAC_SHA512");
    assert_eq!(setting(CipherSetting::PageSize), "4096");
    let provider = setting(CipherSetting::Provider);
    assert!(
        ["openssl", "commoncrypto"].contains(&provider.as_str()),
        "{provider}"
    );
}

#[test]
fn wrong_password_is_rejected() {
    let dir = TempDir::new().unwrap();
    let path = file_in(&dir);
    Vault::create(&path, PASSWORD).unwrap().close().unwrap();

    for wrong in ["wrong", "correct horse", ""] {
        let err = Vault::open(&path, wrong).unwrap_err();
        assert!(
            matches!(err, StorageError::WrongPassword),
            "{wrong:?}: {err:?}"
        );
    }
    // The file survives failed attempts.
    Vault::open(&path, PASSWORD).unwrap();
}

#[test]
fn create_refuses_to_overwrite() {
    let dir = TempDir::new().unwrap();
    let path = file_in(&dir);
    fs::write(&path, b"precious").unwrap();

    let err = Vault::create(&path, PASSWORD).unwrap_err();
    assert!(matches!(err, StorageError::AlreadyExists), "{err:?}");
    assert_eq!(fs::read(&path).unwrap(), b"precious");
}

#[test]
fn create_rejects_empty_password() {
    let dir = TempDir::new().unwrap();
    let path = file_in(&dir);
    let err = Vault::create(&path, "").unwrap_err();
    assert!(matches!(err, StorageError::EmptyPassword), "{err:?}");
    assert!(!path.exists());
}

#[test]
fn open_missing_file() {
    let dir = TempDir::new().unwrap();
    let err = Vault::open(&file_in(&dir), PASSWORD).unwrap_err();
    assert!(matches!(err, StorageError::NotFound), "{err:?}");
}

#[test]
fn open_plain_sqlite_file() {
    let dir = TempDir::new().unwrap();
    let path = file_in(&dir);
    let conn = rusqlite_plain(&path);
    drop(conn);

    let err = Vault::open(&path, PASSWORD).unwrap_err();
    assert!(matches!(err, StorageError::NotEncrypted), "{err:?}");
}

#[test]
fn open_random_bytes() {
    let dir = TempDir::new().unwrap();
    let path = file_in(&dir);
    fs::write(&path, vec![0x5a; 8192]).unwrap();

    let err = Vault::open(&path, PASSWORD).unwrap_err();
    assert!(matches!(err, StorageError::WrongPassword), "{err:?}");
}

#[test]
fn second_open_while_first_is_open_fails() {
    let dir = TempDir::new().unwrap();
    let path = file_in(&dir);
    let first = Vault::create(&path, PASSWORD).unwrap();

    let err = Vault::open(&path, PASSWORD).unwrap_err();
    assert!(matches!(err, StorageError::Locked), "{err:?}");

    first.close().unwrap();
    Vault::open(&path, PASSWORD).unwrap();
}

#[test]
fn unicode_password() {
    let dir = TempDir::new().unwrap();
    let path = file_in(&dir);
    Vault::create(&path, "Пароль от реестра №1").unwrap();
    Vault::open(&path, "Пароль от реестра №1").unwrap();
    assert!(matches!(
        Vault::open(&path, "пароль от реестра №1").unwrap_err(),
        StorageError::WrongPassword
    ));
}

/// A plain SQLite database, created without a key.
fn rusqlite_plain(path: &std::path::Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);")
        .unwrap();
    conn
}
