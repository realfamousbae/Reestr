// The table of the month's cashback rules: one row per card and category or MCC (PLAN §6, D21).
// Spent and remaining sums come from the core; the UI only shows them.

import type { RuleInput, RuleOp, RuleView } from "../api";
import { formatMoney, moneyEditText, parseMoney } from "../format/money";
import { type Parsed, fail, ok } from "../format/parsed";
import { formatPercent, parsePercent, percentEditText } from "../format/percent";
import { matchOption, nameKey, parseMcc } from "../format/text";
import type { CellChange } from "../grid/edits";
import { type TableColumn, collectEdits } from "../grid/table";

export type RuleColumn = TableColumn<RuleView, RuleInput>;

/** What the category column shows for a rule without a category and an MCC. */
export const EVERYTHING = "на всё";

const money = (v: number | null) => (v === null ? "" : formatMoney(v));
const moneyEdit = (v: number | null) => (v === null ? "" : moneyEditText(v));

/** An optional limit: empty means none. */
function parseLimit(text: string, column: string): Parsed<number | null> {
  if (text.trim() === "") return ok(null);
  const parsed = parseMoney(text);
  if (!parsed.ok) return parsed;
  return parsed.value < 0 ? fail(`«${column}» не может быть отрицательным`) : ok(parsed.value);
}

/** A typed category: empty or «на всё» is a rule for everything. */
export function parseRuleCategory(text: string, categories: readonly string[]): string | null {
  const key = nameKey(text);
  if (key === "" || key === nameKey(EVERYTHING) || key === "все") return null;
  return matchOption(text, categories)?.value ?? null;
}

export function ruleColumns(cards: readonly string[], categories: readonly string[]): RuleColumn[] {
  return [
    {
      key: "card",
      title: "Карта",
      width: 130,
      show: (r) => r.card,
      parse: (text) =>
        text.trim() === ""
          ? fail("У правила должна быть карта")
          : ok({ field: "card", value: matchOption(text, cards)?.value ?? text.trim() }),
    },
    {
      key: "category",
      title: "Категория",
      width: 140,
      show: (r) => (r.mcc === null ? (r.category ?? EVERYTHING) : ""),
      edit: (r) => r.category ?? "",
      parse: (text) => ok({ field: "category", value: parseRuleCategory(text, categories) }),
    },
    {
      key: "mcc",
      title: "MCC",
      width: 64,
      align: "right",
      show: (r) => r.mcc ?? "",
      parse: (text) => {
        const parsed = parseMcc(text);
        return parsed.ok ? ok({ field: "mcc", value: parsed.value }) : parsed;
      },
    },
    {
      key: "rate",
      title: "%",
      width: 64,
      align: "right",
      show: (r) => formatPercent(r.rateBp),
      edit: (r) => percentEditText(r.rateBp),
      parse: (text) => {
        const parsed = parsePercent(text);
        return parsed.ok ? ok({ field: "rate", value: parsed.value ?? 0 }) : parsed;
      },
    },
    {
      key: "spendCap",
      title: "Лимит трат",
      width: 112,
      align: "right",
      show: (r) => money(r.spendCap),
      edit: (r) => moneyEdit(r.spendCap),
      parse: (text) => {
        const parsed = parseLimit(text, "Лимит трат");
        return parsed.ok ? ok({ field: "spendCap", value: parsed.value }) : parsed;
      },
    },
    {
      key: "cashbackCap",
      title: "Лимит кэшбэка",
      width: 124,
      align: "right",
      show: (r) => money(r.cashbackCap),
      edit: (r) => moneyEdit(r.cashbackCap),
      parse: (text) => {
        const parsed = parseLimit(text, "Лимит кэшбэка");
        return parsed.ok ? ok({ field: "cashbackCap", value: parsed.value }) : parsed;
      },
    },
    {
      key: "adjustment",
      title: "Корректировка",
      width: 124,
      align: "right",
      show: (r) => (r.adjustment === 0 ? "" : formatMoney(r.adjustment)),
      edit: (r) => (r.adjustment === 0 ? "" : moneyEditText(r.adjustment)),
      parse: (text) => {
        if (text.trim() === "") return ok({ field: "adjustment", value: 0 });
        const parsed = parseMoney(text);
        return parsed.ok ? ok({ field: "adjustment", value: parsed.value }) : parsed;
      },
    },
    {
      key: "spent",
      title: "Потрачено",
      width: 112,
      align: "right",
      show: (r) => formatMoney(r.spent),
    },
    {
      key: "cashback",
      title: "Кэшбэк",
      width: 96,
      align: "right",
      show: (r) => formatMoney(r.cashback),
    },
    {
      key: "remainingSpend",
      title: "Осталось трат",
      width: 124,
      align: "right",
      show: (r) => money(r.remainingSpend),
      status: (r) => (r.remainingSpend === 0 ? "danger" : undefined),
    },
    {
      key: "remainingCashback",
      title: "Осталось кэшбэка",
      width: 136,
      align: "right",
      show: (r) => money(r.remainingCashback),
      status: (r) => (r.remainingCashback === 0 ? "danger" : undefined),
    },
    {
      key: "notes",
      title: "Заметки",
      width: 220,
      show: (r) => r.notes ?? "",
      parse: (text) => ok({ field: "notes", value: text.trim() === "" ? null : text.trim() }),
    },
  ];
}

export interface RuleBatch {
  ops: RuleOp[];
  errors: string[];
}

/** Turns changed cells of the rules table into operations; a new rule needs its card. */
export function buildRuleOps(
  changes: readonly CellChange[],
  rows: readonly RuleView[],
  columns: readonly RuleColumn[],
): RuleBatch {
  const { edits, appends, errors } = collectEdits(changes, rows, columns);
  const ops: RuleOp[] = edits.map(({ id, edit }) => ({ op: "edit", id, edit }));
  for (const inputs of appends) {
    if (inputs.some((e) => e.field === "card")) {
      ops.push({ op: "append", edits: inputs });
    } else {
      errors.push("Новое правило начните с карты");
    }
  }
  return { ops, errors };
}
