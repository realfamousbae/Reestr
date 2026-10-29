import { describe, expect, it } from "vitest";

import type { InflowView } from "../api";
import { inflowColumns, inflowOps, monthTitle, parsePlan } from "./income";

const NBSP = " ";

const inflows: InflowView[] = [
  {
    id: 7,
    date: "2026-07-05",
    source: "Зарплата",
    participant: "Андрей",
    card: "Д ВТБ",
    amount: 5_000_000,
    notes: null,
  },
];

const columns = inflowColumns("2026-07", ["Андрей", "Таня"], ["Д ВТБ", "Газпром КР"]);
const col = (key: string) => columns.findIndex((c) => c.key === key);

describe("income plan", () => {
  it("reads rubles, clears on empty, refuses a negative plan", () => {
    expect(parsePlan("60 000")).toEqual({ ok: true, value: 6_000_000 });
    expect(parsePlan(`45${NBSP}500,50`)).toEqual({ ok: true, value: 4_550_050 });
    expect(parsePlan("  ")).toEqual({ ok: true, value: null });
    expect(parsePlan("0")).toEqual({ ok: true, value: 0 });
    expect(parsePlan("-100").ok).toBe(false);
    expect(parsePlan("сорок").ok).toBe(false);
  });

  it("names months like sheets", () => {
    expect(monthTitle("2025-01")).toBe("01.25");
    expect(monthTitle("2026-12")).toBe("12.26");
  });
});

describe("inflows table", () => {
  it("shows an inflow in its columns, amount first", () => {
    expect(columns.map((c) => c.title)).toEqual([
      "Сумма",
      "Дата",
      "Источник",
      "Участник",
      "Карта",
      "Заметки",
    ]);
    const row = inflows[0];
    if (row === undefined) throw new Error("no row");
    expect(columns.map((c) => c.show(row))).toEqual([
      `50${NBSP}000`,
      "05.07.2026",
      "Зарплата",
      "Андрей",
      "Д ВТБ",
      "",
    ]);
  });

  it("edits a row and completes names", () => {
    const { ops, errors } = inflowOps(
      [
        { row: 0, col: col("participant"), text: "тан" },
        { row: 0, col: col("date"), text: "10" },
        { row: 0, col: col("amount"), text: "51 000" },
      ],
      inflows,
      columns,
    );
    expect(errors).toEqual([]);
    expect(ops).toEqual([
      { op: "edit", id: 7, edit: { field: "amount", value: 5_100_000 } },
      { op: "edit", id: 7, edit: { field: "date", value: "2026-07-10" } },
      { op: "edit", id: 7, edit: { field: "participant", value: "Таня" } },
    ]);
  });

  it("starts a new inflow with its amount", () => {
    const { ops, errors } = inflowOps(
      [
        { row: 1, col: col("amount"), text: "2000" },
        { row: 1, col: col("source"), text: " Аванс " },
        { row: 2, col: col("source"), text: "Подработка" },
      ],
      inflows,
      columns,
    );
    expect(ops).toEqual([
      {
        op: "append",
        edits: [
          { field: "amount", value: 200_000 },
          { field: "source", value: "Аванс" },
        ],
      },
    ]);
    expect(errors).toEqual(["Новое поступление начните с суммы"]);
  });

  it("refuses an amount that is not money that came in", () => {
    const { ops, errors } = inflowOps(
      [
        { row: 0, col: col("amount"), text: "0" },
        { row: 0, col: col("amount"), text: "-5" },
      ],
      inflows,
      columns,
    );
    expect(ops).toEqual([]);
    expect(errors).toHaveLength(2);
  });
});
