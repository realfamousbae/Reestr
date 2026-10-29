import { describe, expect, it } from "vitest";

import type { Dictionaries } from "../api";
import { buildDictOps } from "./edits";
import { specs } from "./specs";

const empty: Dictionaries = {
  participants: [{ id: 1, name: "Марина", sortOrder: 1, isActive: true }],
  banks: [],
  products: [],
  groups: [],
  categories: [],
  templates: [],
};

const participants = specs(empty).find((s) => s.kind === "participant");
if (participants === undefined) throw new Error("spec");
const rows = participants.rows(empty);

describe("buildDictOps", () => {
  it("edits existing rows and appends named new ones", () => {
    const { ops, errors } = buildDictOps(
      [
        { col: 0, row: 0, text: "Мария" },
        { col: 2, row: 1, text: "нет" },
        { col: 0, row: 1, text: "Андрей" },
      ],
      rows,
      participants.columns,
    );
    expect(errors).toEqual([]);
    expect(ops).toEqual([
      { op: "edit", id: 1, edit: { field: "name", value: "Мария" } },
      {
        op: "append",
        edits: [
          { field: "name", value: "Андрей" },
          { field: "isActive", value: false },
        ],
      },
    ]);
  });

  it("refuses a new row without a name and reports bad cells", () => {
    const { ops, errors } = buildDictOps(
      [
        { col: 1, row: 1, text: "3" },
        { col: 2, row: 0, text: "может" },
      ],
      rows,
      participants.columns,
    );
    expect(ops).toEqual([]);
    expect(errors).toEqual([
      "В колонке «Активен» пишут «да» или «нет», а не «может»",
      "Новую строку начните с названия",
    ]);
  });
});
