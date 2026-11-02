import { describe, expect, it } from "vitest";

import type { RuleView } from "../api";
import { buildRuleOps, parseRuleCategory, ruleColumns } from "./specs";

const NBSP = "\u00a0";

const rule: RuleView = {
  id: 7,
  card: "Райф А",
  category: null,
  mcc: null,
  rateBp: 150,
  spendCap: 3_000_000,
  cashbackCap: null,
  adjustment: -120_050,
  notes: null,
  spent: 1_000_000,
  cashback: 15_000,
  entries: 4,
  remainingSpend: 2_000_000,
  remainingCashback: null,
  exhausted: false,
};

const columns = ruleColumns(["Райф А", "Газпром КР"], ["продукты", "еда"]);
const col = (key: string) => columns.findIndex((c) => c.key === key);

describe("rules table", () => {
  it("shows a rule as the user would write it", () => {
    const shown = Object.fromEntries(columns.map((c) => [c.key, c.show(rule)]));
    expect(shown).toMatchObject({
      card: "Райф А",
      category: "на всё",
      rate: "1,5%",
      spendCap: `30${NBSP}000`,
      cashbackCap: "",
      adjustment: `−1${NBSP}200,50`,
      spent: `10${NBSP}000`,
      remainingSpend: `20${NBSP}000`,
      remainingCashback: "",
    });
    // The editor gets an empty category, not the words «на всё».
    expect(columns[col("category")]?.edit?.(rule)).toBe("");
    expect(columns[col("remainingSpend")]?.status?.({ ...rule, remainingSpend: 0 })).toBe("danger");
    // Computed columns are read-only.
    expect(columns[col("spent")]?.parse).toBeUndefined();
  });

  it("does not call a rule by MCC «на всё»", () => {
    const byCode = { ...rule, mcc: "0742" };
    expect(columns[col("category")]?.show(byCode)).toBe("");
    expect(columns[col("mcc")]?.show(byCode)).toBe("0742");
    expect(columns[col("mcc")]?.show(rule)).toBe("");
  });

  it("reads an MCC and refuses anything else", () => {
    const { ops, errors } = buildRuleOps(
      [
        { col: col("mcc"), row: 0, text: "0742" },
        { col: col("mcc"), row: 0, text: "" },
        { col: col("card"), row: 1, text: "газ" },
        { col: col("mcc"), row: 1, text: "5411" },
        { col: col("mcc"), row: 0, text: "74" },
      ],
      [rule],
      columns,
    );
    expect(ops).toEqual([
      { op: "edit", id: 7, edit: { field: "mcc", value: "0742" } },
      { op: "edit", id: 7, edit: { field: "mcc", value: null } },
      {
        op: "append",
        edits: [
          { field: "card", value: "Газпром КР" },
          { field: "mcc", value: "5411" },
        ],
      },
    ]);
    expect(errors).toEqual(["MCC — это 4 цифры, а не «74»"]);
  });

  it("reads «на всё» and completes names", () => {
    expect(parseRuleCategory("на всё", ["еда"])).toBeNull();
    expect(parseRuleCategory("На все", ["еда"])).toBeNull();
    expect(parseRuleCategory("  ", ["еда"])).toBeNull();
    expect(parseRuleCategory("прод", ["продукты", "еда"])).toBe("продукты");
    expect(parseRuleCategory("кино", ["еда"])).toBe("кино");
  });

  it("builds edits and appends, and refuses a rule without a card", () => {
    const { ops, errors } = buildRuleOps(
      [
        { col: col("rate"), row: 0, text: "5" },
        { col: col("spendCap"), row: 0, text: "" },
        { col: col("card"), row: 1, text: "газ" },
        { col: col("category"), row: 1, text: "еда" },
        { col: col("cashbackCap"), row: 1, text: "300" },
        { col: col("rate"), row: 2, text: "3" },
        { col: col("cashbackCap"), row: 0, text: "-5" },
        { col: col("spent"), row: 0, text: "1" },
      ],
      [rule],
      columns,
    );
    expect(ops).toEqual([
      { op: "edit", id: 7, edit: { field: "rate", value: 500 } },
      { op: "edit", id: 7, edit: { field: "spendCap", value: null } },
      {
        op: "append",
        edits: [
          { field: "card", value: "Газпром КР" },
          { field: "category", value: "еда" },
          { field: "cashbackCap", value: 30_000 },
        ],
      },
    ]);
    expect(errors).toEqual([
      "«Лимит кэшбэка» не может быть отрицательным",
      "Новое правило начните с карты",
    ]);
  });
});
