import { describe, expect, it } from "vitest";

import { MAX_AMOUNT, MAX_DAYS, parseCalcFields, parseDays } from "./input";

const fields = { amount: "100 000", cashback: "5", savings: "16,5", days: "30" };

describe("calculator input", () => {
  it("turns the fields into kopecks, basis points and days", () => {
    expect(parseCalcFields(fields)).toEqual({
      request: { amount: 10_000_000, cashbackRate: 500, savingsRate: 1650, days: 30 },
      errors: {},
    });
  });

  it("accepts zero days, zero rates and a percent sign", () => {
    const r = parseCalcFields({ amount: "1 234,56", cashback: "0%", savings: "0", days: "0" });
    expect(r.request).toEqual({ amount: 123_456, cashbackRate: 0, savingsRate: 0, days: 0 });
  });

  it("waits for empty fields without an error", () => {
    const r = parseCalcFields({ ...fields, days: "  " });
    expect(r.request).toBeNull();
    expect(r.errors).toEqual({});
  });

  it("explains each invalid field", () => {
    const r = parseCalcFields({ amount: "abc", cashback: "101", savings: "150", days: "1,5" });
    expect(r.request).toBeNull();
    expect(Object.keys(r.errors).sort()).toEqual(["amount", "cashback", "days", "savings"]);
    expect(r.errors.savings).toBe("Процент не может быть больше 100");
  });

  it("refuses negative and huge amounts", () => {
    expect(parseCalcFields({ ...fields, amount: "-5" }).errors.amount).toMatch(/отрицательной/);
    const huge = (MAX_AMOUNT / 100 + 1).toString();
    expect(parseCalcFields({ ...fields, amount: huge }).errors.amount).toBe(
      "Слишком большая сумма",
    );
    expect(
      parseCalcFields({ ...fields, amount: (MAX_AMOUNT / 100).toString() }).request,
    ).not.toBeNull();
  });

  it("limits the term to ten years", () => {
    expect(parseDays(MAX_DAYS.toString())).toEqual({ kind: "ok", value: MAX_DAYS });
    expect(parseDays((MAX_DAYS + 1).toString()).kind).toBe("error");
    expect(parseDays("-3").kind).toBe("error");
    expect(parseDays("").kind).toBe("empty");
  });
});
