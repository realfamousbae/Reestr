import { isTauri } from "@tauri-apps/api/core";

import { plural } from "../format/text";
import { tauriBackend } from "./tauri";
import type { Backend, CommandError, Created, DictKind } from "./types";

export type * from "./types";

/** The backend exists only inside the desktop app. */
export function loadBackend(): Backend | null {
  return isTauri() ? tauriBackend : null;
}

function isCommandError(error: unknown): error is CommandError {
  return (
    typeof error === "object" && error !== null && "code" in error && typeof error.code === "string"
  );
}

const MESSAGES: Record<string, string> = {
  wrong_password: "Неверный пароль. Проверьте раскладку и Caps Lock.",
  not_found: "Файл не найден. Возможно, его переместили или флешка отключена.",
  already_exists:
    "Файл с таким именем уже есть. Реестр не перезаписывает файлы — выберите новое имя.",
  empty_password: "Введите пароль.",
  not_reestr_file: "Это не файл Реестра.",
  not_encrypted: "Это не файл Реестра: он не зашифрован.",
  locked: "Этот файл уже открыт в другом окне Реестра.",
  too_new: "Файл создан более новой версией Реестра. Обновите программу.",
  io: "Не удалось прочитать или записать файл. Проверьте, что диск доступен.",
  no_file_open: "Файл закрыт. Откройте его снова.",
  invalid_edit: "Изменение не принято: процент кэшбэка больше 100.",
  unknown_id: "Строка уже удалена. Экран обновлён.",
  empty_name: "Название не может быть пустым.",
  invalid_field: "Это поле здесь не меняется.",
  corrupt_log: "Журнал изменений повреждён: отменить это действие нельзя.",
  workbook_unreadable:
    "Не удалось прочитать книгу Excel. Сохраните её в формате .xlsx и попробуйте снова.",
  workbook_no_service: "В книге нет листа «Служебный» — похоже, это другая таблица.",
  workbook_no_months: "В книге нет листов месяцев вида «07.26».",
  import_expired: "Книгу нужно выбрать заново.",
  negative_limit: "Лимит не может быть отрицательным.",
  duplicate_rule: "Для этой карты и категории правило в этом месяце уже есть.",
  end_before_start: "Грейс не может закончиться раньше, чем начался.",
  amount_not_positive: "Сумма должна быть больше нуля.",
  days_not_positive: "Число дней должно быть больше нуля.",
  empty_title: "Напишите, о чём напоминание.",
  no_tranche: "У карты нет ни одного грейса — сначала добавьте его.",
  invalid_date: "Не удалось определить сегодняшнюю дату.",
  export_failed: "Не удалось собрать выгрузку.",
  phone_file:
    "Это рабочий файл телефона, а не основной файл. Откройте основной файл — строки с " +
    "телефона забираются в нём кнопкой «Забрать».",
  snapshot_file:
    "Это копия для телефона, правки в ней пропадут. Откройте основной файл, из которого она " +
    "сделана.",
  not_from_phone: "Это не файл с телефона.",
  not_for_phone: "Это не копия для телефона.",
  other_file:
    "Файл с телефона сделан из другого файла Реестра. На телефоне нужно открыть копию этого файла.",
  older_copy: "Копия для телефона старее той, что уже на телефоне.",
  computer_only: "Это делается на компьютере.",
  not_phone_row: "Эта строка уже на компьютере: править её можно там.",
  phone_not_set_up: "Сначала выберите файлы обмена на Яндекс Диске.",
  phone_no_copy:
    "Не удалось прочитать копию с компьютера. Проверьте интернет и что Яндекс Диск на телефоне " +
    "видит файл, затем попробуйте снова.",
  phone_old_password: "Введите и прежний пароль.",
  phone_new_password: "Введите новый пароль с компьютера.",
};

/** The code of a command error, or null for anything else. */
export function errorCode(error: unknown): string | null {
  return isCommandError(error) ? error.code : null;
}

/** A Russian message for any error a command can reject with. */
export function errorMessage(error: unknown): string {
  if (isCommandError(error)) {
    if (error.code === "duplicate_name") {
      return `«${error.detail ?? ""}» уже есть. Выберите другое название.`;
    }
    if (error.code === "in_use") {
      const n = Number(error.detail ?? "0");
      return `Нельзя удалить: используется в ${n.toString()} ${plural(n, "строке", "строках", "строках")}. Можно снять отметку «активна».`;
    }
    return MESSAGES[error.code] ?? `Ошибка: ${error.message}`;
  }
  return error instanceof Error ? `Ошибка: ${error.message}` : "Неизвестная ошибка.";
}

const KIND_NAMES: Record<DictKind, string> = {
  participant: "участник",
  bank: "банк",
  product: "карта",
  group: "группа",
  category: "категория",
  template: "шаблон",
};

/** "Добавлена категория «кино»", or a list when several items were created. */
export function createdMessage(created: readonly Created[]): string | null {
  if (created.length === 0) {
    return null;
  }
  const items = created.map((c) => `${KIND_NAMES[c.kind]} «${c.name}»`);
  return `В справочник добавлено: ${items.join(", ")}. Ctrl+Z отменит.`;
}

const ACTION_NAMES: Record<string, string> = {
  edit: "правка",
  append: "новые строки",
  delete: "удаление строк",
  templates: "регулярные платежи",
  dictionary: "изменение справочника",
  import: "импорт",
  rules: "правила кэшбэка",
  products: "продукты и сроки",
  inflows: "поступления",
  plan: "план дохода",
  phone: "строки с телефона",
};

export function actionName(label: string): string {
  return ACTION_NAMES[label] ?? label;
}
