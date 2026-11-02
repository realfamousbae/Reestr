// The journal columns, in the order of the source sheet (PLAN §8), then «MCC», which the source
// sheet does not have (D20). «№» is the row marker.
// Changing the order requires an explicit decision by the author.

import { type GridCell, GridCellKind, type TextCell, type Theme } from "@glideapps/glide-data-grid";

import type { EntryEdit, EntryView } from "../api";
import { formatDate, parseDate } from "../format/date";
import { formatMoney, moneyEditText, parseMoney } from "../format/money";
import { type Parsed, ok } from "../format/parsed";
import { formatPercent, parsePercent, percentEditText } from "../format/percent";
import { matchOption, parseMcc, parseYesNo } from "../format/text";

export type ColumnKey =
  | "title"
  | "category"
  | "isRegular"
  | "participant"
  | "date"
  | "amount"
  | "rate"
  | "cashback"
  | "card"
  | "mcc";

export interface ColumnSpec {
  key: ColumnKey;
  title: string;
  width: number;
  align: "left" | "right" | "center";
}

export const COLUMNS: readonly ColumnSpec[] = [
  { key: "title", title: "Наименование", width: 280, align: "left" },
  { key: "category", title: "Назначение", width: 132, align: "left" },
  { key: "isRegular", title: "Регулярная", width: 96, align: "center" },
  { key: "participant", title: "Участник", width: 104, align: "left" },
  { key: "date", title: "Дата", width: 104, align: "right" },
  { key: "amount", title: "Сумма", width: 120, align: "right" },
  { key: "rate", title: "%", width: 72, align: "right" },
  { key: "cashback", title: "Кэшбэк", width: 96, align: "right" },
  { key: "card", title: "Карта", width: 124, align: "left" },
  { key: "mcc", title: "MCC", width: 64, align: "right" },
];

export interface CellContext {
  /** The sheet month, "YYYY-MM": short dates are read relative to it. */
  month: string;
  /** Names to complete typed text against; unknown names are added to the dictionary (D17). */
  categories: readonly string[];
  participants: readonly string[];
  cards: readonly string[];
}

/** Marks the cells of «Наименование», which get the editor with autocomplete. */
export const TITLE_CELL = "reestrTitleCell";

/** Marks the cells of «Карта» with the id of their row, for card suggestions (D6). */
export const CARD_CELL = "reestrCardCell";

/** Marks the cells of «Назначение», which get the list of categories. */
export const CATEGORY_CELL = "reestrCategoryCell";

/** What the cell shows. */
export function displayText(entry: EntryView, key: ColumnKey): string {
  switch (key) {
    case "title":
      return entry.title;
    case "category":
      return entry.category ?? "";
    case "isRegular":
      return entry.isRegular ? "да" : "";
    case "participant":
      return entry.participant ?? "";
    case "date":
      return entry.date === null ? "" : formatDate(entry.date);
    case "amount":
      return entry.amount === 0 ? "" : formatMoney(entry.amount);
    case "rate":
      return entry.rateBp === null ? "" : formatPercent(entry.rateBp);
    case "cashback":
      return entry.cashback === null ? "" : formatMoney(entry.cashback);
    case "card":
      return entry.card ?? "";
    case "mcc":
      return entry.mcc ?? "";
  }
}

/** What the editor starts with, and what is copied to the clipboard: easy to retype. */
export function editorText(entry: EntryView, key: ColumnKey): string {
  switch (key) {
    case "amount":
      return entry.amount === 0 ? "" : moneyEditText(entry.amount);
    case "rate":
      return entry.rateBp === null ? "" : percentEditText(entry.rateBp);
    case "cashback":
      return entry.cashback === null ? "" : moneyEditText(entry.cashback);
    default:
      return displayText(entry, key);
  }
}

/** Turns typed or pasted text into an edit for the core. */
export function parseCell(key: ColumnKey, text: string, ctx: CellContext): Parsed<EntryEdit> {
  const blank = text.trim() === "";
  switch (key) {
    case "title":
      return ok({ field: "title", value: text.trim() });
    case "category":
      return ok({ field: "category", value: matchOption(text, ctx.categories)?.value ?? null });
    case "participant":
      return ok({
        field: "participant",
        value: matchOption(text, ctx.participants)?.value ?? null,
      });
    case "card":
      return ok({ field: "card", value: matchOption(text, ctx.cards)?.value ?? null });
    case "isRegular": {
      const parsed = parseYesNo(text);
      return parsed.ok ? ok({ field: "isRegular", value: parsed.value }) : parsed;
    }
    case "date": {
      const parsed = parseDate(text, ctx.month);
      return parsed.ok ? ok({ field: "date", value: parsed.value }) : parsed;
    }
    case "amount": {
      if (blank) {
        return ok({ field: "amount", value: 0 });
      }
      const parsed = parseMoney(text);
      return parsed.ok ? ok({ field: "amount", value: parsed.value }) : parsed;
    }
    case "rate": {
      const parsed = parsePercent(text);
      return parsed.ok ? ok({ field: "rate", value: parsed.value }) : parsed;
    }
    case "cashback": {
      if (blank) {
        return ok({ field: "cashback", value: null });
      }
      const parsed = parseMoney(text);
      return parsed.ok ? ok({ field: "cashback", value: parsed.value }) : parsed;
    }
    case "mcc": {
      const parsed = parseMcc(text);
      return parsed.ok ? ok({ field: "mcc", value: parsed.value }) : parsed;
    }
  }
}

export interface CellThemes {
  /** A value derived from another column or a rule (cashback from %, % from a rule). */
  derived: Partial<Theme>;
  /** A state that needs attention: a used-up limit, an overdue payment. */
  danger: Partial<Theme>;
  /** A deadline within a week. */
  warn: Partial<Theme>;
}

function themeFor(entry: EntryView, key: ColumnKey, themes: CellThemes) {
  if (key === "cashback" && entry.cashbackSource !== "manual_amount") {
    return themes.derived;
  }
  // A rule's rate is not typed in this row either.
  if (
    key === "rate" &&
    (entry.cashbackSource === "manual_amount" || entry.cashbackSource === "rule")
  ) {
    return themes.derived;
  }
  return undefined;
}

export function cellContent(
  entry: EntryView | undefined,
  spec: ColumnSpec,
  themes: CellThemes,
): GridCell {
  // No entry: the row being typed at the bottom, which is not in the file yet.
  const data = entry === undefined ? "" : editorText(entry, spec.key);
  const themeOverride = entry === undefined ? undefined : themeFor(entry, spec.key, themes);
  const cell: TextCell = {
    kind: GridCellKind.Text,
    data,
    displayData: entry === undefined ? "" : displayText(entry, spec.key),
    copyData: data,
    allowOverlay: true,
    contentAlign: spec.align,
    ...(themeOverride === undefined ? {} : { themeOverride }),
  };
  // The markers survive the copies Glide makes of a cell when it opens the editor.
  if (spec.key === "title") {
    const titleCell: TextCell & { [TITLE_CELL]: true } = { ...cell, [TITLE_CELL]: true };
    return titleCell;
  }
  if (spec.key === "category") {
    const categoryCell: TextCell & { [CATEGORY_CELL]: true } = { ...cell, [CATEGORY_CELL]: true };
    return categoryCell;
  }
  if (spec.key === "card" && entry !== undefined) {
    const cardCell: TextCell & { [CARD_CELL]: number } = { ...cell, [CARD_CELL]: entry.id };
    return cardCell;
  }
  return cell;
}
