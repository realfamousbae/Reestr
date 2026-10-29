import { useMemo } from "react";

import type { ProductOp, ProductsView } from "../api";
import { formatDate } from "../format/date";
import type { CellChange } from "../grid/edits";
import { TableGrid } from "../grid/TableGrid";
import styles from "./DeadlinesSheet.module.css";
import { daysText, deadlineText } from "./labels";
import { reminderColumns, reminderOps } from "./tables";

interface Props {
  view: ProductsView;
  cards: readonly string[];
  /** "YYYY-MM": short dates are read in this month. */
  month: string;
  fontSize: number;
  onOps: (ops: ProductOp[]) => void;
  onErrors: (messages: string[]) => void;
  onDeleteReminders: (ids: number[]) => void;
}

/**
 * «Сроки» (PLAN §7): ends of unpaid grace periods, dates when a savings account can be opened
 * again, and the user's own reminders. Notifications stay inside the program.
 */
export function DeadlinesSheet({
  view,
  cards,
  month,
  fontSize,
  onOps,
  onErrors,
  onDeleteReminders,
}: Props) {
  const columns = useMemo(() => reminderColumns(cards, month), [cards, month]);
  const onChanges = (changes: CellChange[]) => {
    const { ops, errors } = reminderOps(changes, view.reminders, columns);
    onErrors(errors);
    if (ops.length > 0) onOps(ops);
  };

  return (
    <div className={styles.sheet}>
      <section className={styles.list} aria-labelledby="deadlines-heading">
        <h2 id="deadlines-heading">Ближайшие сроки</h2>
        {view.deadlines.length === 0 ? (
          <p className={styles.empty}>Сроков нет.</p>
        ) : (
          <table className={styles.table}>
            <thead>
              <tr>
                <th className={styles.num}>Дата</th>
                <th>Когда</th>
                <th>Что</th>
              </tr>
            </thead>
            <tbody>
              {view.deadlines.map((d) => (
                <tr
                  key={`${d.source.kind}-${d.date}-${d.card ?? ""}-${deadlineText(d)}`}
                  className={styles[d.urgency]}
                >
                  <td className={styles.num}>{formatDate(d.date)}</td>
                  <td>{daysText(d.daysLeft)}</td>
                  <td>{deadlineText(d)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
      <section className={styles.reminders} aria-labelledby="reminders-heading">
        <h2 id="reminders-heading">
          Напоминания <span>— свои сроки: закрыть вклад, перевыпустить карту</span>
        </h2>
        <div className={styles.grid}>
          <TableGrid
            columns={columns}
            rows={view.reminders}
            fontSize={fontSize}
            newRowHint="Новое напоминание"
            onChanges={onChanges}
            onDeleteRows={onDeleteReminders}
          />
        </div>
      </section>
    </div>
  );
}
