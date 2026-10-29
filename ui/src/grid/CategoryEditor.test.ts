import { describe, expect, it } from "vitest";

import { categoryOptions } from "./CategoryEditor";

const CATEGORIES = ["еда", "продукты", "лечение", "лекарства", "квартиры", "Транспорт"];

describe("category list", () => {
  it("shows every category for an empty cell", () => {
    expect(categoryOptions("", CATEGORIES)).toEqual(CATEGORIES);
  });

  it("puts names that begin with the text first, then those that contain it", () => {
    expect(categoryOptions("ле", CATEGORIES)).toEqual(["лечение", "лекарства"]);
    expect(categoryOptions("ар", CATEGORIES)).toEqual(["лекарства", "квартиры"]);
    expect(categoryOptions("тр", CATEGORIES)).toEqual(["Транспорт"]);
    expect(categoryOptions("кино", CATEGORIES)).toEqual([]);
  });
});
