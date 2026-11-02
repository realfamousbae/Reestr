import { invoke } from "@tauri-apps/api/core";
import { ask, open, save } from "@tauri-apps/plugin-dialog";

import { withExtension } from "../export/exportFile";
import type { Backend } from "./types";

const FILTERS = [{ name: "Файл Реестра", extensions: ["reestr"] }];

export const tauriBackend: Backend = {
  vaultCreate: (path, password) => invoke("vault_create", { path, password }),
  vaultOpen: (path, password, today) => invoke("vault_open", { path, password, today }),
  fileAbout: () => invoke("file_about"),
  changePassword: (current, next) => invoke("vault_change_password", { current, new: next }),
  vaultClose: () => invoke("vault_close"),
  dictionaries: () => invoke("dictionaries"),
  dictEdit: (kind, ops) => invoke("dict_edit", { kind, ops }),
  journalMonths: (current) => invoke("journal_months", { current }),
  journalAddMonth: (month, current) => invoke("journal_add_month", { month, current }),
  journalMonth: (month) => invoke("journal_month", { month }),
  journalTitles: () => invoke("journal_titles"),
  journalApply: (month, ops) => invoke("journal_apply", { month, ops }),
  journalDelete: (month, ids) => invoke("journal_delete", { month, ids }),
  journalAddTemplates: (month, ids) => invoke("journal_add_templates", { month, ids }),
  rulesImpact: (month, ops) => invoke("rules_impact", { month, ops }),
  rulesApply: (month, ops, recompute) => invoke("rules_apply", { month, ops, recompute }),
  cardSuggestions: (month, entry) => invoke("card_suggestions", { month, entry }),
  setCardSuggestions: (enabled) => invoke("settings_card_suggestions", { enabled }),
  inflowsApply: (month, ops) => invoke("inflows_apply", { month, ops }),
  planSet: (month, amount) => invoke("plan_set", { month, amount }),
  calculatorCompare: (amount, cashbackRate, savingsRate, days) =>
    invoke("calculator_compare", { amount, cashbackRate, savingsRate, days }),
  summaryYears: () => invoke("summary_years"),
  summaryYear: (year, grouped) => invoke("summary_year", { year, grouped }),
  productsView: (today) => invoke("products_view", { today }),
  productsApply: (ops, today) => invoke("products_apply", { ops, today }),
  historyUndo: (month) => invoke("history_undo", { month }),
  historyRedo: (month) => invoke("history_redo", { month }),
  importPreview: (path) => invoke("import_preview", { path }),
  importApply: (cards) => invoke("import_apply", { cards }),
  importCancel: () => invoke("import_cancel"),
  exportFile: (path, format, today) => invoke("export_file", { path, format, today }),

  async pickExportFile(format, defaultName) {
    const path = await save({
      title: format === "xlsx" ? "Экспорт в Excel" : "Экспорт в JSON",
      defaultPath: defaultName,
      filters: [
        format === "xlsx"
          ? { name: "Книга Excel", extensions: ["xlsx"] }
          : { name: "JSON", extensions: ["json"] },
      ],
    });
    return path === null ? null : withExtension(path, format);
  },

  async pickWorkbook() {
    const path = await open({
      title: "Книга Excel для импорта",
      multiple: false,
      directory: false,
      filters: [{ name: "Книга Excel", extensions: ["xlsx", "xlsm", "xls"] }],
    });
    return typeof path === "string" ? path : null;
  },

  async pickNewFile() {
    const path = await save({
      title: "Новый файл Реестра",
      defaultPath: "Семья.reestr",
      filters: FILTERS,
    });
    if (path === null) {
      return null;
    }
    return path.toLowerCase().endsWith(".reestr") ? path : `${path}.reestr`;
  },

  async pickExistingFile() {
    const path = await open({
      title: "Открыть файл Реестра",
      multiple: false,
      directory: false,
      filters: FILTERS,
    });
    return typeof path === "string" ? path : null;
  },

  confirm: (message, title, labels) =>
    ask(message, {
      title,
      kind: "warning",
      ...(labels === undefined ? {} : { okLabel: labels.ok, cancelLabel: labels.cancel }),
    }),
};
