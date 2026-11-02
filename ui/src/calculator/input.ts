// «Калькулятор»: turns what is typed into a request for the core. Parsing only; every amount of
// money in the answer comes from the core (PLAN §15).

import { parseMoney } from "../format/money";
import { parsePercent } from "../format/percent";

/** The longest term, as in the core (`MAX_DAYS`). */
export const MAX_DAYS = 3650;
/** The largest amount in kopecks, as in the core (`MAX_AMOUNT`). */
export const MAX_AMOUNT = 100_000_000_000_000;

export interface CalcFields {
  amount: string;
  cashback: string;
  savings: string;
  days: string;
}

export type CalcField = keyof CalcFields;

/** The arguments of the command: kopecks, basis points, days. */
export interface CalcRequest {
  amount: number;
  cashbackRate: number;
  savingsRate: number;
  days: number;
}

export interface CalcParse {
  /** Set when every field is filled and valid. */
  request: CalcRequest | null;
  /** A message under each field that is filled but not valid. */
  errors: Partial<Record<CalcField, string>>;
}

type Field<T> = { kind: "empty" } | { kind: "error"; message: string } | { kind: "ok"; value: T };

function amountField(text: string): Field<number> {
  if (text.trim() === "") return { kind: "empty" };
  const parsed = parseMoney(text);
  if (!parsed.ok) return { kind: "error", message: parsed.message };
  if (parsed.value < 0) return { kind: "error", message: "Сумма не может быть отрицательной" };
  if (parsed.value > MAX_AMOUNT) return { kind: "error", message: "Слишком большая сумма" };
  return { kind: "ok", value: parsed.value };
}

function rateField(text: string): Field<number> {
  const parsed = parsePercent(text);
  if (!parsed.ok) {
    // The shared parser words the upper bound for a cashback; here it serves both rates.
    const message = parsed.message.startsWith("Процент кэшбэка")
      ? "Процент не может быть больше 100"
      : parsed.message;
    return { kind: "error", message };
  }
  return parsed.value === null ? { kind: "empty" } : { kind: "ok", value: parsed.value };
}

/** Whole days: "30", "0"; a term over ten years is refused. */
export function parseDays(input: string): Field<number> {
  const text = input.trim();
  if (text === "") return { kind: "empty" };
  if (!/^\d+$/.test(text)) {
    return { kind: "error", message: `Срок — целое число дней, а не «${text}»` };
  }
  const days = Number(text);
  if (days > MAX_DAYS) {
    return { kind: "error", message: `Срок не больше ${MAX_DAYS.toString()} дней (10 лет)` };
  }
  return { kind: "ok", value: days };
}

/** Parses the four fields. Empty fields are not errors: the answer just waits for them. */
export function parseCalcFields(fields: CalcFields): CalcParse {
  const amount = amountField(fields.amount);
  const cashback = rateField(fields.cashback);
  const savings = rateField(fields.savings);
  const days = parseDays(fields.days);
  const errors: Partial<Record<CalcField, string>> = {};
  const parts: [CalcField, Field<number>][] = [
    ["amount", amount],
    ["cashback", cashback],
    ["savings", savings],
    ["days", days],
  ];
  for (const [name, part] of parts) {
    if (part.kind === "error") errors[name] = part.message;
  }
  if (
    amount.kind === "ok" &&
    cashback.kind === "ok" &&
    savings.kind === "ok" &&
    days.kind === "ok"
  ) {
    return {
      request: {
        amount: amount.value,
        cashbackRate: cashback.value,
        savingsRate: savings.value,
        days: days.value,
      },
      errors,
    };
  }
  return { request: null, errors };
}
