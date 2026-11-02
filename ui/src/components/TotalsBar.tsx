import type { MonthTotals } from "../api";
import { formatMoney } from "../format/money";
import styles from "./TotalsBar.module.css";

interface Props {
  totals: MonthTotals;
  /** Show groups («Лечение/лекарства») instead of their categories. */
  grouped: boolean;
  onGroupedChange: (grouped: boolean) => void;
}

function Cell({
  label,
  value,
  kind,
}: {
  label: string;
  value: string;
  kind?: "total" | "warn" | "zero";
}) {
  return (
    <div className={`${styles.cell} ${kind === undefined ? "" : styles[kind]}`}>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

/**
 * The sums row above the journal. «Итого» stays on the left and «Регулярные» on the right;
 * categories scroll in between when the window is narrow. Cashback is a block on the right of
 * the sheet (D22).
 */
export function TotalsBar({ totals, grouped, onGroupedChange }: Props) {
  const columns = grouped ? totals.byGroup : totals.byCategory;
  return (
    <section className={styles.bar} aria-label="Итоги месяца">
      <div className={styles.fixed}>
        <dl className={styles.list}>
          <Cell label="Итого, ₽" value={formatMoney(totals.total)} kind="total" />
        </dl>
        <div className={styles.mode} role="radiogroup" aria-label="Итоги">
          <button
            type="button"
            role="radio"
            aria-checked={!grouped}
            onClick={() => {
              onGroupedChange(false);
            }}
          >
            Категории
          </button>
          <button
            type="button"
            role="radio"
            aria-checked={grouped}
            onClick={() => {
              onGroupedChange(true);
            }}
          >
            Группы
          </button>
        </div>
      </div>
      <dl className={`${styles.list} ${styles.categories}`}>
        {columns.map((c) => (
          <Cell
            key={`${c.isGroup ? "g" : "c"}${c.id.toString()}`}
            label={c.label}
            value={formatMoney(c.amount)}
            {...(c.amount === 0 ? { kind: "zero" as const } : {})}
          />
        ))}
        {totals.uncategorized !== 0 && (
          <Cell label="Без назначения" value={formatMoney(totals.uncategorized)} kind="warn" />
        )}
      </dl>
      <dl className={`${styles.list} ${styles.right}`}>
        <Cell label="Регулярные" value={formatMoney(totals.regular)} />
      </dl>
    </section>
  );
}
