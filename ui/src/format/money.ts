// Money in the UI is integer kopecks, exactly as the core sends it. Parsing and formatting work
// on digit strings, never on floats (PLAN §15).

import { evaluateFormula } from "./formula";
import { type Parsed, fail, ok } from "./parsed";

const NBSP = "\u00a0";
const MINUS = "−";

function groupThousands(digits: string): string {
  return digits.replace(/\B(?=(\d{3})+(?!\d))/g, NBSP);
}

let alwaysKopecks = false;

/**
 * Sets the display mode for every `formatMoney` call: with `true` kopecks are always shown
 * («1 200,00»). It is a per-computer view setting (settings/view.ts), applied by the workbook
 * screen before it renders. The default is off.
 */
export function setAlwaysKopecks(on: boolean): void {
  alwaysKopecks = on;
}

function render(kopecks: number, withKopecks: boolean): string {
  const sign = kopecks < 0 ? MINUS : "";
  const abs = Math.abs(kopecks);
  const rubles = groupThousands(Math.trunc(abs / 100).toString());
  const rest = abs % 100;
  return rest === 0 && !withKopecks
    ? `${sign}${rubles}`
    : `${sign}${rubles},${rest.toString().padStart(2, "0")}`;
}

/**
 * 123456 → "1 234,56"; 120000 → "1 200"; −5000 → "−50". Kopecks are shown only when present,
 * unless the «always show kopecks» mode is on ("1 200,00").
 */
export function formatMoney(kopecks: number): string {
  return render(kopecks, alwaysKopecks);
}

/**
 * The text placed into the cell editor and the clipboard: no grouping, so it is easy to change,
 * and independent of the display mode (1200, never 1200,00).
 */
export function moneyEditText(kopecks: number): string {
  return render(kopecks, false).replaceAll(NBSP, "");
}

/**
 * Parses what a person types or pastes: "1234", "1 234,56", "1234.5", "−50", "1 200 ₽", or a
 * formula like in Excel: "=100+250".
 * Both "," and "." are decimal separators; spaces of any kind group thousands.
 */
export function parseMoney(input: string): Parsed<number> {
  if (input.trim().startsWith("=")) {
    return evaluateFormula(input);
  }
  let text = input
    .trim()
    .replace(/(₽|руб\.?|р\.?)$/iu, "")
    .replace(/\s/g, "");
  let negative = false;
  if (/^[-−]/.test(text)) {
    negative = true;
    text = text.slice(1);
  }
  const match = /^(\d+)(?:[.,](\d{1,2}))?$/.exec(text);
  if (match === null) {
    return fail(`Не удалось распознать сумму «${input.trim()}»`);
  }
  const rubles = match[1] ?? "0";
  const kopecks = (match[2] ?? "").padEnd(2, "0");
  if (rubles.length > 13) {
    return fail("Слишком большая сумма");
  }
  const value = Number(rubles) * 100 + Number(kopecks);
  return ok(negative ? -value : value);
}
