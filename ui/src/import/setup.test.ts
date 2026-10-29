import { describe, expect, it } from "vitest";

import type { ImportPreview } from "../api";
import { initialSetups, mismatches, rowsToAdd } from "./setup";

const preview: ImportPreview = {
  months: [
    {
      month: "2026-07",
      sheet: "07.26",
      rows: 53,
      already: 3,
      check: {
        month: "2026-07",
        sheet: "07.26",
        rows: 53,
        matches: false,
        compared: [
          { label: "Итого", expected: 100000, actual: 100000 },
          { label: "Еда", expected: 120000, actual: 115000 },
        ],
      },
    },
    {
      month: "2026-06",
      sheet: "06.26",
      rows: 0,
      already: 0,
      check: { month: "2026-06", sheet: "06.26", rows: 0, matches: true, compared: [] },
    },
  ],
  categories: [],
  groups: [],
  participants: [],
  cards: [
    {
      shortName: "Газпром КР",
      rows: 10,
      exists: false,
      guess: { bank: "Газпромбанк", kind: "credit", participant: null },
    },
    {
      shortName: "Райф М",
      rows: 4,
      exists: true,
      guess: { bank: "Райффайзенбанк", kind: null, participant: "Марина" },
    },
  ],
  warnings: [],
};

describe("import wizard helpers", () => {
  it("starts new cards from the guesses and leaves known cards alone", () => {
    expect(initialSetups(preview)).toEqual([
      { shortName: "Газпром КР", bank: "Газпромбанк", kind: "credit", participant: null },
      { shortName: "Райф М", bank: null, kind: null, participant: null },
    ]);
  });

  it("counts rows that are not in the file yet", () => {
    expect(rowsToAdd(preview)).toBe(50);
  });

  it("describes differing totals", () => {
    const check = preview.months[0]?.check;
    if (check === undefined) throw new Error("check");
    expect(mismatches(check)).toEqual(["Еда: в листе 1 200, у нас 1 150"]);
  });
});
