import { useMemo } from "react";

import type { MonthView, RuleOp } from "../api";
import type { CellChange } from "../grid/edits";
import { TableGrid } from "../grid/TableGrid";
import styles from "./RulesSheet.module.css";
import { buildRuleOps, ruleColumns } from "./specs";

interface Props {
  view: MonthView;
  /** Names to complete typed cards and categories against. */
  cards: readonly string[];
  categories: readonly string[];
  fontSize: number;
  onOps: (ops: RuleOp[]) => void;
  onErrors: (messages: string[]) => void;
  /** Rules selected by their numbers and deleted with Delete; the parent confirms. */
  onDeleteRows: (ids: number[]) => void;
  onSuggestionsChange: (enabled: boolean) => void;
  onBack: () => void;
}

/**
 * The cashback rules of a month (PLAN §6): what each card pays for which category, its limits,
 * and what is used of them by the journal. Rules for a new month start as a copy of the
 * previous month's, like a new sheet of the source workbook.
 */
export function RulesSheet({
  view,
  cards,
  categories,
  fontSize,
  onOps,
  onErrors,
  onDeleteRows,
  onSuggestionsChange,
  onBack,
}: Props) {
  const columns = useMemo(() => ruleColumns(cards, categories), [cards, categories]);
  const from = view.copyRulesFrom;

  const onChanges = (changes: CellChange[]) => {
    const { ops, errors } = buildRuleOps(changes, view.rules, columns);
    onErrors(errors);
    if (ops.length > 0) onOps(ops);
  };

  return (
    <section className={styles.sheet} aria-labelledby="rules-heading">
      <header className={styles.bar}>
        <h2 id="rules-heading">Правила кэшбэка на {view.title}</h2>
        {from !== null && (
          <button
            type="button"
            className={styles.button}
            onClick={() => {
              onOps([{ op: "copyFrom", from: from.month }]);
            }}
          >
            Скопировать из {from.title}
          </button>
        )}
        <label className={styles.toggle}>
          <input
            type="checkbox"
            checked={view.cardSuggestions}
            onChange={(e) => {
              onSuggestionsChange(e.target.checked);
            }}
          />
          Подсказывать карту при вводе
        </label>
        <button type="button" className={`${styles.button} ${styles.back}`} onClick={onBack}>
          К журналу
        </button>
      </header>
      <p className={styles.hint}>
        Строка — правило карты на месяц. Пустая категория — «на всё»: всё, на что у карты нет своего
        правила. Корректировка — траты, которых нет в журнале. Лимиты расходуются в порядке строк
        журнала.
      </p>
      <div className={styles.grid}>
        <TableGrid
          columns={columns}
          rows={view.rules}
          fontSize={fontSize}
          newRowHint="Новое правило"
          onChanges={onChanges}
          onDeleteRows={onDeleteRows}
        />
      </div>
    </section>
  );
}
