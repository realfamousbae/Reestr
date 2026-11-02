import { describe, expect, it } from "vitest";

import type { EntryView } from "../api";
import { COLUMNS, displayText, editorText } from "./columns";
import { buildBatch, pasteChanges } from "./edits";

const ctx = {
  month: "2026-07",
  categories: ["еда", "продукты"],
  participants: ["Андрей", "Марина"],
  cards: ["Газпром КР", "Газпром А", "Райф М"],
};

function entry(id: number): EntryView {
  return {
    id,
    title: "",
    category: null,
    isRegular: false,
    participant: null,
    date: null,
    amount: 0,
    rateBp: null,
    cashback: null,
    cashbackSource: null,
    card: null,
    mcc: null,
  };
}

describe("buildBatch", () => {
  it("edits existing rows and appends new ones in order", () => {
    const { ops, errors } = buildBatch(
      [
        { col: 0, row: 0, text: "Пятёрочка" },
        { col: 5, row: 0, text: "1 234,50" },
        { col: 0, row: 2, text: "Вторая новая" },
        { col: 0, row: 1, text: "Новая" },
        { col: 1, row: 1, text: "прод" },
      ],
      [entry(7)],
      ctx,
    );
    expect(errors).toEqual([]);
    expect(ops).toEqual([
      { op: "edit", id: 7, edit: { field: "title", value: "Пятёрочка" } },
      { op: "edit", id: 7, edit: { field: "amount", value: 123450 } },
      {
        op: "append",
        edits: [
          { field: "title", value: "Новая" },
          { field: "category", value: "продукты" },
        ],
      },
      { op: "append", edits: [{ field: "title", value: "Вторая новая" }] },
    ]);
  });

  it("puts the amount before the rate and lets % win over cashback", () => {
    const { ops } = buildBatch(
      [
        { col: 7, row: 0, text: "134" },
        { col: 6, row: 0, text: "6%" },
        { col: 5, row: 0, text: "2234" },
      ],
      [],
      ctx,
    );
    expect(ops).toEqual([
      {
        op: "append",
        edits: [
          { field: "amount", value: 223400 },
          { field: "rate", value: 600 },
        ],
      },
    ]);
  });

  it("reports unreadable cells and keeps the rest", () => {
    const { ops, errors } = buildBatch(
      [
        { col: 5, row: 0, text: "много" },
        { col: 4, row: 0, text: "5" },
      ],
      [entry(1)],
      ctx,
    );
    expect(errors).toEqual(["Не удалось распознать сумму «много»"]);
    expect(ops).toEqual([{ op: "edit", id: 1, edit: { field: "date", value: "2026-07-05" } }]);
  });

  it("clearing cells resets them", () => {
    const { ops } = buildBatch(
      [
        { col: 5, row: 0, text: "" },
        { col: 7, row: 0, text: "" },
        { col: 2, row: 0, text: "" },
      ],
      [entry(1)],
      ctx,
    );
    expect(ops.map((o) => (o.op === "edit" ? o.edit : null))).toEqual([
      { field: "isRegular", value: false },
      { field: "amount", value: 0 },
      { field: "cashback", value: null },
    ]);
  });
});

describe("the MCC column", () => {
  const MCC = 9;

  it("is the last one and keeps its leading zeros", () => {
    expect(COLUMNS.map((c) => c.title).slice(-2)).toEqual(["Карта", "MCC"]);
    expect(COLUMNS[MCC]?.key).toBe("mcc");
    const { ops, errors } = buildBatch(
      [
        { col: MCC, row: 0, text: " 0742 " },
        { col: MCC, row: 1, text: "" },
      ],
      [entry(1), entry(2)],
      ctx,
    );
    expect(errors).toEqual([]);
    expect(ops).toEqual([
      { op: "edit", id: 1, edit: { field: "mcc", value: "0742" } },
      { op: "edit", id: 2, edit: { field: "mcc", value: null } },
    ]);
  });

  it("shows the code and copies it as text", () => {
    const row = { ...entry(1), mcc: "0742" };
    expect(displayText(row, "mcc")).toBe("0742");
    expect(editorText(row, "mcc")).toBe("0742");
    expect(displayText(entry(1), "mcc")).toBe("");
  });

  it("reports a bad code like any unreadable cell and keeps the rest of the row", () => {
    const { ops, errors } = buildBatch(
      [
        { col: 0, row: 0, text: "Кофейня" },
        { col: MCC, row: 0, text: "58" },
      ],
      [],
      ctx,
    );
    expect(errors).toEqual(["MCC — это 4 цифры, а не «58»"]);
    expect(ops).toEqual([{ op: "append", edits: [{ field: "title", value: "Кофейня" }] }]);
  });

  it("does not move the columns of a block pasted from the sheet", () => {
    const line = ["Лента", "продукты", "да", "Андрей", "05.07.26", "1000", "5", "50", "Райф А"];
    const { ops, errors } = buildBatch(pasteChanges([0, 0], [[...line, "5411"]]), [], ctx);
    expect(errors).toEqual([]);
    expect(ops).toEqual([
      {
        op: "append",
        edits: [
          { field: "title", value: "Лента" },
          { field: "category", value: "продукты" },
          { field: "isRegular", value: true },
          { field: "participant", value: "Андрей" },
          { field: "date", value: "2026-07-05" },
          { field: "amount", value: 100_000 },
          { field: "rate", value: 500 },
          { field: "card", value: "Райф А" },
          { field: "mcc", value: "5411" },
        ],
      },
    ]);
    // Nine columns, as before MCC: the code stays as it was.
    const old = buildBatch(pasteChanges([0, 0], [line]), [entry(1)], ctx);
    expect(old.ops.map((o) => (o.op === "edit" ? o.edit.field : null))).not.toContain("mcc");
  });
});

describe("new rows", () => {
  it("adds nothing for blank lines pasted below the journal", () => {
    const { ops } = buildBatch(
      [
        { col: 0, row: 1, text: "" },
        { col: 5, row: 1, text: "  " },
        { col: 0, row: 2, text: "Лента" },
        { col: 5, row: 2, text: "=100+250" },
      ],
      [entry(7)],
      ctx,
    );
    expect(ops).toEqual([
      {
        op: "append",
        edits: [
          { field: "title", value: "Лента" },
          { field: "amount", value: 35_000 },
        ],
      },
    ]);
  });
});

describe("pasteChanges", () => {
  it("expands a block and drops columns past the last one", () => {
    const changes = pasteChanges(
      [8, 3],
      [
        ["a", "b", "c"],
        ["d", "e", "f"],
      ],
    );
    expect(changes).toEqual([
      { col: 8, row: 3, text: "a" },
      { col: 9, row: 3, text: "b" },
      { col: 8, row: 4, text: "d" },
      { col: 9, row: 4, text: "e" },
    ]);
  });
});
