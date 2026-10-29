import { describe, expect, it } from "vitest";

import { formatDate, parseDate } from "./date";
import { formatMoney, moneyEditText, parseMoney } from "./money";
import { formatPercent, parsePercent, percentEditText } from "./percent";
import { complete, isKnown, matchOption, nameKey, parseYesNo, plural } from "./text";

const NBSP = "\u00a0";

describe("money", () => {
  it("formats kopecks with thousands groups and optional kopecks", () => {
    expect(formatMoney(0)).toBe("0");
    expect(formatMoney(5)).toBe("0,05");
    expect(formatMoney(123456)).toBe(`1${NBSP}234,56`);
    expect(formatMoney(120000)).toBe(`1${NBSP}200`);
    expect(formatMoney(123456789012)).toBe(`1${NBSP}234${NBSP}567${NBSP}890,12`);
    expect(formatMoney(-5000)).toBe("−50");
    expect(moneyEditText(123450)).toBe("1234,50");
  });

  it.each([
    ["1234", 123400],
    ["1 234,56", 123456],
    [`1${NBSP}234,5`, 123450],
    ["1234.5", 123450],
    ["0,05", 5],
    ["  12 ", 1200],
    ["1 200 ₽", 120000],
    ["350р", 35000],
    ["-50", -5000],
    ["−50,10", -5010],
  ])("parses %j", (input, kopecks) => {
    expect(parseMoney(input)).toEqual({ ok: true, value: kopecks });
  });

  it.each(["", "abc", "1,234,56", "12,345", "1.2.3", "--5", "1e5"])("rejects %j", (input) => {
    expect(parseMoney(input).ok).toBe(false);
  });

  it("round-trips", () => {
    for (const k of [0, 1, 99, 100, 123456, -77]) {
      expect(parseMoney(formatMoney(k))).toEqual({ ok: true, value: k });
    }
  });
});

describe("percent", () => {
  it("formats basis points", () => {
    expect(formatPercent(600)).toBe("6%");
    expect(formatPercent(150)).toBe("1,5%");
    expect(formatPercent(3003)).toBe("30,03%");
    expect(formatPercent(5)).toBe("0,05%");
    expect(percentEditText(150)).toBe("1,5");
  });

  it.each([
    ["6", 600],
    ["6%", 600],
    ["1,5", 150],
    ["0.25 %", 25],
    ["100", 10000],
  ])("parses %j", (input, bp) => {
    expect(parsePercent(input)).toEqual({ ok: true, value: bp });
  });

  it("clears on empty input and rejects nonsense", () => {
    expect(parsePercent(" ")).toEqual({ ok: true, value: null });
    expect(parsePercent("101").ok).toBe(false);
    expect(parsePercent("6,125").ok).toBe(false);
    expect(parsePercent("шесть").ok).toBe(false);
  });
});

describe("date", () => {
  const sheet = "2026-07";

  it.each([
    ["5", "2026-07-05"],
    ["05", "2026-07-05"],
    ["5.8", "2026-08-05"],
    ["31.12", "2026-12-31"],
    ["05.07.26", "2026-07-05"],
    ["5/7/2025", "2025-07-05"],
    ["05-07-2026", "2026-07-05"],
    ["2026-07-05", "2026-07-05"],
  ])("parses %j relative to the sheet month", (input, iso) => {
    expect(parseDate(input, sheet)).toEqual({ ok: true, value: iso });
  });

  it("knows month lengths and leap years", () => {
    expect(parseDate("29.02.2028", sheet).ok).toBe(true);
    expect(parseDate("29.02.2026", sheet).ok).toBe(false);
    expect(parseDate("31", "2026-06").ok).toBe(false);
    expect(parseDate("0", sheet).ok).toBe(false);
  });

  it("rejects nonsense and clears on empty", () => {
    expect(parseDate("вчера", sheet).ok).toBe(false);
    expect(parseDate("1.2.3.4", sheet).ok).toBe(false);
    expect(parseDate("5.7.202", sheet).ok).toBe(false);
    expect(parseDate("", sheet)).toEqual({ ok: true, value: null });
  });

  it("formats ISO as DD.MM.YYYY", () => {
    expect(formatDate("2026-07-05")).toBe("05.07.2026");
  });
});

describe("dictionary matching", () => {
  const categories = ["еда", "продукты", "обучение", "обувь", "Транспорт"];

  it("matches exact names ignoring case", () => {
    expect(matchOption("ЕДА", categories)).toEqual({ value: "еда", known: true });
    expect(matchOption("транспорт", categories)).toEqual({ value: "Транспорт", known: true });
  });

  it("completes a unique prefix", () => {
    expect(matchOption("прод", categories)).toEqual({ value: "продукты", known: true });
    expect(matchOption("а", ["Андрей", "Марина"])).toEqual({ value: "Андрей", known: true });
  });

  it("keeps ambiguous or unknown text as typed", () => {
    expect(matchOption("об", categories)).toEqual({ value: "об", known: false });
    expect(matchOption(" кино ", categories)).toEqual({ value: "кино", known: false });
    expect(matchOption("  ", categories)).toBeNull();
  });

  it("treats ё as е and ignores repeated spaces, like the core", () => {
    expect(nameKey("  Пятёрочка  ")).toBe("пятерочка");
    expect(matchOption("газпром   кр", ["Газпром КР", "Газпром А"])).toEqual({
      value: "Газпром КР",
      known: true,
    });
    expect(matchOption("елка", ["Ёлка"])).toEqual({ value: "Ёлка", known: true });
    expect(isKnown("Газпром  КР", ["газпром кр"])).toBe(true);
  });

  it("reads да/нет", () => {
    expect(parseYesNo("да")).toEqual({ ok: true, value: true });
    expect(parseYesNo("Д")).toEqual({ ok: true, value: true });
    expect(parseYesNo("нет")).toEqual({ ok: true, value: false });
    expect(parseYesNo("")).toEqual({ ok: true, value: false });
    expect(parseYesNo("может").ok).toBe(false);
    expect(parseYesNo("может", "Активна")).toEqual({
      ok: false,
      message: "В колонке «Активна» пишут «да» или «нет», а не «может»",
    });
  });
});

describe("autocomplete of titles", () => {
  const history = ["Пятёрочка", "Перекрёсток", "Пятёрочка у дома", "Аптека"];

  it("completes from the most recent matching title", () => {
    expect(complete("пят", history)).toBe("Пятёрочка");
    expect(complete("Пер", history)).toBe("Перекрёсток");
    expect(complete("пятерочка ", history)).toBe("Пятёрочка у дома");
  });

  it("does not complete a full title, nothing, or an unknown start", () => {
    expect(complete("Аптека", history)).toBeNull();
    expect(complete("", history)).toBeNull();
    expect(complete("Лента", history)).toBeNull();
  });
});

describe("plural", () => {
  it.each([
    [1, "строка"],
    [2, "строки"],
    [5, "строк"],
    [11, "строк"],
    [21, "строка"],
    [104, "строки"],
  ])("%i → %s", (n, word) => {
    expect(plural(n, "строка", "строки", "строк")).toBe(word);
  });
});
