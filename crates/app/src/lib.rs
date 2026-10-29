//! Reestr desktop app: Tauri commands only. Business logic lives in `reestr-core`, the data
//! file in `reestr-storage` (PLAN §4.2).

mod commands;
mod dto;
mod error;
mod phone;
mod state;

use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_reestr_files::init())
        .manage(state::AppState::default())
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                commands::on_exit(&window.state::<state::AppState>());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_create,
            commands::vault_open,
            commands::vault_close,
            commands::vault_info,
            commands::file_about,
            commands::vault_change_password,
            commands::dictionaries,
            commands::dict_edit,
            commands::journal_months,
            commands::journal_add_month,
            commands::journal_month,
            commands::journal_titles,
            commands::journal_apply,
            commands::journal_delete,
            commands::journal_add_templates,
            commands::rules_impact,
            commands::rules_apply,
            commands::card_suggestions,
            commands::settings_card_suggestions,
            commands::products_view,
            commands::products_apply,
            commands::inflows_apply,
            commands::plan_set,
            commands::summary_years,
            commands::summary_year,
            commands::history_undo,
            commands::history_redo,
            commands::import_preview,
            commands::import_apply,
            commands::import_cancel,
            commands::export_file,
            commands::exchange_folder,
            commands::exchange_check,
            commands::exchange_take,
            phone::app_platform,
            phone::phone_setup,
            phone::phone_choose,
            phone::phone_open,
            phone::phone_send,
            phone::phone_month,
            phone::phone_suggest,
            phone::phone_last,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Reestr");
}

#[cfg(test)]
mod tests {
    /// Android takes the version from tauri.conf.json, Windows from Cargo.toml: one number.
    #[test]
    fn the_version_is_the_same_on_every_platform() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["version"], env!("CARGO_PKG_VERSION"));
    }
}
