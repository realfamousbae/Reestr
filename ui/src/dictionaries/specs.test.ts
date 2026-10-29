import { describe, expect, it } from "vitest";

import type { Dictionaries } from "../api";
import { specs } from "./specs";

const dictionaries: Dictionaries = {
  participants: [{ id: 1, name: "Марина", sortOrder: 1, isActive: true }],
  banks: [{ id: 2, name: "Газпромбанк", shortName: "ГПБ" }],
  products: [
    {
      id: 3,
      bankId: 2,
      participantId: 1,
      kind: "credit",
      shortName: "Газпром КР",
      fullName: null,
      cashbackRounding: "floor_rub",
      isActive: true,
      openedAt: "2025-03-01",
      closedAt: null,
      notes: null,
    },
  ],
  groups: [{ id: 4, name: "Лечение/лекарства", sortOrder: 1 }],
  categories: [
    {
      id: 5,
      name: "лечение",
      groupId: 4,
      isRegularDefault: false,
      sortOrder: 1,
      isActive: true,
    },
    {
      id: 6,
      name: "лекарства",
      groupId: 4,
      isRegularDefault: false,
      sortOrder: 2,
      isActive: true,
    },
    {
      id: 7,
      name: "квартиры",
      groupId: null,
      isRegularDefault: true,
      sortOrder: 3,
      isActive: true,
    },
  ],
  templates: [
    {
      id: 8,
      title: "Квартплата",
      categoryId: 7,
      participantId: null,
      productId: null,
      defaultAmount: null,
      sortOrder: 1,
      isActive: true,
      excludeFromRate: true,
    },
    {
      id: 9,
      title: "Интернет",
      categoryId: null,
      participantId: null,
      productId: null,
      defaultAmount: 79_000,
      sortOrder: 2,
      isActive: true,
      excludeFromRate: false,
    },
  ],
};

function spec(kind: string) {
  const found = specs(dictionaries).find((s) => s.kind === kind);
  if (found === undefined) throw new Error(kind);
  return found;
}

function column(kind: string, key: string) {
  const found = spec(kind).columns.find((c) => c.key === key);
  if (found === undefined) throw new Error(key);
  return found;
}

describe("dictionary specs", () => {
  it("shows references by name", () => {
    const products = spec("product");
    const row = products.rows(dictionaries)[0];
    if (row === undefined) throw new Error("row");
    expect(column("product", "bank").show(row)).toBe("Газпромбанк");
    expect(column("product", "participant").show(row)).toBe("Марина");
    expect(column("product", "kind").show(row)).toBe("кредитная");
    expect(column("product", "rounding").show(row)).toBe("вниз до рубля");
    expect(column("product", "openedAt").show(row)).toBe("01.03.2025");
    expect(column("group", "members").show({ id: 4 })).toBe("лечение, лекарства");
  });

  it("marks regular payments left out of «Ср. %»", () => {
    expect(spec("category").columns.map((c) => c.key)).not.toContain("excludeFromRate");
    const flag = column("template", "excludeFromRate");
    expect(flag.title).toBe("Не в Ср. %");
    expect(dictionaries.templates.map((t) => flag.show(t))).toEqual(["да", ""]);
    expect(flag.parse?.("да")).toEqual({
      ok: true,
      value: { field: "excludeFromRate", value: true },
    });
  });

  it("reads labels by a unique beginning", () => {
    expect(column("product", "kind").parse?.("кр")).toEqual({
      ok: true,
      value: { field: "kind", value: "credit" },
    });
    expect(column("product", "kind").parse?.("")).toEqual({
      ok: true,
      value: { field: "kind", value: null },
    });
    expect(column("product", "rounding").parse?.("до коп")).toEqual({
      ok: true,
      value: { field: "rounding", value: "half_up_kop" },
    });
    expect(column("product", "rounding").parse?.("вниз до к")).toEqual({
      ok: true,
      value: { field: "rounding", value: "floor_kop" },
    });
    expect(column("product", "kind").parse?.("ипотека").ok).toBe(false);
  });

  it("completes references and keeps new names for creation", () => {
    expect(column("product", "bank").parse?.("газ")).toEqual({
      ok: true,
      value: { field: "bank", value: "Газпромбанк" },
    });
    expect(column("product", "bank").parse?.("Райффайзенбанк")).toEqual({
      ok: true,
      value: { field: "bank", value: "Райффайзенбанк" },
    });
    expect(column("category", "group").parse?.("")).toEqual({
      ok: true,
      value: { field: "group", value: null },
    });
  });

  it("validates names, numbers and amounts", () => {
    expect(column("category", "name").parse?.("  ").ok).toBe(false);
    expect(column("category", "sortOrder").parse?.("2,5").ok).toBe(false);
    expect(column("category", "sortOrder").parse?.("7")).toEqual({
      ok: true,
      value: { field: "sortOrder", value: 7 },
    });
    expect(column("template", "defaultAmount").parse?.("650")).toEqual({
      ok: true,
      value: { field: "defaultAmount", value: 65000 },
    });
    expect(column("group", "members").parse).toBeUndefined();
  });
});
