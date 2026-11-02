// The export of the open file (PLAN §9, D29–D31): file names and the texts around it. The
// workbook and the json are written by the core.

import type { ExportFormat } from "../api";

export const EXPORT_FORMATS: { format: ExportFormat; label: string; hint: string }[] = [
  { format: "xlsx", label: "Excel (.xlsx)", hint: "листы месяцев и «Свод», числа вместо формул" },
  { format: "json", label: "JSON (.json)", hint: "все данные файла, резервная копия" },
];

/** Shown before the save dialog: the export leaves the encryption behind (PLAN §10). */
export const EXPORT_WARNING =
  "Выгрузка не зашифрована: её сможет открыть любой, у кого окажется этот файл. " +
  "Храните её в надёжном месте и удалите, когда она станет не нужна.";

/** "Семья.reestr", "2026-10-27" → "Семья 2026-10-27.xlsx". */
export function exportName(fileName: string, format: ExportFormat, today: string): string {
  const base = fileName.replace(/\.reestr$/i, "").trim() || "Реестр";
  return `${base} ${today}.${format}`;
}

/** The chosen path with the format's extension, added when the dialog left it out. */
export function withExtension(path: string, format: ExportFormat): string {
  return path.toLowerCase().endsWith(`.${format}`) ? path : `${path}.${format}`;
}

/** "/home/oleg/Семья 2026-10-27.xlsx" → "Семья 2026-10-27.xlsx". */
export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}
