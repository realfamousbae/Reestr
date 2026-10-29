import { describe, expect, it } from "vitest";

import type { EntryView } from "../api";
import {
  appendEdits,
  changedEdits,
  emptyForm,
  fillFromLast,
  formOf,
  monthOf,
  readDraft,
  titleMatches,
} from "./draft";

const row: EntryView = {
  id: 7,
  title: "Пятёрочка",
  category: "продукты",
  isRegular: false,
  participant: "Андрей",
  date: "2026-10-28",
  amount: 120_000,
  rateBp: null,
  cashback: null,
  cashbackSource: null,
  card: "Газпром КР",
};

describe("the phone's entry form", () => {
  it("reads a typed row", () => {
    const form = { ...emptyForm("2026-10-28"), title: " Лента ", amount: "1 874,5" };
    const draft = readDraft(form);
    expect(draft).toEqual({
      ok: true,
      value: {
        title: "Лента",
        amount: 187_450,
        category: null,
        card: null,
        participant: null,
        date: "2026-10-28",
        regular: false,
      },
    });
    if (!draft.ok) throw new Error("draft");
    expect(appendEdits(draft.value)).toContainEqual({ field: "amount", value: 187_450 });
    expect(monthOf(draft.value.date, "2026-11")).toBe("2026-10");
    expect(monthOf(null, "2026-11")).toBe("2026-11");
  });

  it("asks for a title and an amount", () => {
    expect(readDraft(emptyForm("2026-10-28"))).toEqual({
      ok: false,
      message: "Напишите наименование.",
    });
    expect(readDraft({ ...emptyForm("2026-10-28"), title: "Лента" })).toEqual({
      ok: false,
      message: "Введите сумму.",
    });
    expect(readDraft({ ...emptyForm(""), title: "Лента", amount: "=1200+300/2" })).toMatchObject({
      ok: true,
      value: { amount: 135_000 },
    });
  });

  it("sends only what changed in a row", () => {
    const form = formOf(row, "2026-10-30");
    expect(form.amount).toBe("1200");
    const draft = readDraft({ ...form, amount: "1300", card: "" });
    if (!draft.ok) throw new Error("draft");
    expect(changedEdits(draft.value, row)).toEqual([
      { field: "amount", value: 130_000 },
      { field: "card", value: null },
    ]);
  });

  it("fills empty fields from the last row with that title", () => {
    const form = { ...emptyForm("2026-10-30"), title: "Пятёрочка", participant: "Марина" };
    expect(fillFromLast(form, row)).toMatchObject({
      category: "продукты",
      card: "Газпром КР",
      participant: "Марина",
    });
  });

  it("offers past titles", () => {
    const history = ["Пятёрочка", "Пятёрочка у дома", "Лента", "Аптека «Ригла»", "Рынок"];
    expect(titleMatches("пят", history)).toEqual(["Пятёрочка", "Пятёрочка у дома"]);
    expect(titleMatches("пятерочка", history)).toEqual(["Пятёрочка у дома"]);
    expect(titleMatches("ригл", history)).toEqual(["Аптека «Ригла»"]);
    expect(titleMatches(" ", history)).toEqual([]);
  });
});
