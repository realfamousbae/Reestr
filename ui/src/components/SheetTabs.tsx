import type { MonthTab } from "../api";
import styles from "./SheetTabs.module.css";

/**
 * A sheet of the workbook: a month, «Свод», the products, the deadlines, or the dictionaries
 * (the source «Служебный» sheet).
 */
export type Sheet =
  | { kind: "month"; month: string }
  | { kind: "summary" }
  | { kind: "products" }
  | { kind: "deadlines" }
  | { kind: "dictionaries" };

interface Props {
  tabs: readonly MonthTab[];
  active: Sheet;
  /** Deadlines that are overdue or within a week: shown on the «Сроки» tab. */
  urgent: number;
  onSelect: (sheet: Sheet) => void;
  onAddMonth: () => void;
}

const SERVICE: { kind: Exclude<Sheet["kind"], "month">; title: string }[] = [
  { kind: "summary", title: "Свод" },
  { kind: "products", title: "Продукты" },
  { kind: "deadlines", title: "Сроки" },
  { kind: "dictionaries", title: "Справочники" },
];

/** Sheet tabs at the bottom, like in the workbook. */
export function SheetTabs({ tabs, active, urgent, onSelect, onAddMonth }: Props) {
  return (
    <div className={styles.strip}>
      <div role="tablist" aria-label="Листы" className={styles.tabs}>
        {tabs.map((tab) => (
          <button
            key={tab.month}
            type="button"
            role="tab"
            aria-selected={active.kind === "month" && tab.month === active.month}
            className={styles.tab}
            onClick={() => {
              onSelect({ kind: "month", month: tab.month });
            }}
          >
            {tab.title}
          </button>
        ))}
        <button
          type="button"
          className={styles.add}
          onClick={onAddMonth}
          title="Добавить следующий месяц"
          aria-label="Добавить следующий месяц"
        >
          +
        </button>
        {SERVICE.map((s, i) => (
          <button
            key={s.kind}
            type="button"
            role="tab"
            aria-selected={active.kind === s.kind}
            className={`${styles.tab} ${i === 0 ? styles.service : ""}`}
            onClick={() => {
              onSelect({ kind: s.kind });
            }}
          >
            {s.title}
            {s.kind === "deadlines" && urgent > 0 && (
              <span className={styles.badge} aria-label={`срочных: ${urgent.toString()}`}>
                {urgent}
              </span>
            )}
          </button>
        ))}
      </div>
    </div>
  );
}
