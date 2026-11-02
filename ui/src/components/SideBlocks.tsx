import { type ReactNode, useState } from "react";

import type { Kopecks, MonthTotals, MonthView, ProductsView } from "../api";
import { formatDate } from "../format/date";
import { formatMoney } from "../format/money";
import { formatPercent } from "../format/percent";
import { monthTitle, parsePlan } from "../income/income";
import { daysText, deadlineText, savingsText, trancheTone } from "../products/labels";
import { remainingText, ruleTitle } from "../rules/limits";
import styles from "./SideBlocks.module.css";
import { TermField } from "./TermField";

const COLLAPSED_KEY = "reestr.collapsedBlocks";

function readCollapsed(): string[] {
  try {
    const raw = localStorage.getItem(COLLAPSED_KEY);
    const parsed: unknown = raw === null ? [] : JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

function writeCollapsed(ids: string[]): void {
  try {
    localStorage.setItem(COLLAPSED_KEY, JSON.stringify(ids));
  } catch {
    // A view preference; losing it is harmless.
  }
}

interface BlockProps {
  id: string;
  title: string;
  /** Buttons or a short figure next to the title. */
  actions?: ReactNode;
  children: ReactNode;
}

/** A block on the right of the month sheet; the title folds it (PLAN §8). */
export function Block({ id, title, actions, children }: BlockProps) {
  const [collapsed, setCollapsed] = useState(() => readCollapsed().includes(id));
  const toggle = () => {
    const others = readCollapsed().filter((x) => x !== id);
    writeCollapsed(collapsed ? others : [...others, id]);
    setCollapsed(!collapsed);
  };
  return (
    <section className={styles.block} aria-labelledby={`${id}-heading`}>
      <header className={styles.header}>
        <h2 id={`${id}-heading`} className={styles.heading}>
          <button type="button" className={styles.fold} aria-expanded={!collapsed} onClick={toggle}>
            <span className={styles.chevron} aria-hidden="true">
              {collapsed ? "▸" : "▾"}
            </span>
            {title}
          </button>
        </h2>
        {!collapsed && actions}
      </header>
      {!collapsed && children}
    </section>
  );
}

/** «Итого кэшбэк / Ср. %» — the cells L7:M8 of an exported month sheet (D22). */
export function CashbackBlock({ totals }: { totals: MonthTotals }) {
  return (
    <Block id="cashback" title="Кэшбэк">
      <dl className={styles.figures}>
        <div>
          <dt>Итого, ₽</dt>
          <dd className={styles.big}>{formatMoney(totals.cashback)}</dd>
        </div>
        <div>
          <dt>Ср. %</dt>
          <dd className={styles.big}>
            {totals.cashbackRateBp === null ? "—" : formatPercent(totals.cashbackRateBp)}
          </dd>
        </div>
      </dl>
    </Block>
  );
}

interface LimitsProps {
  view: MonthView;
  onOpenRules: () => void;
  onCopyRules: (from: string) => void;
}

/** «Лимиты по картам»: the month's rules with what is left of them (PLAN §8). */
export function LimitsBlock({ view, onOpenRules, onCopyRules }: LimitsProps) {
  const from = view.copyRulesFrom;
  return (
    <Block
      id="limits"
      title="Лимиты по картам"
      actions={
        <button type="button" className={styles.link} onClick={onOpenRules}>
          Правила
        </button>
      }
    >
      {view.rules.length === 0 ? (
        <div className={styles.empty}>
          <p>Правил кэшбэка на {view.title} нет.</p>
          {from !== null && (
            <button
              type="button"
              className={styles.link}
              onClick={() => {
                onCopyRules(from.month);
              }}
            >
              Скопировать из {from.title}
            </button>
          )}
        </div>
      ) : (
        <>
          <ul className={styles.list}>
            {view.rules.map((r) => (
              <li key={r.id} className={r.exhausted ? styles.danger : undefined}>
                <span className={styles.title}>{ruleTitle(r)}</span>
                <span className={styles.meta}>
                  потрачено {formatMoney(r.spent)} ₽ · кэшбэк {formatMoney(r.cashback)} ₽
                </span>
                <span className={styles.state}>{remainingText(r)}</span>
              </li>
            ))}
          </ul>
          <p className={styles.note}>По данным журнала.</p>
        </>
      )}
    </Block>
  );
}

interface ProductsProps {
  products: ProductsView;
  onOpen: () => void;
}

/** «Кредитки / грейс»: what each card owes and when (PLAN §7, §8). */
export function CreditBlock({ products, onOpen }: ProductsProps) {
  const owing = products.credit.filter((c) => c.debt > 0);
  return (
    <Block
      id="credit"
      title="Кредитки / грейс"
      actions={
        <button type="button" className={styles.link} onClick={onOpen}>
          Продукты
        </button>
      }
    >
      {owing.length === 0 ? (
        <p className={styles.emptyLine}>Долгов по кредиткам нет.</p>
      ) : (
        <>
          <ul className={styles.list}>
            {owing.map((c) => {
              const next = c.tranches
                .filter((t) => t.remaining > 0)
                .sort((a, b) => a.end.localeCompare(b.end))[0];
              const tone = next === undefined ? undefined : trancheTone(next);
              return (
                <li key={c.productId} className={tone === undefined ? undefined : styles[tone]}>
                  <span className={styles.title}>
                    {c.card} — {formatMoney(c.debt)} ₽
                  </span>
                  {next !== undefined && (
                    <span className={styles.state}>
                      {formatMoney(next.remaining)} ₽ до {formatDate(next.end)} (
                      {daysText(next.daysLeft)})
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
          <p className={styles.note}>
            Всего долг: <strong>{formatMoney(products.debt)} ₽</strong>
          </p>
        </>
      )}
    </Block>
  );
}

/** «Накопительные счета»: what can be opened now and what waits (PLAN §7). */
export function SavingsBlock({ products, onOpen }: ProductsProps) {
  const closed = products.savings.filter((s) => s.state.state !== "open");
  return (
    <Block
      id="savings"
      title="Накопительные счета"
      actions={
        <button type="button" className={styles.link} onClick={onOpen}>
          Продукты
        </button>
      }
    >
      {closed.length === 0 ? (
        <p className={styles.emptyLine}>Закрытых счетов с ожиданием нет.</p>
      ) : (
        <ul className={styles.list}>
          {closed.map((s) => (
            <li
              key={s.productId}
              className={s.state.state === "canOpen" ? styles.ready : undefined}
            >
              <span className={styles.title}>
                {s.card}
                {s.promoRateBp === null ? "" : ` · ${formatPercent(s.promoRateBp)}`}
              </span>
              <span className={styles.state}>{savingsText(s.state)}</span>
            </li>
          ))}
        </ul>
      )}
    </Block>
  );
}

interface IncomeProps {
  view: MonthView;
  /** The plan from this month on; `null` returns to the plan of an earlier month. */
  onPlan: (amount: Kopecks | null) => void;
  onError: (message: string) => void;
  onOpenInflows: () => void;
}

function Figure({ label, value, hint }: { label: string; value: Kopecks | null; hint?: string }) {
  return (
    <div className={value !== null && value < 0 ? styles.danger : undefined} title={hint}>
      <dt>{label}</dt>
      <dd>{value === null ? "—" : formatMoney(value)}</dd>
    </div>
  );
}

/** «Приход / расход / дельта»: the month against its income plan (PLAN §8). */
export function IncomeBlock({ view, onPlan, onError, onOpenInflows }: IncomeProps) {
  const b = view.balance;
  const carried = b.plan !== null && b.plan.setIn !== view.month;
  return (
    <Block
      id="income"
      title="Приход / расход"
      actions={
        <button type="button" className={styles.link} onClick={onOpenInflows}>
          Поступления
        </button>
      }
    >
      <div className={styles.plan}>
        <TermField
          key={view.month}
          label="План дохода, ₽"
          value={b.plan === null ? "" : formatMoney(b.plan.amount)}
          placeholder="не задан"
          parse={parsePlan}
          onSave={onPlan}
          onError={onError}
        />
        <span className={styles.meta}>
          {b.plan === null
            ? "переносится на следующие месяцы"
            : carried
              ? `из ${monthTitle(b.plan.setIn)}; новая сумма — с этого месяца`
              : "с этого месяца"}
        </span>
      </div>
      <dl className={styles.rows}>
        <Figure label="Приход" value={b.inflow} />
        <Figure label="Расход" value={b.spent} />
        <Figure label="Дельта по плану" value={b.planned} hint="План дохода минус расход" />
        <Figure label="Дельта по факту" value={b.actual} hint="Приход минус расход" />
        <Figure
          label="Общий баланс"
          value={b.overall}
          hint="План дохода плюс кэшбэк минус расход, как в «Своде»"
        />
      </dl>
    </Block>
  );
}

/** «Сроки»: the nearest five (PLAN §7). */
export function DeadlinesBlock({ products, onOpen }: ProductsProps) {
  const next = products.deadlines.slice(0, 5);
  return (
    <Block
      id="deadlines"
      title="Сроки"
      actions={
        <button type="button" className={styles.link} onClick={onOpen}>
          Все
        </button>
      }
    >
      {next.length === 0 ? (
        <p className={styles.emptyLine}>Сроков нет.</p>
      ) : (
        <ul className={styles.list}>
          {next.map((d) => (
            <li
              key={`${d.source.kind}-${d.date}-${deadlineText(d)}`}
              className={
                d.urgency === "overdue"
                  ? styles.danger
                  : d.urgency === "soon"
                    ? styles.warn
                    : undefined
              }
            >
              <span className={styles.title}>{deadlineText(d)}</span>
              <span className={styles.state}>
                {formatDate(d.date)} · {daysText(d.daysLeft)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </Block>
  );
}
