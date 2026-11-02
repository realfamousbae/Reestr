import { describe, expect, it } from "vitest";

import type { RuleView, SuggestionView } from "../api";
import { remainingText, ruleTitle, suggestionText } from "./limits";

const NBSP = "\u00a0";

const base: RuleView = {
  id: 1,
  card: "Газпром КР",
  category: "еда",
  mcc: null,
  rateBp: 1000,
  spendCap: null,
  cashbackCap: 30_000,
  adjustment: 0,
  notes: null,
  spent: 335_000,
  cashback: 18_500,
  entries: 1,
  remainingSpend: null,
  remainingCashback: 11_500,
  exhausted: false,
};

const suggestion: SuggestionView = {
  card: "Газпром КР",
  rateBp: 500,
  expected: null,
  exhausted: false,
  remainingSpend: null,
  remainingCashback: null,
};

describe("limits in words", () => {
  it("names the rule", () => {
    expect(ruleTitle(base)).toBe("Газпром КР · еда 10%");
    expect(ruleTitle({ ...base, category: null, rateBp: 150 })).toBe("Газпром КР · на всё 1,5%");
    expect(ruleTitle({ ...base, category: null, mcc: "0742" })).toBe("Газпром КР · MCC 0742 10%");
  });

  it("says what is left", () => {
    expect(remainingText(base)).toBe("осталось кэшбэка 115 ₽");
    expect(remainingText({ ...base, remainingSpend: 247_950, remainingCashback: null })).toBe(
      `осталось трат 2${NBSP}479,50 ₽`,
    );
    expect(remainingText({ ...base, remainingSpend: 247_950 })).toBe(
      `осталось: трат 2${NBSP}479,50 ₽, кэшбэка 115 ₽`,
    );
    expect(remainingText({ ...base, remainingCashback: null })).toBe("без лимита");
    expect(remainingText({ ...base, remainingCashback: 0, exhausted: true })).toBe(
      "лимит исчерпан",
    );
  });

  it("describes a suggestion", () => {
    expect(suggestionText({ ...suggestion, expected: 5_000 })).toBe("кэшбэк 50 ₽");
    expect(suggestionText({ ...suggestion, remainingCashback: 1_000 })).toBe(
      "осталось кэшбэка 10 ₽",
    );
    expect(suggestionText(suggestion)).toBe("");
    expect(suggestionText({ ...suggestion, exhausted: true, expected: 0 })).toBe("лимит исчерпан");
  });
});
