use std::collections::BTreeSet;
use std::sync::{Mutex, MutexGuard};

use reestr_core::Month;
use reestr_core::import::Workbook;
use reestr_storage::Vault;

use crate::error::CommandError;

/// State shared by all commands.
#[derive(Default)]
pub struct AppState {
    /// The open data file, if any.
    pub vault: Mutex<Option<Vault>>,
    /// Month tabs opened in this session that have no entries yet; not stored in the file.
    pub extra_months: Mutex<BTreeSet<Month>>,
    /// A workbook read for the import wizard, between preview and apply.
    pub import: Mutex<Option<Workbook>>,
}

pub fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, CommandError> {
    mutex.lock().map_err(|_| CommandError::internal())
}

/// Runs `f` with the open file, or fails with `no_file_open`.
pub fn with_vault<T>(
    state: &AppState,
    f: impl FnOnce(&mut Vault) -> Result<T, CommandError>,
) -> Result<T, CommandError> {
    let mut guard = lock(&state.vault)?;
    let vault = guard.as_mut().ok_or_else(CommandError::no_file_open)?;
    f(vault)
}
