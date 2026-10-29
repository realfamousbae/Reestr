import { describe, expect, it } from "vitest";

import type { SummaryRow, YearSummary } from "../api";
import { monthName, summaryTable } from "./table";

const NBSP = " ";

function row(month: number, data?: Partial<SummaryRow>): SummaryRow {
  return {
    month: `2026-${month.toString().padStart(2, "0")}`,
    hasData: false,
    total: 0,
    plan: 4_000_000,
    balance: null,
    growthBp: null,
    columns: [0, 0],
    regular: 0,
    cashback: 0,
    overallBalance: null,
    cashbackRateBp: null,
    inflow: 0,
    ...data,
  };
}

function summary(withPrevious: boolean): YearSummary {
  const months = Array.from({ length: 12 }, (_, i) => row(i + 1));
  months[6] = row(7, {
    hasData: true,
    total: 4_500_000,
    balance: -500_000,
    growthBp: 1_250,
    columns: [3_000_000, 0],
    regular: 1_500_000,
    cashback: 90_000,
    overallBalance: -410_000,
    cashbackRateBp: 200,
  });
  const none = { columns: [null, null], regular: null, cashback: null };
  return {
    year: 2026,
    grouped: false,
    columns: [
      { id: 1, isGroup: false, label: "Еда" },
      { id: 5, isGroup: true, label: "Лечение/лекарства" },
    ],
    months,
    footer: {
      months: 1,
      total: 4_500_000,
      sums: { columns: [3_000_000, 0], regular: 1_500_000, cashback: 90_000 },
      sharesBp: { columns: [6_667, 0], regular: 3_333, cashback: 200 },
      averages: { columns: [3_000_000, 0], regular: 1_500_000, cashback: 90_000 },
      averageTotal: 4_500_000,
      averageTotalGrowthBp: withPrevious ? 1_250 : null,
      averageGrowth: withPrevious
        ? { columns: [100_000, -20_000], regular: 0, cashback: 5_000 }
        : none,
      averageGrowthBp: withPrevious ? { columns: [345, -10_000], regular: 0, cashback: 588 } : none,
      overallBalance: -410_000,
    },
  };
}

describe("«Свод» table", () => {
  it("keeps the source columns in their order", () => {
    expect(summaryTable(summary(true)).head.map((h) => h.title)).toEqual([
      "Сумма",
      "План дохода",
      "Баланс в мес.",
      "Прирост к прошлому году",
      "Еда",
      "Лечение/лекарства",
      "Регулярные",
      "Кешбек",
      "Общий баланс",
      "Общий % кешбека",
    ]);
  });

  it("shows a month with entries and marks it over plan", () => {
    const t = summaryTable(summary(true));
    const july = t.months[6];
    expect(july?.label).toBe("июль");
    expect(july?.empty).toBeUndefined();
    expect(july?.cells.map((c) => c.text)).toEqual([
      `45${NBSP}000`,
      `40${NBSP}000`,
      `−5${NBSP}000`,
      "12,5%",
      `30${NBSP}000`,
      "0",
      `15${NBSP}000`,
      "900",
      `−4${NBSP}100`,
      "2%",
    ]);
    expect(july?.cells[2]?.tone).toBe("danger");
    expect(july?.cells[5]?.tone).toBe("muted");
  });

  it("leaves months without entries blank except the plan", () => {
    const t = summaryTable(summary(true));
    const january = t.months[0];
    expect(january?.empty).toBe(true);
    expect(january?.cells).toHaveLength(t.head.length);
    expect(january?.cells.map((c) => c.text)).toEqual([
      "",
      `40${NBSP}000`,
      ...Array<string>(8).fill(""),
    ]);
  });

  it("puts the sums, shares and averages under the months", () => {
    const t = summaryTable(summary(true));
    expect(t.footer.map((l) => l.label)).toEqual([
      "Сумма",
      "От общ. %",
      "Среднее",
      "Ср. прирост, ₽",
      "Ср. прирост, %",
    ]);
    for (const line of t.footer) expect(line.cells).toHaveLength(t.head.length);
    const [sums, shares, averages, growth, growthBp] = t.footer;
    expect(sums?.cells[0]?.text).toBe(`45${NBSP}000`);
    expect(sums?.cells[8]?.text).toBe(`−4${NBSP}100`);
    expect(shares?.cells.slice(4, 8).map((c) => c.text)).toEqual(["66,67%", "0%", "33,33%", "2%"]);
    expect(averages?.cells[3]?.text).toBe("12,5%");
    expect(growth?.cells[5]?.text).toBe(`−200`);
    expect(growthBp?.cells[5]?.text).toBe("−100%");
  });

  it("drops the growth lines when there is no year before", () => {
    const t = summaryTable(summary(false));
    expect(t.footer.map((l) => l.label)).toEqual(["Сумма", "От общ. %", "Среднее"]);
    expect(t.footer[2]?.cells[3]?.text).toBe("");
  });

  it("names months in Russian", () => {
    expect(monthName("2026-01")).toBe("январь");
    expect(monthName("2026-12")).toBe("декабрь");
  });
});
