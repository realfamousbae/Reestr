import { describe, expect, it } from "vitest";

import type { DeadlineView } from "../api";
import {
  daysText,
  deadlineText,
  savingsText,
  shortDate,
  trancheLabel,
  trancheStatusText,
  trancheTone,
} from "./labels";

const NBSP = " ";

describe("product labels", () => {
  it("names tranches by their dates", () => {
    expect(shortDate("2026-06-01")).toBe("01.06.26");
    expect(trancheLabel({ start: "2026-06-01", end: "2026-09-28" })).toBe("01.06.26–28.09.26");
  });

  it("says how many days are left", () => {
    expect(daysText(0)).toBe("сегодня");
    expect(daysText(1)).toBe("завтра");
    expect(daysText(3)).toBe("через 3 дня");
    expect(daysText(21)).toBe("через 21 день");
    expect(daysText(-2)).toBe("просрочено на 2 дня");
    expect(daysText(-11)).toBe("просрочено на 11 дней");
  });

  it("colors only states that matter", () => {
    expect(trancheTone({ status: "overdue", daysLeft: -1, remaining: 100 })).toBe("danger");
    expect(trancheTone({ status: "active", daysLeft: 7, remaining: 100 })).toBe("warn");
    expect(trancheTone({ status: "active", daysLeft: 8, remaining: 100 })).toBeUndefined();
    // A fresh tranche with nothing bought: nothing to warn about.
    expect(trancheTone({ status: "active", daysLeft: 2, remaining: 0 })).toBeUndefined();
    expect(trancheTone({ status: "repaid", daysLeft: 2, remaining: 0 })).toBeUndefined();
    expect(trancheStatusText({ status: "repaid", daysLeft: -30 })).toBe("погашен");
  });

  it("describes savings and deadlines", () => {
    expect(savingsText({ state: "open" })).toBe("открыт");
    expect(savingsText({ state: "waiting", from: "2026-11-30", daysLeft: 34 })).toBe(
      "можно открыть с 30.11.2026 (через 34 дня)",
    );
    expect(savingsText({ state: "canOpen", since: "2026-10-01" })).toBe(
      "можно открыть — с 01.10.2026",
    );
    const base: DeadlineView = {
      date: "2026-08-28",
      card: "Газпром КР",
      daysLeft: 3,
      urgency: "soon",
      source: { kind: "graceEnd", gracePeriodId: 2, remaining: 230_000 },
    };
    expect(deadlineText(base)).toBe(`Газпром КР: конец грейса, к оплате 2${NBSP}300 ₽`);
    expect(deadlineText({ ...base, source: { kind: "savingsReopen" } })).toBe(
      "Газпром КР: можно снова открыть",
    );
    expect(
      deadlineText({ ...base, card: null, source: { kind: "custom", id: 1, title: "Налог" } }),
    ).toBe("Налог");
    expect(deadlineText({ ...base, source: { kind: "custom", id: 1, title: "Перевыпуск" } })).toBe(
      "Перевыпуск (Газпром КР)",
    );
  });
});
