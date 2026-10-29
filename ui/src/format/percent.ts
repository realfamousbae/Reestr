// Rates are integer basis points: 1% = 100 bp.

import { type Parsed, fail, ok } from "./parsed";

/** 600 → "6%", 150 → "1,5%", 3003 → "30,03%". */
export function formatPercent(bp: number): string {
  const sign = bp < 0 ? "−" : "";
  const abs = Math.abs(bp);
  const whole = Math.trunc(abs / 100).toString();
  const fraction = (abs % 100).toString().padStart(2, "0").replace(/0+$/, "");
  return fraction === "" ? `${sign}${whole}%` : `${sign}${whole},${fraction}%`;
}

/** The text placed into the cell editor. */
export function percentEditText(bp: number): string {
  return formatPercent(bp).replace("%", "");
}

/** "6", "6%", "1,5", "0.25 %" → bp. Empty input clears the rate. */
export function parsePercent(input: string): Parsed<number | null> {
  const text = input.trim().replace(/\s/g, "").replace(/%$/, "");
  if (text === "") {
    return ok(null);
  }
  const match = /^(\d{1,3})(?:[.,](\d{1,2}))?$/.exec(text);
  if (match === null) {
    return fail(`Не удалось распознать процент «${input.trim()}»`);
  }
  const bp = Number(match[1]) * 100 + Number((match[2] ?? "").padEnd(2, "0"));
  if (bp > 10_000) {
    return fail("Процент кэшбэка не может быть больше 100");
  }
  return ok(bp);
}
