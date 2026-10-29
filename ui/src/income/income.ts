// Inflows and the income plan of a month (PLAN §5, stage 3b). Sums and balances come from the
// core; the UI parses input and shows text.

import type { InflowInput, InflowOp, InflowView, Month } from "../api";
import { formatDate, parseDate } from "../format/date";
import { formatMoney, moneyEditText, parseMoney } from "../format/money";
import { type Parsed, fail, ok } from "../format/parsed";
import { matchOption } from "../format/text";
import type { CellChange } from "../grid/edits";
import { type TableColumn, collectEdits } from "../grid/table";

export interface InflowBatch {
  ops: InflowOp[];
  errors: string[];
}

/** "2026-01" → "01.26", the name of the month's sheet. */
export function monthTitle(month: Month): string {
  const [year = "", m = ""] = month.split("-");
  return `${m}.${year.slice(2)}`;
}

/** The income plan as typed: empty removes this month's own plan, negative is refused. */
export function parsePlan(text: string): Parsed<number | null> {
  if (text.trim() === "") return ok(null);
  const parsed = parseMoney(text);
  if (!parsed.ok) return parsed;
  return parsed.value < 0 ? fail("План дохода не может быть меньше нуля") : ok(parsed.value);
}

function optional(text: string): string | null {
  const t = text.trim();
  return t === "" ? null : t;
}

export function inflowColumns(
  month: Month,
  participants: readonly string[],
  cards: readonly string[],
): TableColumn<InflowView, InflowInput>[] {
  return [
    {
      key: "amount",
      title: "Сумма",
      width: 160,
      align: "right",
      show: (i) => formatMoney(i.amount),
      edit: (i) => moneyEditText(i.amount),
      parse: (text) => {
        const parsed = parseMoney(text);
        if (!parsed.ok) return parsed;
        return parsed.value <= 0
          ? fail("Сумма поступления должна быть больше нуля")
          : ok({ field: "amount", value: parsed.value });
      },
    },
    {
      key: "date",
      title: "Дата",
      width: 104,
      align: "right",
      show: (i) => (i.date === null ? "" : formatDate(i.date)),
      parse: (text) => {
        const parsed = parseDate(text, month);
        return parsed.ok ? ok({ field: "date", value: parsed.value }) : parsed;
      },
    },
    {
      key: "source",
      title: "Источник",
      width: 200,
      show: (i) => i.source,
      parse: (text) => ok({ field: "source", value: text.trim() }),
    },
    {
      key: "participant",
      title: "Участник",
      width: 120,
      show: (i) => i.participant ?? "",
      parse: (text) =>
        ok({ field: "participant", value: matchOption(text, participants)?.value ?? null }),
    },
    {
      key: "card",
      title: "Карта",
      width: 130,
      show: (i) => i.card ?? "",
      parse: (text) => ok({ field: "card", value: matchOption(text, cards)?.value ?? null }),
    },
    {
      key: "notes",
      title: "Заметки",
      width: 220,
      show: (i) => i.notes ?? "",
      parse: (text) => ok({ field: "notes", value: optional(text) }),
    },
  ];
}

/** Changed cells → inflow ops; a new inflow starts with its amount. */
export function inflowOps(
  changes: readonly CellChange[],
  inflows: readonly InflowView[],
  columns: readonly TableColumn<InflowView, InflowInput>[],
): InflowBatch {
  const { edits, appends, errors } = collectEdits(changes, inflows, columns);
  const ops: InflowOp[] = edits.map(({ id, edit }) => ({ op: "edit", id, edit }));
  for (const inputs of appends) {
    if (inputs.some((e) => e.field === "amount")) {
      ops.push({ op: "append", edits: inputs });
    } else {
      errors.push("Новое поступление начните с суммы");
    }
  }
  return { ops, errors };
}
