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
    /// The phone's working file was given where the main file is expected (D39).
    #[error("this is the phone's file, not the main one")]
    PhoneFile,
    /// A copy of the main file written for the phone was given where the main file is
    /// expected (D39).
    #[error("this is a copy for the phone, not the main file")]
    SnapshotFile,
    #[error("this is not a file from the phone")]
    NotFromPhone,
    #[error("this is not a copy of the main file for the phone")]
    NotForPhone,
    /// The phone's file or copy belongs to another main file.
    #[error("the file belongs to another main file")]
    OtherFile,
    /// The copy for the phone is older than the one the phone works with.
    #[error("the copy for the phone is older than the phone's file")]
    OlderCopy,
    /// Rules, dictionaries, products and the rest are edited on the computer only (D40).
    #[error("only the computer can do this")]
    ComputerOnly,
    /// The phone edits only its own rows the computer has not taken yet (D40).
    #[error("the row belongs to the computer")]
    NotPhoneRow,
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
