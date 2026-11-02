use rusqlite::ErrorCode;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("file not found")]
    NotFound,
    #[error("file already exists")]
    AlreadyExists,
    #[error("password must not be empty")]
    EmptyPassword,
    /// SQLCipher cannot tell a wrong password from a damaged file.
    #[error("wrong password or damaged file")]
    WrongPassword,
    #[error("not a Reestr file")]
    NotReestrFile,
    #[error("file is a plain, unencrypted SQLite database")]
    NotEncrypted,
    #[error("file is already open in another window or program")]
    Locked,
    #[error("file has schema version {found}, this version supports up to {supported}")]
    TooNew { found: u32, supported: u32 },
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Sqlite(rusqlite::Error),
    #[error("no row with id {0}")]
    UnknownId(i64),
    #[error("invalid edit: {0}")]
    Edit(#[from] reestr_core::EditError),
    #[error("a name must not be empty")]
    EmptyName,
    #[error("the name {0:?} is already taken")]
    DuplicateName(String),
    #[error("still used by {count} rows")]
    InUse { count: i64 },
    #[error("field {field} does not belong to {entity}")]
    InvalidField {
        entity: &'static str,
        field: &'static str,
    },
    #[error("the change log is damaged")]
    CorruptLog,
    #[error("invalid rule: {0}")]
    Rule(#[from] reestr_core::RuleError),
    /// The month already has a rule for this card and category.
    #[error("the month already has a rule for this card and category")]
    DuplicateRule,
    #[error("invalid product data: {0}")]
    Product(#[from] reestr_core::ProductError),
    #[error("export failed: {0}")]
    Export(#[from] reestr_core::export::ExportError),
    /// A file made by the phone exchange of an old version: edits there would be lost (D41).
    #[error("this is a copy from the phone exchange of an old version, not the main file")]
    PhoneCopy,
}

impl From<rusqlite::Error> for StorageError {
    fn from(error: rusqlite::Error) -> Self {
        match error.sqlite_error_code() {
            Some(ErrorCode::NotADatabase) => Self::WrongPassword,
            Some(ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) => Self::Locked,
            _ => Self::Sqlite(error),
        }
    }
}
