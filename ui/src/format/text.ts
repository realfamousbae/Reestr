import { type Parsed, fail, ok } from "./parsed";

/**
 * The key names are compared by, the same as `name_key` in the core: case-insensitive,
 * «ё» equal to «е», repeated spaces ignored.
 */
export function nameKey(name: string): string {
  return name.trim().split(/\s+/).join(" ").toLocaleLowerCase("ru").replaceAll("ё", "е");
}

export interface Matched {
  value: string;
  /** False when the text is not in the dictionary; it is kept as typed. */
  known: boolean;
}

/**
 * Matches typed text against a dictionary: an exact name wins, otherwise a unique prefix
 * completes ("прод" → "продукты", "а" → "Андрей"). Anything else is kept as typed.
 */
export function matchOption(input: string, options: readonly string[]): Matched | null {
  const text = input.trim().split(/\s+/).join(" ");
  if (text === "") {
    return null;
  }
  const key = nameKey(text);
  const exact = options.find((o) => nameKey(o) === key);
  if (exact !== undefined) {
    return { value: exact, known: true };
  }
  const byPrefix = options.filter((o) => nameKey(o).startsWith(key));
  if (byPrefix.length === 1 && byPrefix[0] !== undefined) {
    return { value: byPrefix[0], known: true };
  }
  return { value: text, known: false };
}

/** An MCC: exactly four digits, kept as text for its leading zeros. Empty clears the cell. */
export function parseMcc(text: string): Parsed<string | null> {
  const code = text.trim();
  if (code === "") return ok(null);
  return /^[0-9]{4}$/.test(code) ? ok(code) : fail(`MCC — это 4 цифры, а не «${code}»`);
}

export function isKnown(value: string | null, options: readonly string[]): boolean {
  if (value === null) {
    return true;
  }
  const key = nameKey(value);
  return options.some((o) => nameKey(o) === key);
}

/**
 * Excel-style AutoComplete: the first title in `history` (most recent first) that starts with
 * what was typed. Returns the full title, or null when nothing longer matches.
 */
export function complete(typed: string, history: readonly string[]): string | null {
  if (typed.trim() === "") {
    return null;
  }
  const key = nameKey(typed);
  const found = history.find((title) => {
    const titleKey = nameKey(title);
    return titleKey.length > key.length && titleKey.startsWith(key);
  });
  return found ?? null;
}

const YES = new Set(["да", "д", "yes", "y", "1", "+", "true", "x", "х", "✓", "v"]);
const NO = new Set(["нет", "н", "no", "n", "0", "-", "false", ""]);

/** A yes/no column: "да" / "нет", plus common shorthands. */
export function parseYesNo(input: string, column = "Регулярная"): Parsed<boolean> {
  const text = input.trim().toLocaleLowerCase("ru");
  if (YES.has(text)) {
    return ok(true);
  }
  if (NO.has(text)) {
    return ok(false);
  }
  return fail(`В колонке «${column}» пишут «да» или «нет», а не «${input.trim()}»`);
}

/** Russian plural: plural(1, "строка", "строки", "строк"). */
export function plural(n: number, one: string, few: string, many: string): string {
  const tens = Math.abs(n) % 100;
  const ones = tens % 10;
  if (tens >= 11 && tens <= 14) return many;
  if (ones === 1) return one;
  if (ones >= 2 && ones <= 4) return few;
  return many;
}
