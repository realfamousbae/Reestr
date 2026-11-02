import { describe, expect, it } from "vitest";

import type { CreditCardView } from "../api";
import {
  bindingOps,
  purchaseColumns,
  purchaseRows,
  reminderColumns,
  reminderOps,
  repaymentColumns,
  repaymentOps,
  trancheColumns,
  trancheOps,
} from "./tables";

const card: CreditCardView = {
  productId: 3,
  card: "Газпром КР",
  bank: null,
  participant: null,
  creditLimit: null,
  graceDays: 55,
  manualAvailable: null,
  manualAvailableOn: null,
  available: null,
  debt: 230_000,
  unbound: 1,
  repaymentTranche: 10,
  tranches: [
    {
      id: 10,
      start: "2026-06-01",
      end: "2026-08-28",
      adjustment: 0,
      notes: null,
      debt: 230_000,
      repaid: 0,
      remaining: 230_000,
      purchases: 2,
      daysLeft: 3,
      status: "active",
    },
    {
      id: 11,
      start: "2026-07-01",
      end: "2026-09-28",
      adjustment: 0,
      notes: null,
      debt: 0,
      repaid: 0,
      remaining: 0,
      purchases: 0,
      daysLeft: 34,
      status: "repaid",
    },
  ],
  repayments: [{ id: 5, trancheId: 10, date: "2026-07-10", amount: 100_000, notes: null }],
  purchases: [
    {
      entryId: 40,
      month: "2026-06",
      date: "2026-06-30",
      title: "Лента",
      amount: 150_000,
      trancheId: 10,
      manual: false,
    },
    {
      entryId: 41,
      month: "2026-07",
      date: null,
      title: "Такси",
      amount: 30_000,
      trancheId: null,
      manual: false,
    },
  ],
};

const col = (columns: { key: string }[], key: string) => columns.findIndex((c) => c.key === key);

describe("product tables", () => {
  it("edits and adds tranches with dates typed as in the journal", () => {
    const columns = trancheColumns("2026-08");
    const { ops, errors } = trancheOps(
      [
        { col: col(columns, "end"), row: 0, text: "30.8" },
        { col: col(columns, "notes"), row: 0, text: " выписка 1-го " },
        { col: col(columns, "start"), row: 2, text: "1.9" },
        { col: col(columns, "end"), row: 2, text: "" },
        { col: col(columns, "debt"), row: 0, text: "5" },
      ],
      card,
      columns,
    );
    expect(ops).toEqual([
      { op: "editTranche", id: 10, edit: { field: "end", value: "2026-08-30" } },
      { op: "editTranche", id: 10, edit: { field: "notes", value: "выписка 1-го" } },
      { op: "addTranche", product: 3, edits: [{ field: "start", value: "2026-09-01" }] },
    ]);
    expect(errors).toEqual(["Укажите дату"]);
    expect(columns[col(columns, "status")]?.show(card.tranches[0] ?? ({} as never))).toBe(
      "через 3 дня",
    );
    expect(columns[col(columns, "status")]?.status?.(card.tranches[0] ?? ({} as never))).toBe(
      "warn",
    );
  });

  it("adds repayments to a tranche typed by its start date", () => {
    const columns = repaymentColumns(card, "2026-08");
    expect(columns[col(columns, "tranche")]?.show(card.repayments[0] ?? ({} as never))).toBe(
      "01.06.26–28.08.26",
    );
    const { ops, errors } = repaymentOps(
      [
        { col: col(columns, "amount"), row: 1, text: "1 300" },
        { col: col(columns, "tranche"), row: 1, text: "01.07" },
        { col: col(columns, "date"), row: 2, text: "5" },
        { col: col(columns, "amount"), row: 0, text: "0" },
        { col: col(columns, "tranche"), row: 0, text: "15.05" },
      ],
      card,
      columns,
    );
    expect(ops).toEqual([
      {
        op: "addRepayment",
        product: 3,
        edits: [
          { field: "amount", value: 130_000 },
          { field: "tranche", value: 11 },
        ],
      },
    ]);
    expect(errors).toEqual([
      "Сумма погашения должна быть больше нуля",
      "Грейс «15.05» не найден: начните с даты его начала",
      "Новое погашение начните с суммы",
    ]);
  });

  it("binds purchases by hand and back by date", () => {
    const columns = purchaseColumns(card);
    expect(purchaseRows(card).map((p) => columns[col(columns, "binding")]?.show(p))).toEqual([
      "по дате",
      "нет грейса",
    ]);
    const { ops } = bindingOps(
      [
        { col: col(columns, "tranche"), row: 0, text: "01.07.26" },
        { col: col(columns, "tranche"), row: 1, text: "" },
        { col: col(columns, "title"), row: 1, text: "не меняется" },
      ],
      card,
      columns,
    );
    expect(ops).toEqual([
      { op: "bind", entry: 40, tranche: 11 },
      { op: "bind", entry: 41, tranche: null },
    ]);
  });

  it("writes reminders", () => {
    const columns = reminderColumns(["Газпром КР"], "2026-10");
    const { ops, errors } = reminderOps(
      [
        { col: col(columns, "title"), row: 0, text: "Перевыпустить карту" },
        { col: col(columns, "date"), row: 0, text: "15.12" },
        { col: col(columns, "card"), row: 0, text: "газ" },
        { col: col(columns, "done"), row: 1, text: "да" },
      ],
      [],
      columns,
    );
    expect(ops).toEqual([
      {
        op: "addDeadline",
        edits: [
          { field: "title", value: "Перевыпустить карту" },
          { field: "date", value: "2026-12-15" },
          { field: "card", value: "Газпром КР" },
        ],
      },
    ]);
    expect(errors).toEqual(["Новое напоминание начните с того, что сделать"]);
  });
});
