//! Reestr desktop app: Tauri commands only. Business logic lives in `reestr-core`, the data
//! file in `reestr-storage` (PLAN §4.2).

mod commands;
mod dto;
mod error;
mod state;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::default())
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
            commands::calculator_compare,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Reestr");
}

#[cfg(test)]
mod tests {
    /// The version lives in tauri.conf.json and in Cargo.toml: one number.
    #[test]
    fn the_version_is_the_same_in_both_files() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["version"], env!("CARGO_PKG_VERSION"));
    }
}
