import { useEffect, useMemo, useState } from "react";

import type { Backend, YearSummary } from "../api";
import { plural } from "../format/text";
import styles from "./SummarySheet.module.css";
import { type SummaryLine, summaryTable } from "./table";

interface Props {
  backend: Backend;
  /** The year to open: that of the month sheet shown last. */
  initialYear: number;
  /** Groups («Лечение/лекарства») instead of their categories, as in the month totals. */
  grouped: boolean;
  onGroupedChange: (grouped: boolean) => void;
  /** Changes when the data changes under the open sheet (undo), so it reloads. */
  revision: number;
  fontSize: number;
  onError: (error: unknown) => void;
}

function Line({ line, footer }: { line: SummaryLine; footer?: boolean }) {
  return (
    <tr className={line.empty === true ? styles.empty : undefined}>
      <th scope="row" className={footer === true ? styles.footLabel : styles.label}>
        {line.label}
      </th>
      {line.cells.map((cell, i) => (
        <td key={i} className={cell.tone === undefined ? undefined : styles[cell.tone]}>
          {cell.text}
        </td>
      ))}
    </tr>
  );
}

/**
 * «Свод» (PLAN §8): the months of a year with its columns, and the year's sums, shares,
 * averages and growth against the year before underneath. Nothing here is typed in: it is
 * computed from the journal, the income plan and the inflows.
 */
export function SummarySheet({
  backend,
  initialYear,
  grouped,
  onGroupedChange,
  revision,
  fontSize,
  onError,
}: Props) {
  const [years, setYears] = useState<number[] | null>(null);
  const [year, setYear] = useState(initialYear);
  const [summary, setSummary] = useState<YearSummary | null>(null);

  useEffect(() => {
    let live = true;
    backend
      .summaryYears()
      .then((list) => {
        if (!live) return;
        setYears(list);
        // A year without entries has nothing to show: open the nearest one that has them.
        setYear((y) => {
          if (list.length === 0 || list.includes(y)) return y;
          return list.filter((x) => x < y).at(-1) ?? list[0] ?? y;
        });
      })
      .catch(onError);
    return () => {
      live = false;
    };
  }, [backend, revision, onError]);

  useEffect(() => {
    if (years === null || years.length === 0) return;
    let live = true;
    backend
      .summaryYear(year, grouped)
      .then((s) => {
        if (live) setSummary(s);
      })
      .catch(onError);
    return () => {
      live = false;
    };
  }, [backend, years, year, grouped, revision, onError]);

  const table = useMemo(() => (summary === null ? null : summaryTable(summary)), [summary]);

  if (years !== null && years.length === 0) {
    return (
      <section className={styles.sheet} aria-labelledby="summary-heading">
        <header className={styles.bar}>
          <h2 id="summary-heading">Свод</h2>
        </header>
        <p className={styles.hint}>Записей в журнале пока нет — «Свод» соберётся из них сам.</p>
      </section>
    );
  }

  const months = summary?.footer.months ?? 0;
  return (
    <section className={styles.sheet} aria-labelledby="summary-heading">
      <header className={styles.bar}>
        <h2 id="summary-heading">Свод</h2>
        <div role="tablist" aria-label="Год" className={styles.years}>
          {(years ?? []).map((y) => (
            <button
              key={y}
              type="button"
              role="tab"
              aria-selected={y === year}
              onClick={() => {
                setYear(y);
              }}
            >
              {y}
            </button>
          ))}
        </div>
        <div className={styles.mode} role="radiogroup" aria-label="Колонки">
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
        {summary !== null && (
          <span className={styles.note}>
            Среднее — по {months} {plural(months, "месяцу", "месяцам", "месяцам")} с записями.
            Баланс — план дохода минус траты, общий — с кэшбэком.
          </span>
        )}
      </header>
      {table !== null && summary !== null && (
        <div className={styles.scroll}>
          <table className={styles.table} style={{ fontSize }}>
            <thead>
              <tr>
                <th scope="col" className={styles.label}>
                  {summary.year}
                </th>
                {table.head.map((h) => (
                  <th key={h.key} scope="col">
                    {h.title}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {table.months.map((line) => (
                <Line key={line.key} line={line} />
              ))}
            </tbody>
            <tfoot>
              {table.footer.map((line) => (
                <Line key={line.key} line={line} footer />
              ))}
            </tfoot>
          </table>
        </div>
      )}
    </section>
  );
}
