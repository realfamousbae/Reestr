// The tables of the «Продукты» and «Сроки» sheets: tranches, repayments, purchases of a credit
// card, reminders. Debts and days come from the core; the UI parses input and shows text.

import type {
  CreditCardView,
  DeadlineInput,
  ProductOp,
  PurchaseView,
  ReminderView,
  RepaymentInput,
  RepaymentView,
  TrancheInput,
  TrancheView,
} from "../api";
import { formatDate, parseDate } from "../format/date";
import { formatMoney, moneyEditText, parseMoney } from "../format/money";
import { type Parsed, fail, ok } from "../format/parsed";
import { matchOption, parseYesNo } from "../format/text";
import type { CellChange } from "../grid/edits";
import { type TableColumn, collectEdits } from "../grid/table";
import { trancheLabel, trancheStatusText, trancheTone } from "./labels";

export interface Batch {
  ops: ProductOp[];
  errors: string[];
}

/** A required date, typed like in the journal ("5.7", "05.07.26") relative to `month`. */
function parseRequiredDate(text: string, month: string): Parsed<string> {
  const parsed = parseDate(text, month);
  if (!parsed.ok) return parsed;
  return parsed.value === null ? fail("Укажите дату") : ok(parsed.value);
}

const money = (v: number) => (v === 0 ? "" : formatMoney(v));

export function trancheColumns(month: string): TableColumn<TrancheView, TrancheInput>[] {
  return [
    {
      key: "start",
      title: "Начало",
      width: 120,
      align: "right",
      show: (t) => formatDate(t.start),
      parse: (text) => {
        const parsed = parseRequiredDate(text, month);
        return parsed.ok ? ok({ field: "start", value: parsed.value }) : parsed;
      },
    },
    {
      key: "end",
      title: "Конец грейса",
      width: 116,
      align: "right",
      show: (t) => formatDate(t.end),
      parse: (text) => {
        const parsed = parseRequiredDate(text, month);
        return parsed.ok ? ok({ field: "end", value: parsed.value }) : parsed;
      },
    },
    {
      key: "status",
      title: "Срок",
      width: 160,
      show: trancheStatusText,
      status: trancheTone,
    },
    {
      key: "purchases",
      title: "Покупок",
      width: 80,
      align: "right",
      show: (t) => t.purchases.toString(),
    },
    {
      key: "adjustment",
      title: "Корректировка",
      width: 120,
      align: "right",
      show: (t) => money(t.adjustment),
      edit: (t) => (t.adjustment === 0 ? "" : moneyEditText(t.adjustment)),
      parse: (text) => {
        if (text.trim() === "") return ok({ field: "adjustment", value: 0 });
        const parsed = parseMoney(text);
        return parsed.ok ? ok({ field: "adjustment", value: parsed.value }) : parsed;
      },
    },
    { key: "debt", title: "Долг", width: 104, align: "right", show: (t) => formatMoney(t.debt) },
    {
      key: "repaid",
      title: "Погашено",
      width: 104,
      align: "right",
      show: (t) => formatMoney(t.repaid),
    },
    {
      key: "remaining",
      title: "Осталось",
      width: 104,
      align: "right",
      show: (t) => formatMoney(t.remaining),
      status: (t) => (t.status === "overdue" ? "danger" : undefined),
    },
    {
      key: "notes",
      title: "Заметки",
      width: 200,
      show: (t) => t.notes ?? "",
      parse: (text) => ok({ field: "notes", value: text.trim() === "" ? null : text.trim() }),
    },
  ];
}

/** Tranches of the card by their labels, for typing «01.06» into a «Грейс» cell. */
function trancheByLabel(card: CreditCardView, text: string): number | null {
  const labels = card.tranches.map(trancheLabel);
  const match = matchOption(text, labels);
  if (match?.known !== true) return null;
  return card.tranches.find((t) => trancheLabel(t) === match.value)?.id ?? null;
}

function trancheName(card: CreditCardView, id: number | null): string {
  const t = card.tranches.find((x) => x.id === id);
  return t === undefined ? "" : trancheLabel(t);
}

export function repaymentColumns(
  card: CreditCardView,
  month: string,
): TableColumn<RepaymentView, RepaymentInput>[] {
  return [
    {
      key: "amount",
      title: "Сумма",
      width: 140,
      align: "right",
      show: (r) => formatMoney(r.amount),
      edit: (r) => moneyEditText(r.amount),
      parse: (text) => {
        const parsed = parseMoney(text);
        if (!parsed.ok) return parsed;
        return parsed.value <= 0
          ? fail("Сумма погашения должна быть больше нуля")
          : ok({ field: "amount", value: parsed.value });
      },
    },
    {
      key: "date",
      title: "Дата",
      width: 104,
      align: "right",
      show: (r) => formatDate(r.date),
      parse: (text) => {
        const parsed = parseRequiredDate(text, month);
        return parsed.ok ? ok({ field: "date", value: parsed.value }) : parsed;
      },
    },
    {
      key: "tranche",
      title: "Грейс",
      width: 150,
      show: (r) => trancheName(card, r.trancheId),
      parse: (text) => {
        const id = trancheByLabel(card, text);
        return id === null
          ? fail(`Грейс «${text.trim()}» не найден: начните с даты его начала`)
          : ok({ field: "tranche", value: id });
      },
    },
    {
      key: "notes",
      title: "Заметки",
      width: 180,
      show: (r) => r.notes ?? "",
      parse: (text) => ok({ field: "notes", value: text.trim() === "" ? null : text.trim() }),
    },
  ];
}

/** A purchase cell edit: the tranche chosen by hand, or `null` for the binding by date. */
export interface Binding {
  tranche: number | null;
}

/** A purchase as a table row: its id is the journal entry's. */
export type PurchaseRow = PurchaseView & { id: number };

export function purchaseRows(card: CreditCardView): PurchaseRow[] {
  return card.purchases.map((p) => ({ ...p, id: p.entryId }));
}

export function purchaseColumns(card: CreditCardView): TableColumn<PurchaseRow, Binding>[] {
  return [
    {
      key: "date",
      title: "Дата",
      width: 104,
      align: "right",
      show: (p) => (p.date === null ? "" : formatDate(p.date)),
    },
    { key: "title", title: "Наименование", width: 180, show: (p) => p.title },
    {
      key: "amount",
      title: "Сумма",
      width: 104,
      align: "right",
      show: (p) => formatMoney(p.amount),
    },
    {
      key: "tranche",
      title: "Грейс",
      width: 150,
      show: (p) => trancheName(card, p.trancheId),
      edit: (p) => (p.manual ? trancheName(card, p.trancheId) : ""),
      parse: (text) => {
        if (text.trim() === "") return ok({ tranche: null });
        const id = trancheByLabel(card, text);
        return id === null
          ? fail(`Грейс «${text.trim()}» не найден: начните с даты его начала`)
          : ok({ tranche: id });
      },
    },
    {
      key: "binding",
      title: "Привязка",
      width: 110,
      show: (p) => (p.manual ? "вручную" : p.trancheId === null ? "нет грейса" : "по дате"),
      status: (p) => (p.trancheId === null ? "warn" : undefined),
    },
  ];
}

export function reminderColumns(
  cards: readonly string[],
  month: string,
): TableColumn<ReminderView, DeadlineInput>[] {
  return [
    {
      key: "title",
      title: "Что сделать",
      width: 280,
      show: (r) => r.title,
      parse: (text) =>
        text.trim() === ""
          ? fail("Напишите, о чём напоминание")
          : ok({ field: "title", value: text.trim() }),
    },
    {
      key: "date",
      title: "Дата",
      width: 104,
      align: "right",
      show: (r) => formatDate(r.date),
      parse: (text) => {
        const parsed = parseRequiredDate(text, month);
        return parsed.ok ? ok({ field: "date", value: parsed.value }) : parsed;
      },
    },
    {
      key: "card",
      title: "Карта",
      width: 130,
      show: (r) => r.card ?? "",
      parse: (text) => ok({ field: "card", value: matchOption(text, cards)?.value ?? null }),
    },
    {
      key: "done",
      title: "Сделано",
      width: 90,
      align: "center",
      show: (r) => (r.isDone ? "да" : ""),
      parse: (text) => {
        const parsed = parseYesNo(text, "Сделано");
        return parsed.ok ? ok({ field: "done", value: parsed.value }) : parsed;
      },
    },
  ];
}

export function trancheOps(
  changes: readonly CellChange[],
  card: CreditCardView,
  columns: readonly TableColumn<TrancheView, TrancheInput>[],
): Batch {
  const { edits, appends, errors } = collectEdits(changes, card.tranches, columns);
  const ops: ProductOp[] = edits.map(({ id, edit }) => ({ op: "editTranche", id, edit }));
  for (const inputs of appends) {
    ops.push({ op: "addTranche", product: card.productId, edits: inputs });
  }
  return { ops, errors };
}

export function repaymentOps(
  changes: readonly CellChange[],
  card: CreditCardView,
  columns: readonly TableColumn<RepaymentView, RepaymentInput>[],
): Batch {
  const { edits, appends, errors } = collectEdits(changes, card.repayments, columns);
  const ops: ProductOp[] = edits.map(({ id, edit }) => ({ op: "editRepayment", id, edit }));
  for (const inputs of appends) {
    if (inputs.some((e) => e.field === "amount")) {
      ops.push({ op: "addRepayment", product: card.productId, edits: inputs });
    } else {
      errors.push("Новое погашение начните с суммы");
    }
  }
  return { ops, errors };
}

export function bindingOps(
  changes: readonly CellChange[],
  card: CreditCardView,
  columns: readonly TableColumn<PurchaseRow, Binding>[],
): Batch {
  const { edits, errors } = collectEdits(changes, purchaseRows(card), columns);
  const ops: ProductOp[] = edits.map(({ id, edit }) => ({
    op: "bind",
    entry: id,
    tranche: edit.tranche,
  }));
  return { ops, errors };
}

export function reminderOps(
  changes: readonly CellChange[],
  reminders: readonly ReminderView[],
  columns: readonly TableColumn<ReminderView, DeadlineInput>[],
): Batch {
  const { edits, appends, errors } = collectEdits(changes, reminders, columns);
  const ops: ProductOp[] = edits.map(({ id, edit }) => ({ op: "editDeadline", id, edit }));
  for (const inputs of appends) {
    if (inputs.some((e) => e.field === "title")) {
      ops.push({ op: "addDeadline", edits: inputs });
    } else {
      errors.push("Новое напоминание начните с того, что сделать");
    }
  }
  return { ops, errors };
}
