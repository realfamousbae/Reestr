import { describe, expect, it } from "vitest";

import type { TakeReport } from "../api";
import { bannerText, changesText, exchangeFileNames, takenText } from "./phone";

function report(over: Partial<TakeReport>): TakeReport {
  return { added: 0, changed: 0, deleted: 0, kept: 0, months: [], created: [], ...over };
}

describe("exchange with the phone", () => {
  it("names the exchange files after the file", () => {
    expect(exchangeFileNames("Семья.reestr")).toEqual({
      forPhone: "Семья — для телефона.reestr",
      fromPhone: "Семья — с телефона.reestr",
    });
  });

  it("counts the changes in Russian", () => {
    expect(changesText(report({ added: 1 }))).toBe("1 новая строка");
    expect(changesText(report({ added: 2, changed: 1, deleted: 5 }))).toBe(
      "2 новые строки, 1 изменена, 5 удалено",
    );
    expect(changesText(report({ added: 11, changed: 3 }))).toBe("11 новых строк, 3 изменены");
    expect(changesText(report({ kept: 1 }))).toBeNull();
  });

  it("says what the banner offers", () => {
    expect(bannerText(report({ added: 3 }))).toBe("3 новые строки.");
    expect(bannerText(report({ added: 1, kept: 2 }))).toBe(
      "1 новая строка; 2 строки меняли и здесь, и на телефоне — оставлено, как на компьютере.",
    );
  });

  it("says what was taken and where", () => {
    expect(takenText(report({ added: 2, months: ["2026-10", "2026-11"] }))).toBe(
      "Забрано с телефона (10.26, 11.26): 2 новые строки. Ctrl+Z отменит.",
    );
    expect(takenText(report({ kept: 1 }))).toBe(
      "1 строку меняли и здесь, и на телефоне — оставлено, как на компьютере.",
    );
    expect(takenText(report({}))).toBe("С телефона нечего забрать.");
  });
});
