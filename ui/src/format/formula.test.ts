import { describe, expect, it } from "vitest";

import { evaluateFormula } from "./formula";
import { parseMoney } from "./money";

describe("formulas in money cells", () => {
  it.each([
    ["=100+250", 35_000],
    ["=1874,5*2", 374_900],
    ["= 1 200 - 200", 100_000],
    ["=(1200-200)/3", 33_333],
    ["=2/3", 67],
    ["=-50+20", -3_000],
    ["=0.1+0.2", 30],
    ["=10×3", 3_000],
    ["=100:4", 2_500],
    ["=-(1+2)*2", -600],
    ["=1/200", 1],
    ["=-1/200", -1],
  ])("%s → %d kopecks", (input, kopecks) => {
    expect(evaluateFormula(input)).toEqual({ ok: true, value: kopecks });
    expect(parseMoney(input)).toEqual({ ok: true, value: kopecks });
  });

  it.each([
    ["=", "Формула пустая"],
    ["=100/0", "деление на ноль"],
    ["=(1+2", "не хватает «)»"],
    ["=1+", "формула оборвалась"],
    ["=1+a", "непонятное «a»"],
    ["=2)", "лишнее «)»"],
    ["=99999999999999*1000", "Слишком большая сумма"],
  ])("refuses %s", (input, message) => {
    const parsed = evaluateFormula(input);
    expect(parsed.ok).toBe(false);
    if (!parsed.ok) expect(parsed.message).toContain(message);
  });
});
