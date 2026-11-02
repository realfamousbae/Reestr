// Words for product states: tranche periods, days left, deadlines. Numbers come from the core.

import type { AvailableCheck, DeadlineView, SavingsState, TrancheView } from "../api";
import { formatDate } from "../format/date";
import { formatMoney } from "../format/money";
import { plural } from "../format/text";

/** "2026-06-01" → "01.06.26". */
export function shortDate(iso: string): string {
  const [year = "", month = "", day = ""] = iso.split("-");
  return `${day}.${month}.${year.slice(2)}`;
}

/** «01.06.26–28.09.26»: how a tranche is named in tables and lists. */
export function trancheLabel(t: Pick<TrancheView, "start" | "end">): string {
  return `${shortDate(t.start)}–${shortDate(t.end)}`;
}

/** «через 3 дня», «сегодня», «просрочено на 2 дня». */
export function daysText(daysLeft: number): string {
  if (daysLeft === 0) return "сегодня";
  if (daysLeft === 1) return "завтра";
  if (daysLeft > 1) {
    return `через ${daysLeft.toString()} ${plural(daysLeft, "день", "дня", "дней")}`;
  }
  const late = -daysLeft;
  return `просрочено на ${late.toString()} ${plural(late, "день", "дня", "дней")}`;
}

export function trancheStatusText(t: Pick<TrancheView, "status" | "daysLeft">): string {
  if (t.status === "repaid") return "погашен";
  return daysText(t.daysLeft);
}

/** A state worth a color: overdue in red, close in amber (PLAN §7: ≤ 7 days) if owing. */
export function trancheTone(
  t: Pick<TrancheView, "status" | "daysLeft" | "remaining">,
): "danger" | "warn" | undefined {
  if (t.status === "overdue") return "danger";
  if (t.status === "active" && t.remaining > 0 && t.daysLeft <= 7) return "warn";
  return undefined;
}

/** «На 18.12.2026: долг 58 000 ₽, расчётный остаток 92 000 ₽, у банка на 300 ₽ больше». */
export function availableCheckText(check: AvailableCheck): string {
  const base =
    `На ${formatDate(check.on)}: долг ${formatMoney(check.debt)} ₽, ` +
    `расчётный остаток ${formatMoney(check.computed)} ₽`;
  if (check.difference === 0) return `${base}, совпадает с банком`;
  const more = check.difference > 0;
  const gap = formatMoney(more ? check.difference : -check.difference);
  return `${base}, у банка на ${gap} ₽ ${more ? "больше" : "меньше"}`;
}

export function savingsText(state: SavingsState): string {
  switch (state.state) {
    case "open":
      return "открыт";
    case "waiting":
      return `можно открыть с ${formatDate(state.from)} (${daysText(state.daysLeft)})`;
    case "canOpen":
      return `можно открыть — с ${formatDate(state.since)}`;
  }
}

/** What a deadline is about: «Газпром КР: конец грейса, к оплате 2 300 ₽». */
export function deadlineText(d: DeadlineView): string {
  const card = d.card ?? "";
  switch (d.source.kind) {
    case "graceEnd":
      return `${card}: конец грейса, к оплате ${formatMoney(d.source.remaining)} ₽`;
    case "savingsReopen":
      return `${card}: можно снова открыть`;
    case "custom":
      return card === "" ? d.source.title : `${d.source.title} (${card})`;
  }
}
