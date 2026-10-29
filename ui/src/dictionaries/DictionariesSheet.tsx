import { useMemo } from "react";

import type { DictKind, DictOp, Dictionaries } from "../api";
import { plural } from "../format/text";
import { TableGrid } from "../grid/TableGrid";
import styles from "./DictionariesSheet.module.css";
import { buildDictOps } from "./edits";
import { specs } from "./specs";

interface Props {
  dictionaries: Dictionaries;
  fontSize: number;
  kind: DictKind;
  onKindChange: (kind: DictKind) => void;
  onOps: (kind: DictKind, ops: DictOp[]) => Promise<boolean>;
  onErrors: (messages: string[]) => void;
  confirm: (message: string, title: string) => Promise<boolean>;
}

/** The «Справочники» sheet: the dictionaries side by side with one table open. */
export function DictionariesSheet({
  dictionaries,
  fontSize,
  kind,
  onKindChange,
  onOps,
  onErrors,
  confirm,
}: Props) {
  const all = useMemo(() => specs(dictionaries), [dictionaries]);
  const spec = all.find((s) => s.kind === kind) ?? all[0];
  if (spec === undefined) {
    return null;
  }
  const rows = spec.rows(dictionaries);

  const deleteRows = (ids: number[]) => {
    const n = ids.length;
    void (async () => {
      const yes = await confirm(
        `Удалить ${n.toString()} ${plural(n, "запись", "записи", "записей")} из «${spec.title}»?`,
        "Удаление",
      );
      if (yes) {
        await onOps(spec.kind, [{ op: "delete", ids }]);
      }
    })();
  };

  return (
    <div className={styles.sheet}>
      <nav className={styles.nav} aria-label="Справочники">
        {all.map((s) => (
          <button
            key={s.kind}
            type="button"
            aria-current={s.kind === spec.kind ? "page" : undefined}
            onClick={() => {
              onKindChange(s.kind);
            }}
          >
            <span>{s.title}</span>
            <span className={styles.count}>{s.rows(dictionaries).length}</span>
          </button>
        ))}
      </nav>
      <section className={styles.table} aria-label={spec.title}>
        <p className={styles.hint}>{spec.hint}</p>
        <div className={styles.grid}>
          <TableGrid
            key={spec.kind}
            columns={spec.columns}
            rows={rows}
            fontSize={fontSize}
            newRowHint="Новая запись"
            onChanges={(changes) => {
              const { ops, errors } = buildDictOps(changes, rows, spec.columns);
              onErrors(errors);
              if (ops.length > 0) void onOps(spec.kind, ops);
            }}
            onDeleteRows={deleteRows}
          />
        </div>
      </section>
    </div>
  );
}
