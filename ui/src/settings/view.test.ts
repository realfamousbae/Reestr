import { describe, expect, it } from "vitest";

import { DEFAULT_VIEW, moveBlock, normalizeView, shownBlocks, toggleBlock } from "./view";

describe("view settings", () => {
  it("falls back to the source order and the default size", () => {
    expect(normalizeView(null)).toEqual(DEFAULT_VIEW);
    expect(normalizeView({ fontSize: 99 }).fontSize).toBe(15);
    expect(DEFAULT_VIEW.order[0]).toBe("cashback");
    expect(DEFAULT_VIEW.order.at(-1)).toBe("deadlines");
  });

  it("keeps a saved order, drops unknown blocks and appends new ones", () => {
    const view = normalizeView({
      fontSize: 18,
      order: ["income", "кино", "cashback", "income"],
      hidden: ["limits", "кино"],
    });
    expect(view.fontSize).toBe(18);
    expect(view.order.slice(0, 2)).toEqual(["income", "cashback"]);
    expect(view.order).toHaveLength(7);
    expect(view.hidden).toEqual(["limits"]);
  });

  it("moves and hides blocks", () => {
    const up = moveBlock(DEFAULT_VIEW, "income", -1);
    expect(up.order.indexOf("income")).toBe(DEFAULT_VIEW.order.indexOf("income") - 1);
    expect(moveBlock(DEFAULT_VIEW, "cashback", -1)).toBe(DEFAULT_VIEW);
    expect(moveBlock(DEFAULT_VIEW, "deadlines", 1)).toBe(DEFAULT_VIEW);
    const hidden = toggleBlock(DEFAULT_VIEW, "savings");
    expect(shownBlocks(hidden)).not.toContain("savings");
    expect(shownBlocks(toggleBlock(hidden, "savings"))).toEqual(DEFAULT_VIEW.order);
  });
});
