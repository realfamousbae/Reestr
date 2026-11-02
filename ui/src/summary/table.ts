// «Свод» as text cells (PLAN §8): its columns in their order, months, then the footer. Every
// figure comes from the core; this only formats it.

import type { FooterLine, Kopecks, SummaryRow, YearSummary } from "../api";
import { formatMoney } from "../format/money";
import { formatPercent } from "../format/percent";

export interface SummaryCell {
  text: string;
  /** "danger" for a month over its plan, "muted" for a zero. */
  tone?: "danger" | "muted";
}

export interface SummaryColumnHead {
  key: string;
  title: string;
}

export interface SummaryLine {
  key: string;
  label: string;
  cells: SummaryCell[];
  /** A month without entries. */
  empty?: boolean;
}

export interface SummaryTable {
  head: SummaryColumnHead[];
  months: SummaryLine[];
  footer: SummaryLine[];
}

const MONTH_NAMES = [
  "январь",
  "февраль",
  "март",
  "апрель",
  "май",
  "июнь",
  "июль",
  "август",
  "сентябрь",
  "октябрь",
  "ноябрь",
  "декабрь",
];

const BLANK: SummaryCell = { text: "" };

function money(value: Kopecks | null): SummaryCell {
  if (value === null) return BLANK;
  return value === 0 ? { text: formatMoney(0), tone: "muted" } : { text: formatMoney(value) };
}

/** A balance: below zero means the month spent more than planned. */
function balance(value: Kopecks | null): SummaryCell {
  if (value === null) return BLANK;
  return value < 0 ? { text: formatMoney(value), tone: "danger" } : { text: formatMoney(value) };
}

function percent(bp: number | null): SummaryCell {
  return bp === null ? BLANK : { text: formatPercent(bp) };
}

export function monthName(month: string): string {
  const m = Number(month.split("-")[1] ?? "0");
  return MONTH_NAMES[m - 1] ?? month;
}

function monthLine(row: SummaryRow, width: number): SummaryLine {
  if (!row.hasData) {
    return {
      key: row.month,
      label: monthName(row.month),
      cells: [BLANK, money(row.plan), ...Array<SummaryCell>(width - 2).fill(BLANK)],
      empty: true,
    };
  }
  return {
    key: row.month,
    label: monthName(row.month),
    cells: [
      money(row.total),
      money(row.plan),
      balance(row.balance),
      percent(row.growthBp),
      ...row.columns.map(money),
      money(row.regular),
      money(row.cashback),
      balance(row.overallBalance),
      percent(row.cashbackRateBp),
    ],
  };
}

/** A footer line: the first four cells (sum, plan, balance, growth), the columns, two after. */
function footerLine<T>(
  key: string,
  label: string,
  line: FooterLine<T>,
  show: (value: T) => SummaryCell,
  first: SummaryCell[],
  last: SummaryCell[],
): SummaryLine {
  return {
    key,
    label,
    cells: [...first, ...line.columns.map(show), show(line.regular), show(line.cashback), ...last],
  };
}

function hasAny<T>(line: FooterLine<T | null>): boolean {
  return [...line.columns, line.regular, line.cashback].some((v) => v !== null);
}

export function summaryTable(s: YearSummary): SummaryTable {
  const head: SummaryColumnHead[] = [
    { key: "total", title: "Сумма" },
    { key: "plan", title: "План дохода" },
    { key: "balance", title: "Баланс в мес." },
    { key: "growth", title: "Прирост к прошлому году" },
    ...s.columns.map((c) => ({
      key: `${c.isGroup ? "g" : "c"}${c.id.toString()}`,
      title: c.label,
    })),
    { key: "regular", title: "Регулярные" },
    { key: "cashback", title: "Кешбек" },
    { key: "overall", title: "Общий баланс" },
    { key: "rate", title: "Общий % кешбека" },
  ];
  const f = s.footer;
  const footer: SummaryLine[] = [
    footerLine(
      "sums",
      "Сумма",
      f.sums,
      money,
      [money(f.total), BLANK, BLANK, BLANK],
      [balance(f.overallBalance), BLANK],
    ),
    footerLine(
      "shares",
      "От общ. %",
      f.sharesBp,
      percent,
      [BLANK, BLANK, BLANK, BLANK],
      [BLANK, BLANK],
    ),
    footerLine(
      "averages",
      "Среднее",
      f.averages,
      money,
      [money(f.averageTotal), BLANK, BLANK, percent(f.averageTotalGrowthBp)],
      [BLANK, BLANK],
    ),
  ];
  // The comparison with the year before, when there is one.
  if (hasAny(f.averageGrowth)) {
    footer.push(
      footerLine(
        "growth",
        "Ср. прирост, ₽",
        f.averageGrowth,
        money,
        [BLANK, BLANK, BLANK, BLANK],
        [BLANK, BLANK],
      ),
      footerLine(
        "growthBp",
        "Ср. прирост, %",
        f.averageGrowthBp,
        percent,
        [BLANK, BLANK, BLANK, BLANK],
        [BLANK, BLANK],
      ),
    );
  }
  return {
    head,
    months: s.months.map((row) => monthLine(row, head.length)),
    footer,
  };
}
