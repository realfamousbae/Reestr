// The entry form of the phone (D40): what was typed, read into journal edits. Money is
// parsed here only; totals and cashback come from the core.

import type { EntryEdit, EntryView, Kopecks, Month } from "../api";
import { moneyEditText, parseMoney } from "../format/money";
import { fail, ok, type Parsed } from "../format/parsed";
import { nameKey } from "../format/text";

/** The form as typed: text fields, a date from the date picker ("YYYY-MM-DD"). */
export interface DraftForm {
  title: string;
  amount: string;
  category: string;
  card: string;
  participant: string;
  date: string;
  regular: boolean;
}

export interface Draft {
  title: string;
  amount: Kopecks;
  category: string | null;
  card: string | null;
  participant: string | null;
  date: string | null;
  regular: boolean;
}

export function emptyForm(today: string): DraftForm {
  return {
    title: "",
    amount: "",
    category: "",
    card: "",
    participant: "",
    date: today,
    regular: false,
  };
}

const orNull = (text: string) => (text.trim() === "" ? null : text.trim());

export function readDraft(form: DraftForm): Parsed<Draft> {
  const title = form.title.trim();
  if (title === "") return fail("Напишите наименование.");
  if (form.amount.trim() === "") return fail("Введите сумму.");
  const amount = parseMoney(form.amount);
  if (!amount.ok) return amount;
  return ok({
    title,
    amount: amount.value,
    category: orNull(form.category),
    card: orNull(form.card),
    participant: orNull(form.participant),
    date: orNull(form.date),
    regular: form.regular,
  });
}

/** "2026-10-28" → "2026-10": a new row goes to the month of its date. */
export function monthOf(date: string | null, fallback: Month): Month {
  return date !== null && /^\d{4}-\d{2}-\d{2}$/.test(date) ? date.slice(0, 7) : fallback;
}

/** Every field of a new row. */
export function appendEdits(d: Draft): EntryEdit[] {
  return [
    { field: "title", value: d.title },
    { field: "category", value: d.category },
    { field: "isRegular", value: d.regular },
    { field: "participant", value: d.participant },
    { field: "date", value: d.date },
    { field: "amount", value: d.amount },
    { field: "card", value: d.card },
  ];
}

/** Only the fields that changed in a row of the phone. */
export function changedEdits(d: Draft, e: EntryView): EntryEdit[] {
  const edits: EntryEdit[] = [];
  if (d.title !== e.title) edits.push({ field: "title", value: d.title });
  if (d.category !== e.category) edits.push({ field: "category", value: d.category });
  if (d.regular !== e.isRegular) edits.push({ field: "isRegular", value: d.regular });
  if (d.participant !== e.participant) {
    edits.push({ field: "participant", value: d.participant });
  }
  if (d.date !== e.date) edits.push({ field: "date", value: d.date });
  if (d.amount !== e.amount) edits.push({ field: "amount", value: d.amount });
  if (d.card !== e.card) edits.push({ field: "card", value: d.card });
  return edits;
}

/** A row put back into the form for editing. */
export function formOf(e: EntryView, today: string): DraftForm {
  return {
    title: e.title,
    amount: e.amount === 0 ? "" : moneyEditText(e.amount),
    category: e.category ?? "",
    card: e.card ?? "",
    participant: e.participant ?? "",
    date: e.date ?? today,
    regular: e.isRegular,
  };
}

/** Fills the empty fields from the last row with the same title, as the journal does. */
export function fillFromLast(form: DraftForm, last: EntryView): DraftForm {
  return {
    ...form,
    category: form.category === "" ? (last.category ?? "") : form.category,
    card: form.card === "" ? (last.card ?? "") : form.card,
    participant: form.participant === "" ? (last.participant ?? "") : form.participant,
    regular: form.regular || last.isRegular,
  };
}

/** Past titles for what is being typed: those starting with it first, then containing it. */
export function titleMatches(typed: string, history: readonly string[], limit = 5): string[] {
  const key = nameKey(typed);
  if (key === "") return [];
  const starts: string[] = [];
  const contains: string[] = [];
  for (const title of history) {
    const titleKey = nameKey(title);
    if (titleKey === key) continue;
    if (titleKey.startsWith(key)) starts.push(title);
    else if (titleKey.includes(key)) contains.push(title);
  }
  return [...starts, ...contains].slice(0, limit);
}
