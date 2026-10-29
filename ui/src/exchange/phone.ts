// The exchange with the phone on the computer (D39): the folder of this computer and the texts
// of the banner and the status line.

import type { Month, TakeReport } from "../api";
import { plural } from "../format/text";

const KEY = "reestr.exchangeFolder";

/** The exchange folder chosen on this computer, or null. */
export function readExchangeFolder(): string | null {
  try {
    const folder = localStorage.getItem(KEY);
    return folder === null || folder === "" ? null : folder;
  } catch {
    return null;
  }
}

export function writeExchangeFolder(folder: string | null): void {
  try {
    if (folder === null) localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, folder);
  } catch {
    // The folder is then asked for again next time.
  }
}

/** The exchange files of «Семья.reestr», named like the computer names them. */
export function exchangeFileNames(fileName: string): { forPhone: string; fromPhone: string } {
  const stem = fileName.replace(/\.reestr$/i, "");
  return {
    forPhone: `${stem} — для телефона.reestr`,
    fromPhone: `${stem} — с телефона.reestr`,
  };
}

/** "2026-11" → "11.26", the name of the month sheet. */
function sheetName(month: Month): string {
  return `${month.slice(5, 7)}.${month.slice(2, 4)}`;
}

function count(n: number, one: string, few: string, many: string): string {
  return `${n.toString()} ${plural(n, one, few, many)}`;
}

/** "2 новые строки, 1 изменена, 1 удалена", or null when the phone has nothing new. */
export function changesText(report: TakeReport): string | null {
  const parts: string[] = [];
  if (report.added > 0) {
    parts.push(count(report.added, "новая строка", "новые строки", "новых строк"));
  }
  if (report.changed > 0) parts.push(count(report.changed, "изменена", "изменены", "изменено"));
  if (report.deleted > 0) parts.push(count(report.deleted, "удалена", "удалены", "удалено"));
  return parts.length === 0 ? null : parts.join(", ");
}

/** Rows changed on both sides keep the computer's version: said so they are not a surprise. */
function keptText(report: TakeReport): string | null {
  if (report.kept === 0) return null;
  const rows = count(report.kept, "строку", "строки", "строк");
  return `${rows} меняли и здесь, и на телефоне — оставлено, как на компьютере`;
}

/** The banner after «С телефона:»: what there is to take. */
export function bannerText(report: TakeReport): string {
  const parts = [changesText(report), keptText(report)].filter((p) => p !== null);
  return `${parts.join("; ")}.`;
}

/** The status line after «Забрать». */
export function takenText(report: TakeReport): string {
  const changes = changesText(report);
  const kept = keptText(report);
  if (changes === null) {
    return kept === null ? "С телефона нечего забрать." : `${capitalize(kept)}.`;
  }
  const months = report.months.map(sheetName).join(", ");
  const where = months === "" ? "" : ` (${months})`;
  const tail = kept === null ? "" : ` ${capitalize(kept)}.`;
  return `Забрано с телефона${where}: ${changes}.${tail} Ctrl+Z отменит.`;
}

function capitalize(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** The problem with the phone's file in words, by error code. */
export function problemText(code: string): string {
  switch (code) {
    case "other_file":
      return "Файл с телефона сделан из другого файла Реестра. На телефоне нужно открыть копию этого файла.";
    case "not_from_phone":
      return "В папке обмена лежит не файл телефона.";
    case "locked":
      return "Файл с телефона сейчас занят — наверное, Яндекс Диск ещё его загружает. Попробуйте позже.";
    case "not_found":
      return "Папка обмена с телефоном не найдена. Выберите её снова в «Настройках».";
    default:
      return "Не удалось прочитать файл с телефона.";
  }
}
