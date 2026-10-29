//! Reestr storage: the encrypted data file (PLAN §4.3, D3).
//!
//! A `.reestr` file is a SQLCipher 4 database encrypted as a whole. This crate knows about
//! SQLite but nothing about Tauri.

mod copies;
mod dict;
mod error;
mod exchange;
mod export;
mod history;
mod import;
mod income;
mod journal;
mod migrations;
mod products;
mod rules;
mod vault;

pub use copies::{COPIES_KEPT, CopyOutcome, FileCopy, copies_dir};
pub use dict::{Created, Dictionaries};
pub use error::StorageError;
pub use exchange::{ExchangePaths, PhoneStart, Role, SnapshotStatus, TakeReport, exchange_paths};
pub use history::Replayed;
pub use import::{ImportPreview, ImportReport, PreviewCard, PreviewMonth, PreviewName};
pub use journal::ApplyOutcome;
pub use migrations::SCHEMA_VERSION;
pub use products::{CreditCard, ProductsData, Purchase, SavingsAccount};
pub use rules::{MonthRules, RulesOutcome};
pub use vault::{CipherSetting, FileInfo, Vault};
