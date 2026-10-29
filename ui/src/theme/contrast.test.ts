import { describe, expect, it } from "vitest";

import { colorTokens, contrastRatio } from "./contrast";
import tokensCss from "./tokens.css?raw";

const tokens = colorTokens(tokensCss);
const color = (name: string) => {
  const value = tokens.get(name);
  if (value === undefined) throw new Error(`no token --${name}`);
  return value;
};

const TEXT = ["ink", "ink-muted", "ink-faint", "accent", "accent-strong", "warn", "danger"];
const SURFACES = ["sheet", "paper", "band", "band-strong", "accent-wash", "warn-bg", "danger-bg"];

describe("contrast of the palette (PLAN §8)", () => {
  it("computes WCAG ratios", () => {
    expect(contrastRatio("#000000", "#ffffff")).toBeCloseTo(21, 5);
    expect(contrastRatio("#ffffff", "#ffffff")).toBeCloseTo(1, 5);
  });

  it("keeps every text color at 4.5:1 or more on every surface", () => {
    for (const text of TEXT) {
      for (const surface of SURFACES) {
        expect(
          contrastRatio(color(text), color(surface)),
          `--${text} on --${surface}`,
        ).toBeGreaterThanOrEqual(4.5);
      }
    }
    // White text on the primary button.
    expect(contrastRatio(color("sheet"), color("accent"))).toBeGreaterThanOrEqual(4.5);
  });

  it("keeps field borders at 3:1 on the sheet and the paper", () => {
    expect(contrastRatio(color("ruling-strong"), color("sheet"))).toBeGreaterThanOrEqual(3);
    expect(contrastRatio(color("ruling-strong"), color("paper"))).toBeGreaterThanOrEqual(3);
  });
});
