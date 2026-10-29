//! Tauri commands. Each one is thin: lock state, call core/storage, convert to a DTO.

use std::path::Path;

use reestr_core::date::parse_iso;
use reestr_core::import::{CardSetup, read_workbook};
use reestr_core::{
    DictKind, DictOp, EntryId, InflowOp, JournalOp, Kopecks, Month, ProductOp, RuleOp, TemplateId,
    YearSummary,
};
use reestr_storage::{
    CopyOutcome, Dictionaries, ImportPreview, StorageError, Vault, exchange_paths,
};
use tauri::State;
use time::Date;
use zeroize::Zeroizing;

use crate::dto::{
    AboutView, ChangeResult, DictResult, ExchangeView, FileInfoDto, HistoryState, ImportResult,
    MonthTab, MonthView, PhoneCheck, ProductsResult, ProductsView, ReplayResult, SuggestionView,
    TakeResult,
};
use crate::error::CommandError;
use crate::state::{AppState, lock, with_vault};

type CommandResult<T> = Result<T, CommandError>;

/// How many past titles the autocomplete gets.
const TITLE_HISTORY: usize = 5000;

/// Creates a new encrypted file and makes it the open one.
/// Runs off the main thread: key derivation takes a noticeable moment.
#[tauri::command(async)]
pub fn vault_create(
    state: State<'_, AppState>,
    path: String,
    password: String,
) -> CommandResult<FileInfoDto> {
    let password = Zeroizing::new(password);
    switch_vault(&state, || Vault::create(Path::new(&path), &password))
}

/// Opens an existing file and makes it the open one. The first opening of the day copies the
/// file next to it (D32); `today` is the user's date, "YYYY-MM-DD".
#[tauri::command(async)]
pub fn vault_open(
    state: State<'_, AppState>,
    path: String,
    password: String,
    today: String,
) -> CommandResult<FileInfoDto> {
    let password = Zeroizing::new(password);
    let today = self::today(&today)?;
    let mut copy_failed = false;
    let mut info = switch_vault(&state, || {
        let (vault, outcome) = Vault::open_with_copy(Path::new(&path), &password, today)?;
        copy_failed = matches!(outcome, CopyOutcome::Failed(_));
        Ok(vault)
    })?;
    info.copy_failed = copy_failed;
    Ok(info)
}

/// The «О файле и пароле» page: the file, its encryption and its copies.
#[tauri::command]
pub fn file_about(state: State<'_, AppState>) -> CommandResult<AboutView> {
    with_vault(&state, |v| Ok(AboutView::build(v)?))
}

/// Changes the password of the open file (D33).
#[tauri::command(async)]
pub fn vault_change_password(
    state: State<'_, AppState>,
    current: String,
    new: String,
) -> CommandResult<()> {
    let current = Zeroizing::new(current);
    let new = Zeroizing::new(new);
    with_vault(&state, |v| Ok(v.change_password(&current, &new)?))
}

/// Closes the file. The phone gets a copy of it as it is now (D39).
#[tauri::command]
pub fn vault_close(state: State<'_, AppState>) -> CommandResult<()> {
    let mut vault = lock(&state.vault)?;
    lock(&state.extra_months)?.clear();
    *lock(&state.import)? = None;
    if let Some(mut open) = vault.take() {
        write_copy(&state, &mut open);
        open.close()?;
    }
    Ok(())
}

/// When the window closes: the phone gets a copy of the open file (D39).
pub fn on_exit(state: &AppState) {
    if let Ok(mut vault) = lock(&state.vault)
        && let Some(open) = vault.as_mut()
    {
        write_copy(state, open);
    }
}

/// Writes the copy for the phone into the exchange folder, if one is chosen, and an empty file
/// from the phone the first time. Returns `false` when they could not be written.
fn write_copy(state: &AppState, vault: &mut Vault) -> bool {
    let Ok(folder) = lock(&state.exchange).map(|f| f.clone()) else {
        return false;
    };
    let Some(folder) = folder else {
        return true;
    };
    let Some(paths) = exchange_paths(vault.path(), &folder) else {
        return false;
    };
    // The phone opens its file for writing only if it exists: the first time it is empty.
    let reply_ready =
        paths.from_phone.exists() || vault.write_empty_phone_file(&paths.from_phone).is_ok();
    vault.write_for_phone(&paths.for_phone).is_ok() && reply_ready
}

/// What the phone's file in the exchange folder holds for the open file.
fn check_phone(state: &AppState, vault: &Vault) -> CommandResult<PhoneCheck> {
    let Some(folder) = lock(&state.exchange)?.clone() else {
        return Ok(PhoneCheck::Off);
    };
    let Some(paths) = exchange_paths(vault.path(), &folder) else {
        return Ok(PhoneCheck::Problem { code: "not_found" });
    };
    if !paths.from_phone.is_file() {
        return Ok(PhoneCheck::NoFile);
    }
    Ok(match vault.phone_preview(&paths.from_phone, None) {
        Ok(report) if report.is_empty() => PhoneCheck::UpToDate,
        Ok(report) => PhoneCheck::Changes { report },
        Err(StorageError::WrongPassword) => PhoneCheck::NeedsPassword,
        Err(error) => PhoneCheck::Problem {
            code: CommandError::from(error).code,
        },
    })
}

/// Sets the exchange folder with the phone on this computer (D39), or none. With a folder,
/// writes the copy for the phone at once and looks at the phone's file.
#[tauri::command(async)]
pub fn exchange_folder(
    state: State<'_, AppState>,
    folder: Option<String>,
) -> CommandResult<ExchangeView> {
    *lock(&state.exchange)? = folder.map(Into::into);
    with_vault(&state, |v| {
        Ok(ExchangeView {
            copy_failed: !write_copy(&state, v),
            phone: check_phone(&state, v)?,
        })
    })
}

/// Looks at the phone's file again, e.g. when the window gets the focus back.
#[tauri::command(async)]
pub fn exchange_check(state: State<'_, AppState>) -> CommandResult<ExchangeView> {
    with_vault(&state, |v| {
        Ok(ExchangeView {
            copy_failed: false,
            phone: check_phone(&state, v)?,
        })
    })
}

/// «Забрать с телефона» (D39), then a fresh copy for the phone. `password` is the phone
/// file's password when it differs from this file's.
#[tauri::command(async)]
pub fn exchange_take(
    state: State<'_, AppState>,
    password: Option<String>,
) -> CommandResult<TakeResult> {
    let password = password.map(Zeroizing::new);
    let folder = lock(&state.exchange)?
        .clone()
        .ok_or_else(|| CommandError::new("not_found", "no exchange folder"))?;
    with_vault(&state, |v| {
        let paths = exchange_paths(v.path(), &folder)
            .ok_or_else(|| CommandError::new("not_found", "no exchange file name"))?;
        let report = v.take_from_phone(&paths.from_phone, password.as_ref().map(|p| p.as_str()))?;
        Ok(TakeResult {
            report,
            history: HistoryState::of(v),
            copy_failed: !write_copy(&state, v),
        })
    })
}

#[tauri::command]
pub fn vault_info(state: State<'_, AppState>) -> CommandResult<Option<FileInfoDto>> {
    let vault = lock(&state.vault)?;
    vault
        .as_ref()
        .map(|v| v.info().map(FileInfoDto::from))
        .transpose()
        .map_err(Into::into)
}

/// Closes the current file (releasing its lock) and opens another one.
fn switch_vault(
    state: &AppState,
    open: impl FnOnce() -> Result<Vault, StorageError>,
) -> CommandResult<FileInfoDto> {
    let mut vault = lock(&state.vault)?;
    lock(&state.extra_months)?.clear();
    *lock(&state.import)? = None;
    if let Some(previous) = vault.take() {
        previous.close()?;
    }
    let opened = open()?;
    let info = opened.info()?;
    *vault = Some(opened);
    Ok(info.into())
}

#[tauri::command]
pub fn dictionaries(state: State<'_, AppState>) -> CommandResult<Dictionaries> {
    with_vault(&state, |v| Ok(v.dictionaries()?))
}

/// Edits one dictionary as one undoable action.
#[tauri::command]
pub fn dict_edit(
    state: State<'_, AppState>,
    kind: DictKind,
    ops: Vec<DictOp>,
) -> CommandResult<DictResult> {
    with_vault(&state, |v| {
        let created = v.edit_dictionary(kind, ops)?;
        Ok(DictResult {
            dictionaries: v.dictionaries()?,
            created,
            history: HistoryState::of(v),
        })
    })
}

/// Month tabs: months with entries, months opened in this session, and `current` (the
/// user's current month, which always gets a tab).
#[tauri::command]
pub fn journal_months(state: State<'_, AppState>, current: Month) -> CommandResult<Vec<MonthTab>> {
    let stored = with_vault(&state, |v| Ok(v.months()?))?;
    let mut extra = lock(&state.extra_months)?;
    extra.insert(current);
    Ok(MonthTab::list(
        stored.into_iter().chain(extra.iter().copied()),
    ))
}

#[tauri::command]
pub fn journal_add_month(
    state: State<'_, AppState>,
    month: Month,
    current: Month,
) -> CommandResult<Vec<MonthTab>> {
    lock(&state.extra_months)?.insert(month);
    journal_months(state, current)
}

#[tauri::command]
pub fn journal_month(state: State<'_, AppState>, month: Month) -> CommandResult<MonthView> {
    with_vault(&state, |v| Ok(MonthView::build(v, month)?))
}

/// Past titles for autocomplete, most recent first.
#[tauri::command]
pub fn journal_titles(state: State<'_, AppState>) -> CommandResult<Vec<String>> {
    with_vault(&state, |v| Ok(v.title_history(TITLE_HISTORY)?))
}

/// Applies typed cells, pastes and fills as one undoable action.
#[tauri::command]
pub fn journal_apply(
    state: State<'_, AppState>,
    month: Month,
    ops: Vec<JournalOp>,
) -> CommandResult<ChangeResult> {
    with_vault(&state, |v| {
        let outcome = v.apply(month, ops)?;
        Ok(ChangeResult {
            view: MonthView::build(v, month)?,
            appended: outcome.appended,
            created: outcome.created,
            history: HistoryState::of(v),
        })
    })
}

#[tauri::command]
pub fn journal_delete(
    state: State<'_, AppState>,
    month: Month,
    ids: Vec<EntryId>,
) -> CommandResult<ChangeResult> {
    with_vault(&state, |v| {
        v.delete_entries(&ids)?;
        Ok(ChangeResult {
            view: MonthView::build(v, month)?,
            appended: Vec::new(),
            created: Vec::new(),
            history: HistoryState::of(v),
        })
    })
}

/// Adds rows from regular payment templates at the bottom of the month.
#[tauri::command]
pub fn journal_add_templates(
    state: State<'_, AppState>,
    month: Month,
    ids: Vec<TemplateId>,
) -> CommandResult<ChangeResult> {
    with_vault(&state, |v| {
        let appended = v.add_from_templates(month, &ids)?;
        Ok(ChangeResult {
            view: MonthView::build(v, month)?,
            appended,
            created: Vec::new(),
            history: HistoryState::of(v),
        })
    })
}

/// How many entries of the month would get another cashback from these rule changes: when
/// some would, the UI asks before applying them (PLAN §6.3). Changes nothing.
#[tauri::command]
pub fn rules_impact(
    state: State<'_, AppState>,
    month: Month,
    ops: Vec<RuleOp>,
) -> CommandResult<usize> {
    with_vault(&state, |v| Ok(v.rules_impact(month, ops)?))
}

/// Edits the month's cashback rules as one undoable action. `recompute` is the answer to the
/// question: entries follow the new rules, or keep their cashback as typed.
#[tauri::command]
pub fn rules_apply(
    state: State<'_, AppState>,
    month: Month,
    ops: Vec<RuleOp>,
    recompute: bool,
) -> CommandResult<ChangeResult> {
    with_vault(&state, |v| {
        let outcome = v.apply_rules(month, ops, recompute)?;
        Ok(ChangeResult {
            view: MonthView::build(v, month)?,
            appended: Vec::new(),
            created: outcome.created,
            history: HistoryState::of(v),
        })
    })
}

/// Cards for the card cell of an entry (or of a new row), best first (D6).
#[tauri::command]
pub fn card_suggestions(
    state: State<'_, AppState>,
    month: Month,
    entry: Option<EntryId>,
) -> CommandResult<Vec<SuggestionView>> {
    with_vault(&state, |v| {
        let suggestions = v.card_suggestions(month, entry)?;
        Ok(SuggestionView::list(&v.dictionaries()?, suggestions))
    })
}

/// Turns card suggestions on or off. A setting, not undoable.
#[tauri::command]
pub fn settings_card_suggestions(state: State<'_, AppState>, enabled: bool) -> CommandResult<()> {
    with_vault(&state, |v| Ok(v.set_card_suggestions(enabled)?))
}

/// Changes the month's inflows as one undoable action.
#[tauri::command]
pub fn inflows_apply(
    state: State<'_, AppState>,
    month: Month,
    ops: Vec<InflowOp>,
) -> CommandResult<ChangeResult> {
    with_vault(&state, |v| {
        let created = v.apply_inflows(month, ops)?;
        Ok(ChangeResult {
            view: MonthView::build(v, month)?,
            appended: Vec::new(),
            created,
            history: HistoryState::of(v),
        })
    })
}

/// Sets the income plan from `month` on; `None` returns to the plan of an earlier month.
#[tauri::command]
pub fn plan_set(
    state: State<'_, AppState>,
    month: Month,
    amount: Option<Kopecks>,
) -> CommandResult<ChangeResult> {
    with_vault(&state, |v| {
        v.set_plan(month, amount)?;
        Ok(ChangeResult {
            view: MonthView::build(v, month)?,
            appended: Vec::new(),
            created: Vec::new(),
            history: HistoryState::of(v),
        })
    })
}

/// Years «Свод» can show: those with entries.
#[tauri::command]
pub fn summary_years(state: State<'_, AppState>) -> CommandResult<Vec<i32>> {
    with_vault(&state, |v| Ok(v.summary_years()?))
}

/// «Свод» of a year, by categories or by groups.
#[tauri::command]
pub fn summary_year(
    state: State<'_, AppState>,
    year: i32,
    grouped: bool,
) -> CommandResult<YearSummary> {
    with_vault(&state, |v| Ok(v.year_summary(year, grouped)?))
}

/// The user's today, `YYYY-MM-DD`: the core never reads the clock.
fn today(value: &str) -> CommandResult<Date> {
    parse_iso(value).ok_or_else(|| CommandError::new("invalid_date", "today is not a date"))
}

/// Credit cards, savings accounts and deadlines as of `today`.
#[tauri::command]
pub fn products_view(state: State<'_, AppState>, today: String) -> CommandResult<ProductsView> {
    let today = self::today(&today)?;
    with_vault(&state, |v| {
        Ok(ProductsView::build(&v.dictionaries()?, v.products(today)?))
    })
}

/// Changes on the «Продукты» and «Сроки» sheets as one undoable action.
#[tauri::command]
pub fn products_apply(
    state: State<'_, AppState>,
    ops: Vec<ProductOp>,
    today: String,
) -> CommandResult<ProductsResult> {
    let today = self::today(&today)?;
    with_vault(&state, |v| {
        let created = v.apply_products(ops, today)?;
        Ok(ProductsResult {
            view: ProductsView::build(&v.dictionaries()?, v.products(today)?),
            created,
            history: HistoryState::of(v),
        })
    })
}

/// Undo (Ctrl+Z). Shows the month the change was in when it was not `month`.
#[tauri::command]
pub fn history_undo(state: State<'_, AppState>, month: Month) -> CommandResult<ReplayResult> {
    replay(&state, month, Vault::undo)
}

/// Redo (Ctrl+Y).
#[tauri::command]
pub fn history_redo(state: State<'_, AppState>, month: Month) -> CommandResult<ReplayResult> {
    replay(&state, month, Vault::redo)
}

fn replay(
    state: &AppState,
    month: Month,
    action: fn(&mut Vault) -> Result<Option<reestr_storage::Replayed>, StorageError>,
) -> CommandResult<ReplayResult> {
    with_vault(state, |v| {
        let replayed = action(v)?;
        let show = replayed
            .as_ref()
            .filter(|r| !r.months.iter().any(|m| *m == month.to_string()))
            .and_then(|r| r.months.first())
            .and_then(|m| m.parse().ok())
            .unwrap_or(month);
        Ok(ReplayResult {
            label: replayed.map(|r| r.label),
            view: MonthView::build(v, show)?,
            history: HistoryState::of(v),
        })
    })
}

/// What the export writes (PLAN §9).
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Xlsx,
    Json,
}

/// Writes the open file's export to `path`, chosen in the save dialog. The export is not
/// encrypted; the UI warns before asking for the path (PLAN §10).
#[tauri::command(async)]
pub fn export_file(
    state: State<'_, AppState>,
    path: String,
    format: ExportFormat,
    today: String,
) -> CommandResult<()> {
    let today = self::today(&today)?;
    let bytes = with_vault(&state, |v| {
        Ok(match format {
            ExportFormat::Xlsx => v.export_xlsx(today)?,
            ExportFormat::Json => v.export_json(today)?.into_bytes(),
        })
    })?;
    std::fs::write(&path, bytes).map_err(StorageError::from)?;
    Ok(())
}

/// Reads a workbook for the import wizard and says what importing it would do.
#[tauri::command(async)]
pub fn import_preview(state: State<'_, AppState>, path: String) -> CommandResult<ImportPreview> {
    let bytes = std::fs::read(&path).map_err(StorageError::from)?;
    let book = read_workbook(&bytes)?;
    let preview = with_vault(&state, |v| Ok(v.import_preview(&book)?))?;
    *lock(&state.import)? = Some(book);
    Ok(preview)
}

/// Imports the workbook read by `import_preview`, with the card settings from the wizard.
#[tauri::command(async)]
pub fn import_apply(
    state: State<'_, AppState>,
    cards: Vec<CardSetup>,
) -> CommandResult<ImportResult> {
    let book = lock(&state.import)?
        .take()
        .ok_or_else(|| CommandError::new("import_expired", "no workbook was read"))?;
    with_vault(&state, |v| {
        let report = v.import_workbook(&book, &cards)?;
        Ok(ImportResult {
            report,
            history: HistoryState::of(v),
        })
    })
}

#[tauri::command]
pub fn import_cancel(state: State<'_, AppState>) -> CommandResult<()> {
    *lock(&state.import)? = None;
    Ok(())
}
