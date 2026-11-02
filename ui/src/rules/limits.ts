import type { RuleView, SuggestionView } from "../api";
import { formatMoney } from "../format/money";
import { formatPercent } from "../format/percent";
import { EVERYTHING } from "./specs";

/** «Райф А · продукты 5%», «Райф А · MCC 5411 5%». */
export function ruleTitle(rule: RuleView): string {
  const scope = rule.mcc === null ? (rule.category ?? EVERYTHING) : `MCC ${rule.mcc}`;
  return `${rule.card} · ${scope} ${formatPercent(rule.rateBp)}`;
}

/** What is left of a rule's limits, in words. */
export function remainingText(
  rule: Pick<RuleView, "exhausted" | "remainingSpend" | "remainingCashback">,
): string {
  if (rule.exhausted) return "лимит исчерпан";
  const spend = rule.remainingSpend;
  const cashback = rule.remainingCashback;
  if (spend !== null && cashback !== null) {
    return `осталось: трат ${formatMoney(spend)} ₽, кэшбэка ${formatMoney(cashback)} ₽`;
  }
  if (spend !== null) return `осталось трат ${formatMoney(spend)} ₽`;
  if (cashback !== null) return `осталось кэшбэка ${formatMoney(cashback)} ₽`;
  return "без лимита";
}

/** The second line of a card suggestion: the cashback for the amount, or what is left. */
export function suggestionText(s: SuggestionView): string {
  if (s.exhausted) return "лимит исчерпан";
  if (s.expected !== null) return `кэшбэк ${formatMoney(s.expected)} ₽`;
  const left = remainingText(s);
  return left === "без лимита" ? "" : left;
}
