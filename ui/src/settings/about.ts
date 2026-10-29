// Texts of the «О файле и пароле» page: sizes, dates, the encryption in words.

import type { AboutView } from "../api";
import { formatDate } from "../format/date";

/** 84 000 → "82 КБ", 1 300 000 → "1,2 МБ". */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes.toString()} Б`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${Math.round(kb).toString()} КБ`;
  const mb = kb / 1024;
  return `${mb.toFixed(1).replace(".", ",")} МБ`;
}

/** "2026-10-27T20:03:02Z" → "27.10.2026". */
export function createdText(createdAt: string): string {
  return /^\d{4}-\d{2}-\d{2}/.test(createdAt) ? formatDate(createdAt.slice(0, 10)) : "—";
}

/** The encryption of the file in one line. */
export function cipherText(c: AboutView["cipher"]): string {
  const iterations = Number(c.kdfIterations);
  const count = Number.isFinite(iterations)
    ? iterations.toLocaleString("ru-RU").replace(/\s/g, " ")
    : c.kdfIterations;
  return (
    `SQLCipher ${c.version}: весь файл зашифрован AES-256, ключ получается из пароля ` +
    `(${c.kdfAlgorithm.replaceAll("_", "-")}, ${count} повторений), ` +
    `каждая страница проверяется ${c.hmacAlgorithm.replaceAll("_", "-")}.`
  );
}
