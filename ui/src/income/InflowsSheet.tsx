import { useMemo } from "react";

import type { InflowOp, MonthView } from "../api";
import type { CellChange } from "../grid/edits";
import { TableGrid } from "../grid/TableGrid";
import { inflowColumns, inflowOps } from "./income";
import styles from "./InflowsSheet.module.css";

interface Props {
  view: MonthView;
  /** Names to complete typed participants and cards against. */
  participants: readonly string[];
  cards: readonly string[];
  fontSize: number;
  onOps: (ops: InflowOp[]) => void;
  onErrors: (messages: string[]) => void;
  /** Inflows selected by their numbers and deleted with Delete; the parent confirms. */
  onDeleteRows: (ids: number[]) => void;
  onBack: () => void;
}

/**
 * The money that came in during a month (PLAN §5): salary, advance, side jobs. Their sum is
 * the «Приход» of the month, against its income plan.
 */
export function InflowsSheet({
  view,
  participants,
  cards,
  fontSize,
  onOps,
  onErrors,
  onDeleteRows,
  onBack,
}: Props) {
  const columns = useMemo(
    () => inflowColumns(view.month, participants, cards),
    [view.month, participants, cards],
  );

  const onChanges = (changes: CellChange[]) => {
    const { ops, errors } = inflowOps(changes, view.inflows, columns);
    onErrors(errors);
    if (ops.length > 0) onOps(ops);
  };

  return (
    <section className={styles.sheet} aria-labelledby="inflows-heading">
      <header className={styles.bar}>
        <h2 id="inflows-heading">Поступления за {view.title}</h2>
        <button type="button" className={`${styles.button} ${styles.back}`} onClick={onBack}>
          К журналу
        </button>
      </header>
      <p className={styles.hint}>
        Строка — деньги, которые пришли: зарплата, аванс, подработка. Новое поступление начните с
        суммы. Их сумма — «Приход» месяца в блоке справа.
      </p>
      <div className={styles.grid}>
        <TableGrid
          columns={columns}
          rows={view.inflows}
          fontSize={fontSize}
          newRowHint="Новое поступление"
          onChanges={onChanges}
          onDeleteRows={onDeleteRows}
        />
      </div>
    </section>
  );
}
