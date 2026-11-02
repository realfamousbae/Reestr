use reestr_core::import::ImportError;
use reestr_core::{EditError, ProductError, RuleError};
use reestr_storage::StorageError;
use serde::Serialize;

/// An error returned to the UI. `code` is stable and mapped to a Russian message by the UI;
/// `detail` fills that message (a name, a count); `message` is a technical English
/// description. Errors are shown to the user and never logged (PLAN §10).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
    pub detail: Option<String>,
}

impl CommandError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }

    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn internal() -> Self {
        Self::new("internal", "internal state is unavailable")
    }

    pub fn no_file_open() -> Self {
        Self::new("no_file_open", "no file is open")
    }
}

impl From<StorageError> for CommandError {
    fn from(error: StorageError) -> Self {
        let message = error.to_string();
        match error {
            StorageError::NotFound => Self::new("not_found", message),
            StorageError::AlreadyExists => Self::new("already_exists", message),
            StorageError::EmptyPassword => Self::new("empty_password", message),
            StorageError::WrongPassword => Self::new("wrong_password", message),
            StorageError::NotReestrFile => Self::new("not_reestr_file", message),
            StorageError::NotEncrypted => Self::new("not_encrypted", message),
            StorageError::Locked => Self::new("locked", message),
            StorageError::TooNew { .. } => Self::new("too_new", message),
            StorageError::Io(_) => Self::new("io", message),
            StorageError::Sqlite(_) => Self::new("database", message),
            StorageError::UnknownId(_) => Self::new("unknown_id", message),
            StorageError::Edit(EditError::InvalidMcc(_)) => Self::new("invalid_mcc", message),
            StorageError::Edit(_) => Self::new("invalid_edit", message),
            StorageError::EmptyName => Self::new("empty_name", message),
            StorageError::DuplicateName(name) => {
                Self::new("duplicate_name", message).with_detail(name)
            }
            StorageError::InUse { count } => {
                Self::new("in_use", message).with_detail(count.to_string())
            }
            StorageError::InvalidField { .. } => Self::new("invalid_field", message),
            StorageError::CorruptLog => Self::new("corrupt_log", message),
            StorageError::Rule(RuleError::RateTooHigh(_)) => Self::new("invalid_edit", message),
            StorageError::Rule(RuleError::NegativeLimit) => Self::new("negative_limit", message),
            StorageError::Rule(RuleError::InvalidMcc(_)) => Self::new("invalid_mcc", message),
            StorageError::Rule(RuleError::MccAndCategory) => Self::new("mcc_and_category", message),
            StorageError::DuplicateRule => Self::new("duplicate_rule", message),
            StorageError::Export(_) => Self::new("export_failed", message),
            StorageError::PhoneCopy => Self::new("phone_copy", message),
            StorageError::Product(error) => {
                let code = match error {
                    ProductError::EndBeforeStart => "end_before_start",
                    ProductError::AmountNotPositive => "amount_not_positive",
                    ProductError::NegativeLimit => "negative_limit",
                    ProductError::RateTooHigh(_) => "invalid_edit",
                    ProductError::DaysNotPositive => "days_not_positive",
                    ProductError::EmptyTitle => "empty_title",
                    ProductError::NoTranche => "no_tranche",
                    ProductError::AvailableWithoutDate => "available_without_date",
                };
                Self::new(code, message)
            }
        }
    }
}

impl From<EditError> for CommandError {
    fn from(error: EditError) -> Self {
        let code = match error {
            EditError::InvalidMcc(_) => "invalid_mcc",
            EditError::RateTooHigh(_) | EditError::NotFound(_) => "invalid_edit",
        };
        Self::new(code, error.to_string())
    }
}

impl From<ImportError> for CommandError {
    fn from(error: ImportError) -> Self {
        let code = match error {
            ImportError::Read(_) => "workbook_unreadable",
            ImportError::NoServiceSheet => "workbook_no_service",
            ImportError::NoMonths => "workbook_no_months",
        };
        Self::new(code, error.to_string())
    }
}
