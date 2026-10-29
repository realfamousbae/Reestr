import { useState } from "react";

import {
  type Backend,
  type CardSetup,
  type Dictionaries,
  type ImportPreview,
  type ImportReport,
  type ProductKind,
  createdMessage,
  errorMessage,
} from "../api";
import { KIND_LABELS } from "../dictionaries/specs";
import { plural } from "../format/text";
import styles from "./ImportWizard.module.css";
import { initialSetups, mismatches, rowsToAdd } from "./setup";

interface Props {
  backend: Backend;
  dictionaries: Dictionaries;
  /** The import is done: refresh and show the given month. */
  onImported: (report: ImportReport) => void;
  onClose: () => void;
}

type Step =
  | { kind: "choose" }
  | { kind: "reading" }
  | { kind: "preview"; file: string; preview: ImportPreview; cards: CardSetup[] }
  | { kind: "importing" }
  | { kind: "done"; report: ImportReport };

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** The import of the old workbook (PLAN §9): choose, check, adjust cards, import. */
export function ImportWizard({ backend, dictionaries, onImported, onClose }: Props) {
  const [step, setStep] = useState<Step>({ kind: "choose" });
  const [error, setError] = useState<string | null>(null);

  const choose = () => {
    setError(null);
    void (async () => {
      try {
        const path = await backend.pickWorkbook();
        if (path === null) return;
        setStep({ kind: "reading" });
        const preview = await backend.importPreview(path);
        setStep({ kind: "preview", file: fileName(path), preview, cards: initialSetups(preview) });
      } catch (e) {
        setError(errorMessage(e));
        setStep({ kind: "choose" });
      }
    })();
  };

  const run = (cards: CardSetup[]) => {
    setError(null);
    setStep({ kind: "importing" });
    backend
      .importApply(cards)
      .then((result) => {
        setStep({ kind: "done", report: result.report });
        onImported(result.report);
      })
      .catch((e: unknown) => {
        setError(errorMessage(e));
        setStep({ kind: "choose" });
      });
  };

  const cancel = () => {
    void backend.importCancel();
    onClose();
  };

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <h1>Импорт из Excel</h1>
        <button type="button" className={styles.secondary} onClick={cancel}>
          Закрыть
        </button>
      </header>

      {error !== null && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}

      {step.kind === "choose" && (
        <section className={styles.section}>
          <p className={styles.lead}>
            Выберите прежнюю таблицу учёта затрат. Реестр прочитает лист «Служебный» и листы месяцев
            («07.26»), покажет, что найдено, и сверит итоги с формулами каждого листа. Лист «Свод»
            не нужен: он пересчитывается. В самой таблице ничего не меняется.
          </p>
          <button type="button" className={styles.primary} onClick={choose}>
            Выбрать файл Excel…
          </button>
        </section>
      )}

      {(step.kind === "reading" || step.kind === "importing") && (
        <p className={styles.lead} role="status">
          {step.kind === "reading" ? "Читаю таблицу…" : "Импортирую…"}
        </p>
      )}

      {step.kind === "preview" && (
        <Preview
          file={step.file}
          preview={step.preview}
          cards={step.cards}
          banks={dictionaries.banks.map((b) => b.name)}
          onCardsChange={(cards) => {
            setStep({ ...step, cards });
          }}
          onImport={() => {
            run(step.cards);
          }}
          onCancel={cancel}
        />
      )}

      {step.kind === "done" && (
        <section className={styles.section}>
          <p className={styles.lead}>
            Добавлено {step.report.added} {plural(step.report.added, "строка", "строки", "строк")}
            {step.report.skipped > 0 &&
              `, пропущено ${step.report.skipped.toString()} — они уже были в файле`}
            . Ctrl+Z отменит весь импорт.
          </p>
          {createdMessage(step.report.created) !== null && (
            <p className={styles.note}>{createdMessage(step.report.created)}</p>
          )}
          <button type="button" className={styles.primary} onClick={onClose}>
            К журналу
          </button>
        </section>
      )}
    </div>
  );
}

interface PreviewProps {
  file: string;
  preview: ImportPreview;
  cards: CardSetup[];
  banks: string[];
  onCardsChange: (cards: CardSetup[]) => void;
  onImport: () => void;
  onCancel: () => void;
}

function Preview({ file, preview, cards, banks, onCardsChange, onImport, onCancel }: PreviewProps) {
  const toAdd = rowsToAdd(preview);
  const months = preview.months.filter((m) => m.rows > 0);
  const empty = preview.months.length - months.length;
  const bankOptions = [
    ...new Set([
      ...banks,
      ...preview.cards.flatMap((c) => (c.guess.bank === null ? [] : [c.guess.bank])),
    ]),
  ];
  const participants = preview.participants.map((p) => p.name);
  const newNames = (items: { name: string; isNew: boolean }[]) =>
    items.filter((i) => i.isNew).map((i) => i.name);
  const update = (i: number, patch: Partial<CardSetup>) => {
    onCardsChange(cards.map((c, j) => (j === i ? { ...c, ...patch } : c)));
  };

  return (
    <>
      <section className={styles.section} aria-labelledby="months-h">
        <h2 id="months-h">Месяцы в «{file}»</h2>
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Лист</th>
              <th className={styles.num}>Строк</th>
              <th className={styles.num}>Уже в файле</th>
              <th>Сверка с формулами листа</th>
            </tr>
          </thead>
          <tbody>
            {months.map((m) => {
              const wrong = mismatches(m.check);
              return (
                <tr key={m.month}>
                  <td>{m.sheet}</td>
                  <td className={styles.num}>{m.rows}</td>
                  <td className={styles.num}>{m.already === 0 ? "" : m.already}</td>
                  <td>
                    {wrong.length === 0 ? (
                      <span className={styles.ok}>итоги совпадают до копейки</span>
                    ) : (
                      <ul className={styles.mismatch}>
                        {wrong.map((w) => (
                          <li key={w}>{w}</li>
                        ))}
                      </ul>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {empty > 0 && (
          <p className={styles.note}>Пустых листов: {empty}. Из них ничего не импортируется.</p>
        )}
      </section>

      <section className={styles.section} aria-labelledby="cards-h">
        <h2 id="cards-h">Карты</h2>
        <p className={styles.note}>
          Названия карт из колонки «Карта». Банк, тип и владельца можно поправить сейчас или позже в
          «Справочниках».
        </p>
        <datalist id="import-banks">
          {bankOptions.map((b) => (
            <option key={b} value={b} />
          ))}
        </datalist>
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Как в таблице</th>
              <th className={styles.num}>Строк</th>
              <th>Банк</th>
              <th>Тип</th>
              <th>Участник</th>
            </tr>
          </thead>
          <tbody>
            {preview.cards.map((card, i) => {
              const setup = cards[i];
              if (setup === undefined) return null;
              return (
                <tr key={card.shortName}>
                  <td>{card.shortName}</td>
                  <td className={styles.num}>{card.rows}</td>
                  {card.exists ? (
                    <td colSpan={3} className={styles.muted}>
                      уже в справочнике
                    </td>
                  ) : (
                    <>
                      <td>
                        <input
                          list="import-banks"
                          value={setup.bank ?? ""}
                          aria-label={`Банк карты ${card.shortName}`}
                          onChange={(e) => {
                            update(i, {
                              bank: e.target.value.trim() === "" ? null : e.target.value,
                            });
                          }}
                        />
                      </td>
                      <td>
                        <select
                          value={setup.kind ?? ""}
                          aria-label={`Тип карты ${card.shortName}`}
                          onChange={(e) => {
                            update(i, {
                              kind: e.target.value === "" ? null : (e.target.value as ProductKind),
                            });
                          }}
                        >
                          <option value="">не указан</option>
                          {Object.entries(KIND_LABELS).map(([code, label]) => (
                            <option key={code} value={code}>
                              {label}
                            </option>
                          ))}
                        </select>
                      </td>
                      <td>
                        <select
                          value={setup.participant ?? ""}
                          aria-label={`Участник карты ${card.shortName}`}
                          onChange={(e) => {
                            update(i, {
                              participant: e.target.value === "" ? null : e.target.value,
                            });
                          }}
                        >
                          <option value="">не указан</option>
                          {participants.map((p) => (
                            <option key={p} value={p}>
                              {p}
                            </option>
                          ))}
                        </select>
                      </td>
                    </>
                  )}
                </tr>
              );
            })}
          </tbody>
        </table>
      </section>

      <section className={styles.section} aria-labelledby="dict-h">
        <h2 id="dict-h">Добавится в справочники</h2>
        <NewNames title="Категории" names={newNames(preview.categories)} />
        <NewNames title="Группы" names={newNames(preview.groups)} />
        <NewNames title="Участники" names={newNames(preview.participants)} />
        <NewNames
          title="Карты"
          names={preview.cards.filter((c) => !c.exists).map((c) => c.shortName)}
        />
      </section>

      {preview.warnings.length > 0 && (
        <section className={styles.section} aria-labelledby="warn-h">
          <h2 id="warn-h">Замечания ({preview.warnings.length})</h2>
          <ul className={styles.warnings}>
            {preview.warnings.slice(0, 50).map((w) => (
              <li key={`${w.sheet}-${w.row.toString()}-${w.message}`}>
                Лист {w.sheet}, строка {w.row}: {w.message}
              </li>
            ))}
          </ul>
        </section>
      )}

      <footer className={styles.actions}>
        <button type="button" className={styles.primary} onClick={onImport} disabled={toAdd === 0}>
          {toAdd === 0
            ? "Всё уже импортировано"
            : `Импортировать ${toAdd.toString()} ${plural(toAdd, "строку", "строки", "строк")}`}
        </button>
        <button type="button" className={styles.secondary} onClick={onCancel}>
          Отмена
        </button>
      </footer>
    </>
  );
}

function NewNames({ title, names }: { title: string; names: string[] }) {
  return (
    <p className={styles.names}>
      <span className={styles.namesTitle}>{title}:</span>{" "}
      {names.length === 0 ? <span className={styles.muted}>ничего нового</span> : names.join(", ")}
    </p>
  );
}
