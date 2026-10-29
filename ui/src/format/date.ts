// Dates travel as ISO "YYYY-MM-DD" and are shown as "DD.MM.YYYY".

import { type Parsed, fail, ok } from "./parsed";

function pad2(n: number): string {
  return n.toString().padStart(2, "0");
}

function isValidDate(year: number, month: number, day: number): boolean {
  if (month < 1 || month > 12 || day < 1) {
    return false;
  }
  // Day 0 of the next month is the last day of this one; no time zones involved.
  const daysInMonth = new Date(Date.UTC(year, month, 0)).getUTCDate();
  return day <= daysInMonth;
}

/** The user's today as "YYYY-MM-DD", in local time. */
export function todayIso(now: Date = new Date()): string {
  return `${now.getFullYear().toString()}-${pad2(now.getMonth() + 1)}-${pad2(now.getDate())}`;
}

/** "2026-07-05" → "05.07.2026". */
export function formatDate(iso: string): string {
  const [year, month, day] = iso.split("-");
  return `${day ?? ""}.${month ?? ""}.${year ?? ""}`;
}

/**
 * Parses a typed date relative to the month of the sheet (`YYYY-MM`):
 * "5" → the 5th of that month, "5.7" → 5 July of that year, "05.07.26", "05.07.2026",
 * "2026-07-05". Separators may be ".", "/", "-" or ",". Empty input clears the date.
 */
export function parseDate(input: string, sheetMonth: string): Parsed<string | null> {
  const text = input.trim();
  if (text === "") {
    return ok(null);
  }
  const [sheetYear = 0, sheetMonthNumber = 0] = sheetMonth.split("-").map(Number);
  let year = sheetYear;
  let month = sheetMonthNumber;
  let day: number;

  const iso = /^(\d{4})-(\d{1,2})-(\d{1,2})$/.exec(text);
  const parts = text.split(/[./,-]/);
  if (iso !== null) {
    year = Number(iso[1]);
    month = Number(iso[2]);
    day = Number(iso[3]);
  } else if (parts.length <= 3 && parts.every((p) => /^\d{1,4}$/.test(p))) {
    const [d, m, y] = parts;
    day = Number(d);
    if (m !== undefined && m !== "") {
      month = Number(m);
    }
    if (y !== undefined && y !== "") {
      if (y.length === 2) {
        year = 2000 + Number(y);
      } else if (y.length === 4) {
        year = Number(y);
      } else {
        return fail(`Не удалось распознать дату «${text}»`);
      }
    }
  } else {
    return fail(`Не удалось распознать дату «${text}»`);
  }

  if (!isValidDate(year, month, day)) {
    return fail(`Такой даты нет: «${text}»`);
  }
  return ok(`${year.toString()}-${pad2(month)}-${pad2(day)}`);
}
