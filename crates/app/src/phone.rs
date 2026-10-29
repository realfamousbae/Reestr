//! The phone (D38–D40): the two exchange files picked once on Yandex Disk, opening with the
//! password, sending the phone's file after every change. Rows go through the same journal
//! commands as on the computer; storage keeps the phone to its own rows.

use std::path::{Path, PathBuf};

use reestr_core::{EntryId, Kopecks, Month};
use reestr_storage::{PhoneStart, SnapshotStatus, StorageError, Vault};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_reestr_files::{FilesExt, Picked};
use zeroize::Zeroizing;

use crate::dto::{EntryView, MonthView, SuggestionView};
use crate::error::CommandError;
use crate::state::{AppState, lock, with_vault};

type CommandResult<T> = Result<T, CommandError>;

/// The files of the phone in the app's own folder.
const SETUP: &str = "phone.json";
const WORKING: &str = "phone.reestr";
const INCOMING: &str = "incoming.reestr";
const OUTGOING: &str = "outgoing.reestr";

/// The two files on Yandex Disk, as picked.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneSetup {
    /// «Семья — для телефона.reestr»: the copy the computer writes.
    pub copy: Option<Picked>,
    /// «Семья — с телефона.reestr»: the file the phone writes.
    pub reply: Option<Picked>,
    /// The phone has its own file: it was opened before.
    #[serde(default, skip_deserializing)]
    pub started: bool,
}

/// Which of the two files is picked.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Which {
    Copy,
    Reply,
}

/// What opening on the phone did.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneOpened {
    /// The rows moved into a newer copy from the computer; `None` when the copy was not newer.
    pub start: Option<PhoneStart>,
    /// The copy could not be read (no internet, Yandex Disk not synced yet): the phone works
    /// with what it had.
    pub offline: bool,
    /// The phone's file reached Yandex Disk.
    pub sent: bool,
}

/// A month on the phone: its rows and which of them are the phone's own (D40).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneMonth {
    pub view: MonthView,
    pub own: Vec<EntryId>,
}

fn io(error: impl std::fmt::Display) -> CommandError {
    CommandError::new("io", error.to_string())
}

fn folder(app: &AppHandle) -> CommandResult<PathBuf> {
    let dir = app.path().app_data_dir().map_err(io)?;
    std::fs::create_dir_all(&dir).map_err(io)?;
    Ok(dir)
}

fn read_setup(dir: &Path) -> PhoneSetup {
    let mut setup: PhoneSetup = std::fs::read(dir.join(SETUP))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    setup.started = dir.join(WORKING).is_file();
    setup
}

/// Which platform the app runs on: the UI shows the phone's screens on Android.
#[tauri::command]
pub fn app_platform() -> &'static str {
    std::env::consts::OS
}

#[tauri::command(async)]
pub fn phone_setup(app: AppHandle) -> CommandResult<PhoneSetup> {
    Ok(read_setup(&folder(&app)?))
}

/// Picks one of the two files: in the system picker on Android, or at `path` when the screens
/// of the phone are tried on a computer.
#[tauri::command(async)]
pub fn phone_choose(
    app: AppHandle,
    which: Which,
    path: Option<String>,
) -> CommandResult<PhoneSetup> {
    let dir = folder(&app)?;
    let picked = match path {
        Some(path) => {
            let name = Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned());
            Some(Picked { uri: path, name })
        }
        None => app.files().pick().map_err(io)?,
    };
    let mut setup = read_setup(&dir);
    if let Some(picked) = picked {
        match which {
            Which::Copy => setup.copy = Some(picked),
            Which::Reply => setup.reply = Some(picked),
        }
        let json = serde_json::to_vec_pretty(&setup).map_err(io)?;
        std::fs::write(dir.join(SETUP), json).map_err(io)?;
    }
    Ok(read_setup(&dir))
}

/// Opens the phone's file with the password, first moving it onto a newer copy from the
/// computer when there is one (D39). When the password was changed on the computer the copy
/// and the phone's file differ: `password` is then the new one and `other_password` the old.
#[tauri::command(async)]
pub fn phone_open(
    app: AppHandle,
    state: State<'_, AppState>,
    password: String,
    other_password: Option<String>,
) -> CommandResult<PhoneOpened> {
    let password = Zeroizing::new(password);
    let other = other_password.map(Zeroizing::new);
    let dir = folder(&app)?;
    let setup = read_setup(&dir);
    let copy = setup
        .copy
        .as_ref()
        .ok_or_else(|| CommandError::new("phone_not_set_up", "the copy is not picked"))?;
    let mut slot = lock(&state.vault)?;
    if let Some(previous) = slot.take() {
        previous.close()?;
    }
    let working = dir.join(WORKING);
    let incoming = dir.join(INCOMING);
    let fetched = app.files().copy_from(&copy.uri, &incoming).is_ok();
    let opened = open_phone(
        &working,
        fetched.then_some(incoming.as_path()),
        &password,
        other.as_ref().map(|p| p.as_str()),
    );
    let _ = std::fs::remove_file(&incoming);
    let (vault, start) = opened?;
    let sent = send(&app, &dir, &setup, &vault);
    *slot = Some(vault);
    Ok(PhoneOpened {
        start,
        offline: !fetched,
        sent,
    })
}

fn open_phone(
    working: &Path,
    incoming: Option<&Path>,
    password: &str,
    other: Option<&str>,
) -> CommandResult<(Vault, Option<PhoneStart>)> {
    let Some(incoming) = incoming else {
        if !working.is_file() {
            return Err(CommandError::new(
                "phone_no_copy",
                "the copy for the phone cannot be read",
            ));
        }
        return Ok((Vault::open(working, password)?, None));
    };
    if !working.is_file() || other.is_some() {
        let (vault, start) = Vault::start_on_phone(incoming, working, password, other)?;
        return Ok((vault, Some(start)));
    }
    match Vault::open(working, password) {
        Ok(vault) => match vault.snapshot_status(incoming, None) {
            Ok(SnapshotStatus::Newer) => {
                vault.close()?;
                let (vault, start) = Vault::start_on_phone(incoming, working, password, None)?;
                Ok((vault, Some(start)))
            }
            Ok(SnapshotStatus::Same | SnapshotStatus::Older) => Ok((vault, None)),
            // The copy does not open with the key of the phone's file: the computer changed
            // the password, and this one is the old.
            Err(StorageError::WrongPassword) => {
                vault.close()?;
                Err(CommandError::new(
                    "phone_new_password",
                    "the copy has a new password",
                ))
            }
            Err(error) => Err(error.into()),
        },
        // Perhaps the computer's new password: it opens the copy, not the phone's file.
        Err(StorageError::WrongPassword) => match Vault::open(incoming, password) {
            Ok(copy) => {
                copy.close()?;
                Err(CommandError::new(
                    "phone_old_password",
                    "the phone's file has the old password",
                ))
            }
            Err(error) => Err(error.into()),
        },
        Err(error) => Err(error.into()),
    }
}

/// Puts the phone's file on Yandex Disk. Returns whether it got there.
fn send(app: &AppHandle, dir: &Path, setup: &PhoneSetup, vault: &Vault) -> bool {
    let Some(reply) = &setup.reply else {
        return false;
    };
    let out = dir.join(OUTGOING);
    let sent =
        vault.write_for_computer(&out).is_ok() && app.files().copy_to(&out, &reply.uri).is_ok();
    let _ = std::fs::remove_file(&out);
    sent
}

/// Sends the phone's file after a change. Returns whether it reached Yandex Disk.
#[tauri::command(async)]
pub fn phone_send(app: AppHandle, state: State<'_, AppState>) -> CommandResult<bool> {
    let dir = folder(&app)?;
    let setup = read_setup(&dir);
    with_vault(&state, |v| Ok(send(&app, &dir, &setup, v)))
}

#[tauri::command]
pub fn phone_month(state: State<'_, AppState>, month: Month) -> CommandResult<PhoneMonth> {
    with_vault(&state, |v| {
        Ok(PhoneMonth {
            view: MonthView::build(v, month)?,
            own: v.phone_rows()?,
        })
    })
}

/// The last row with this title: the form fills the empty fields from it, like the journal.
#[tauri::command]
pub fn phone_last(state: State<'_, AppState>, title: String) -> CommandResult<Option<EntryView>> {
    with_vault(&state, |v| {
        let Some(entry) = v.last_titled(&title)? else {
            return Ok(None);
        };
        Ok(Some(EntryView::of(&v.dictionaries()?, &entry)))
    })
}

/// «Какой картой платить» for the row being typed (D6), when the hint is on.
#[tauri::command]
pub fn phone_suggest(
    state: State<'_, AppState>,
    month: Month,
    category: Option<String>,
    amount: Option<Kopecks>,
) -> CommandResult<Vec<SuggestionView>> {
    with_vault(&state, |v| {
        if !v.card_suggestions_enabled()? {
            return Ok(Vec::new());
        }
        let suggestions = v.card_suggestions_draft(month, category.as_deref(), amount)?;
        Ok(SuggestionView::list(&v.dictionaries()?, suggestions))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "старый пароль";
    const NEW: &str = "новый пароль";

    fn code(result: CommandResult<(Vault, Option<PhoneStart>)>) -> &'static str {
        match result {
            Ok(_) => "ok",
            Err(e) => e.code,
        }
    }

    #[test]
    fn the_phone_opens_on_the_newest_copy_and_asks_for_the_other_password() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut main = Vault::create(&dir.path().join("Семья.reestr"), OLD).unwrap();
        let copy = dir.path().join("копия.reestr");
        let working = dir.path().join(WORKING);

        // Nothing to open yet.
        assert_eq!(code(open_phone(&working, None, OLD, None)), "phone_no_copy");

        main.write_for_phone(&copy).unwrap();
        let (phone, start) = open_phone(&working, Some(&copy), OLD, None).unwrap();
        assert!(start.is_some());
        phone.close().unwrap();
        // The same copy again: the phone keeps its file.
        let (phone, start) = open_phone(&working, Some(&copy), OLD, None).unwrap();
        assert!(start.is_none());
        phone.close().unwrap();
        // Offline: the phone's own file.
        let (phone, start) = open_phone(&working, None, OLD, None).unwrap();
        assert!(start.is_none());
        phone.close().unwrap();
        assert_eq!(
            code(open_phone(&working, Some(&copy), "не тот", None)),
            "wrong_password"
        );

        // The computer changes the password and writes a newer copy.
        main.change_password(OLD, NEW).unwrap();
        main.write_for_phone(&copy).unwrap();
        assert_eq!(
            code(open_phone(&working, Some(&copy), OLD, None)),
            "phone_new_password"
        );
        assert_eq!(
            code(open_phone(&working, Some(&copy), NEW, None)),
            "phone_old_password"
        );
        let (phone, start) = open_phone(&working, Some(&copy), NEW, Some(OLD)).unwrap();
        assert!(start.is_some());
        phone.close().unwrap();
        // From now on the phone's file has the new password.
        let (phone, _) = open_phone(&working, Some(&copy), NEW, None).unwrap();
        phone.close().unwrap();
    }
}
