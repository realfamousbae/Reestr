import type { DataEditorRef } from "@glideapps/glide-data-grid";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  type Backend,
  type ChangeResult,
  type DictKind,
  type DictOp,
  type ExportFormat,
  type Dictionaries,
  type FileInfo,
  type InflowOp,
  type JournalOp,
  type Kopecks,
  type MonthTab,
  type MonthView,
  type ProductOp,
  type ProductsView,
  type DeadlineView,
  type RuleOp,
  actionName,
  createdMessage,
  errorMessage,
} from "../api";
import { ExportMenu } from "../components/ExportMenu";
import { RegularPanel } from "../components/RegularPanel";
import { type Sheet, SheetTabs } from "../components/SheetTabs";
import {
  CashbackBlock,
  CreditBlock,
  DeadlinesBlock,
  IncomeBlock,
  LimitsBlock,
  SavingsBlock,
} from "../components/SideBlocks";
import { formatDate, todayIso } from "../format/date";
import { TotalsBar } from "../components/TotalsBar";
import { DictionariesSheet } from "../dictionaries/DictionariesSheet";
import { ImportWizard } from "../import/ImportWizard";
import { InflowsSheet } from "../income/InflowsSheet";
import { plural } from "../format/text";
import { JournalGrid } from "../grid/JournalGrid";
import type { CellContext } from "../grid/columns";
import { type CellChange, buildBatch } from "../grid/edits";
import { EXPORT_WARNING, baseName, exportName } from "../export/exportFile";
import { DeadlinesSheet } from "../products/DeadlinesSheet";
import { daysText, deadlineText } from "../products/labels";
import { ProductsSheet } from "../products/ProductsSheet";
import { RulesSheet } from "../rules/RulesSheet";
import { setAlwaysKopecks } from "../format/money";
import { SettingsPage } from "../settings/SettingsPage";
import { type ViewSettings, readView, shownBlocks, writeView } from "../settings/view";
import { SummarySheet } from "../summary/SummarySheet";
import styles from "./WorkbookScreen.module.css";

interface Props {
  backend: Backend;
  file: FileInfo;
  onClose: () => void;
}

interface Status {
  text: string;
  kind: "info" | "error";
}

const GROUPED_KEY = "reestr.totalsByGroup";

function currentMonth(): string {
  const now = new Date();
  return `${now.getFullYear().toString()}-${(now.getMonth() + 1).toString().padStart(2, "0")}`;
}

function nextMonth(month: string): string {
  const [y = 0, m = 1] = month.split("-").map(Number);
  return m === 12
    ? `${(y + 1).toString()}-01`
    : `${y.toString()}-${(m + 1).toString().padStart(2, "0")}`;
}

function readFlag(key: string): boolean {
  try {
    return localStorage.getItem(key) === "1";
  } catch {
    return false;
  }
}

function writeFlag(key: string, value: boolean): void {
  try {
    localStorage.setItem(key, value ? "1" : "0");
  } catch {
    // A view preference; losing it is harmless.
  }
}

/** True when keys go to a text field (the cell editor, a form), which has its own undo. */
function typingInField(): boolean {
  const el = document.activeElement;
  return el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement;
}

/** The open file as a workbook: month sheets and «Справочники». */
export function WorkbookScreen({ backend, file, onClose }: Props) {
  const [dictionaries, setDictionaries] = useState<Dictionaries | null>(null);
  const [titles, setTitles] = useState<string[]>([]);
  const [tabs, setTabs] = useState<MonthTab[]>([]);
  const [sheet, setSheet] = useState<Sheet>({ kind: "month", month: currentMonth() });
  const [dictKind, setDictKind] = useState<DictKind>("category");
  const [view, setView] = useState<MonthView | null>(null);
  // A copy of the day that could not be made is said once, at the bottom (D32).
  const [status, setStatus] = useState<Status | null>(() =>
    file.copyFailed
      ? {
          text:
            "Не удалось сделать копию файла за сегодня: проверьте, что папка рядом с файлом " +
            "доступна для записи. Файл открыт, работать можно.",
          kind: "error",
        }
      : null,
  );
  const [grouped, setGrouped] = useState(() => readFlag(GROUPED_KEY));
  const [importing, setImporting] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  /** The view of this computer (D34). */
  const [prefs, setPrefs] = useState<ViewSettings>(readView);
  // The sheets are unmounted while the settings are open (`overlay`), so they redraw with the
  // new mode when they come back. Setting a module flag during render is idempotent.
  setAlwaysKopecks(prefs.alwaysKopecks);
  /** The import or the settings cover the sheets. */
  const overlay = importing || settingsOpen;
  /** A month sheet shows its journal, its cashback rules or its inflows. */
  const [pane, setPane] = useState<"journal" | "rules" | "inflows">("journal");
  /** Bumped when undo or redo changes the data under the open «Свод». */
  const [revision, setRevision] = useState(0);
  const [products, setProducts] = useState<ProductsView | null>(null);
  /** Deadlines due within a week or overdue, shown once when the file opens (PLAN §7). */
  const [notice, setNotice] = useState<DeadlineView[] | null>(null);
  const noticeShown = useRef(false);
  const grid = useRef<DataEditorRef>(null);
  const month = sheet.kind === "month" ? sheet.month : (view?.month ?? currentMonth());

  const report = useCallback((error: unknown) => {
    setStatus({ text: errorMessage(error), kind: "error" });
  }, []);

  const refreshDictionaries = useCallback(() => {
    backend.dictionaries().then(setDictionaries).catch(report);
  }, [backend, report]);

  const refreshTitles = useCallback(() => {
    backend.journalTitles().then(setTitles).catch(report);
  }, [backend, report]);

  const refreshTabs = useCallback(() => {
    backend.journalMonths(currentMonth()).then(setTabs).catch(report);
  }, [backend, report]);

  const refreshProducts = useCallback(() => {
    backend
      .productsView(todayIso())
      .then((p) => {
        setProducts(p);
        if (!noticeShown.current) {
          noticeShown.current = true;
          const urgent = p.deadlines.filter((d) => d.urgency !== "later");
          if (urgent.length > 0) setNotice(urgent);
        }
      })
      .catch(report);
  }, [backend, report]);

  useEffect(() => {
    refreshDictionaries();
    refreshTitles();
    refreshTabs();
    refreshProducts();
  }, [refreshDictionaries, refreshTitles, refreshTabs, refreshProducts]);

  useEffect(() => {
    if (sheet.kind !== "month") return;
    let live = true;
    backend
      .journalMonth(sheet.month)
      .then((v) => {
        if (live) setView(v);
      })
      .catch(report);
    return () => {
      live = false;
    };
  }, [backend, sheet, report]);

  const names = useCallback(
    (pick: (d: Dictionaries) => { name: string; active: boolean }[]) =>
      dictionaries === null
        ? []
        : pick(dictionaries)
            .filter((i) => i.active)
            .map((i) => i.name),
    [dictionaries],
  );

  const ctx = useMemo<CellContext>(
    () => ({
      month,
      categories: names((d) => d.categories.map((c) => ({ name: c.name, active: c.isActive }))),
      participants: names((d) => d.participants.map((p) => ({ name: p.name, active: p.isActive }))),
      cards: names((d) => d.products.map((p) => ({ name: p.shortName, active: p.isActive }))),
    }),
    [month, names],
  );

  /** Shows the result of a change to the journal. */
  const changed = useCallback(
    (result: ChangeResult, info?: string) => {
      setView(result.view);
      const created = createdMessage(result.created);
      if (created !== null) {
        refreshDictionaries();
      }
      const text = created ?? info;
      setStatus(text === undefined ? null : { text, kind: "info" });
      refreshTitles();
      refreshTabs();
      // Purchases move credit card debts.
      refreshProducts();
    },
    [refreshDictionaries, refreshTitles, refreshTabs, refreshProducts],
  );

  const apply = useCallback(
    async (ops: JournalOp[]) => {
      try {
        changed(await backend.journalApply(month, ops));
      } catch (error) {
        report(error);
      }
    },
    [backend, month, changed, report],
  );

  const onChanges = useCallback(
    (cells: CellChange[]) => {
      if (view === null) return;
      const { ops, errors } = buildBatch(cells, view.entries, ctx);
      if (errors.length > 0) setStatus({ text: errors.join(". "), kind: "error" });
      if (ops.length > 0) void apply(ops);
    },
    [view, ctx, apply],
  );

  const onDeleteRows = useCallback(
    (ids: number[]) => {
      const n = ids.length;
      const rows = `${n.toString()} ${plural(n, "строку", "строки", "строк")}`;
      void (async () => {
        if (!(await backend.confirm(`Удалить ${rows}?`, "Удаление строк"))) return;
        try {
          changed(await backend.journalDelete(month, ids), `Удалено: ${rows}. Ctrl+Z вернёт.`);
        } catch (error) {
          report(error);
        }
      })();
    },
    [backend, month, changed, report],
  );

  const onAddTemplates = useCallback(
    (ids: number[]) => {
      backend
        .journalAddTemplates(month, ids)
        .then((r) => {
          changed(r);
        })
        .catch(report);
    },
    [backend, month, changed, report],
  );

  const onDictOps = useCallback(
    async (kind: DictKind, ops: DictOp[]) => {
      try {
        const result = await backend.dictEdit(kind, ops);
        setDictionaries(result.dictionaries);
        refreshProducts();
        const created = createdMessage(result.created);
        setStatus(created === null ? null : { text: created, kind: "info" });
        return true;
      } catch (error) {
        report(error);
        return false;
      }
    },
    [backend, refreshProducts, report],
  );

  /**
   * Rule changes that move the cashback of journal rows ask first (PLAN §6.3): recompute the
   * rows, or keep their cashback as it is.
   */
  const onRuleOps = useCallback(
    (ops: RuleOp[]) => {
      void (async () => {
        try {
          const affected = await backend.rulesImpact(month, ops);
          const rows = `${affected.toString()} ${plural(affected, "строке", "строках", "строках")}`;
          let recompute = true;
          if (affected > 0) {
            recompute = await backend.confirm(
              `Правила меняют кэшбэк в ${rows} журнала. Пересчитать по новым правилам? ` +
                "«Оставить как есть» сохранит кэшбэк этих строк, как сейчас.",
              "Правила кэшбэка",
              { ok: "Пересчитать", cancel: "Оставить как есть" },
            );
          }
          const result = await backend.rulesApply(month, ops, recompute);
          const info =
            affected === 0
              ? undefined
              : recompute
                ? `Кэшбэк пересчитан в ${rows}. Ctrl+Z отменит.`
                : `Кэшбэк в ${rows} оставлен, как был.`;
          changed(result, info);
        } catch (error) {
          report(error);
        }
      })();
    },
    [backend, month, changed, report],
  );

  const onDeleteRules = useCallback(
    (ids: number[]) => {
      const n = ids.length;
      void (async () => {
        const rules = `${n.toString()} ${plural(n, "правило", "правила", "правил")}`;
        if (await backend.confirm(`Удалить ${rules}?`, "Правила кэшбэка")) {
          onRuleOps([{ op: "delete", ids }]);
        }
      })();
    },
    [backend, onRuleOps],
  );

  const onSuggestionsChange = useCallback(
    (enabled: boolean) => {
      backend
        .setCardSuggestions(enabled)
        .then(() => backend.journalMonth(month))
        .then(setView)
        .catch(report);
    },
    [backend, month, report],
  );

  const suggest = useMemo(
    () =>
      view?.cardSuggestions === true
        ? (entry: number) => backend.cardSuggestions(month, entry)
        : null,
    [backend, month, view?.cardSuggestions],
  );

  const onProductOps = useCallback(
    (ops: ProductOp[]) => {
      backend
        .productsApply(ops, todayIso())
        .then((result) => {
          setProducts(result.view);
          const created = createdMessage(result.created);
          if (created !== null) refreshDictionaries();
          setStatus(created === null ? null : { text: created, kind: "info" });
        })
        .catch(report);
    },
    [backend, refreshDictionaries, report],
  );

  const onProductDelete = useCallback(
    (what: string, ops: ProductOp[]) => {
      void (async () => {
        if (await backend.confirm(`Удалить ${what}?`, "Удаление")) onProductOps(ops);
      })();
    },
    [backend, onProductOps],
  );

  const onInflowOps = useCallback(
    (ops: InflowOp[]) => {
      backend
        .inflowsApply(month, ops)
        .then((r) => {
          changed(r);
        })
        .catch(report);
    },
    [backend, month, changed, report],
  );

  const onDeleteInflows = useCallback(
    (ids: number[]) => {
      const n = ids.length;
      const what = `${n.toString()} ${plural(n, "поступление", "поступления", "поступлений")}`;
      void (async () => {
        if (!(await backend.confirm(`Удалить ${what}?`, "Поступления"))) return;
        try {
          changed(
            await backend.inflowsApply(month, [{ op: "delete", ids }]),
            `Удалено: ${what}. Ctrl+Z вернёт.`,
          );
        } catch (error) {
          report(error);
        }
      })();
    },
    [backend, month, changed, report],
  );

  const onPlan = useCallback(
    (amount: Kopecks | null) => {
      backend
        .planSet(month, amount)
        .then((r) => {
          changed(r);
        })
        .catch(report);
    },
    [backend, month, changed, report],
  );

  const onDictErrors = useCallback((messages: string[]) => {
    setStatus(messages.length === 0 ? null : { text: messages.join(". "), kind: "error" });
  }, []);

  const replay = useCallback(
    (direction: "undo" | "redo") => {
      const run = direction === "undo" ? backend.historyUndo(month) : backend.historyRedo(month);
      run
        .then((r) => {
          if (r.label === null) {
            setStatus({
              text: direction === "undo" ? "Отменять нечего." : "Возвращать нечего.",
              kind: "info",
            });
            return;
          }
          setView(r.view);
          if (sheet.kind === "month" && r.view.month !== sheet.month) {
            setSheet({ kind: "month", month: r.view.month });
          }
          refreshDictionaries();
          refreshTitles();
          refreshTabs();
          refreshProducts();
          setRevision((n) => n + 1);
          const verb = direction === "undo" ? "Отменено" : "Возвращено";
          setStatus({ text: `${verb}: ${actionName(r.label)}.`, kind: "info" });
        })
        .catch(report);
    },
    [
      backend,
      month,
      sheet,
      refreshDictionaries,
      refreshTitles,
      refreshTabs,
      refreshProducts,
      report,
    ],
  );

  const addMonth = useCallback(() => {
    const last = tabs.at(-1)?.month ?? month;
    const next = nextMonth(last);
    backend
      .journalAddMonth(next, currentMonth())
      .then((list) => {
        setTabs(list);
        setSheet({ kind: "month", month: next });
      })
      .catch(report);
  }, [backend, tabs, month, report]);

  // Shortcuts by physical key (e.code), so they work in the Russian layout too.
  // Ctrl+Z / Ctrl+Y (Ctrl+Shift+Z) — undo/redo; Ctrl+N — new row;
  // Ctrl+Tab / Ctrl+PageDown — next sheet, with Shift / PageUp — previous.
  useEffect(() => {
    const sheets: Sheet[] = [
      ...tabs.map((t): Sheet => ({ kind: "month", month: t.month })),
      { kind: "summary" },
      { kind: "products" },
      { kind: "deadlines" },
      { kind: "dictionaries" },
    ];
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      if ((e.code === "KeyZ" || e.code === "KeyY") && !typingInField()) {
        e.preventDefault();
        replay(e.code === "KeyY" || e.shiftKey ? "redo" : "undo");
        return;
      }
      if (e.code === "KeyN" && sheet.kind === "month" && pane === "journal") {
        e.preventDefault();
        void grid.current?.appendRow(0);
        return;
      }
      const step =
        (e.code === "Tab" && !e.shiftKey) || e.code === "PageDown"
          ? 1
          : e.code === "Tab" || e.code === "PageUp"
            ? -1
            : 0;
      if (step !== 0 && sheets.length > 0) {
        e.preventDefault();
        const i = sheets.findIndex((s) =>
          s.kind === "month"
            ? sheet.kind === "month" && s.month === sheet.month
            : sheet.kind === s.kind,
        );
        const target = sheets[(i + step + sheets.length) % sheets.length];
        if (target !== undefined) setSheet(target);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [tabs, sheet, pane, replay]);

  /** Warns that the export is not encrypted, asks where to save it, writes it (PLAN §10). */
  const onExport = useCallback(
    (format: ExportFormat) => {
      void (async () => {
        const go = await backend.confirm(EXPORT_WARNING, "Экспорт", {
          ok: "Выбрать, куда сохранить…",
          cancel: "Отмена",
        });
        if (!go) return;
        const today = todayIso();
        const path = await backend.pickExportFile(format, exportName(file.fileName, format, today));
        if (path === null) return;
        setStatus({ text: "Собираю выгрузку…", kind: "info" });
        try {
          await backend.exportFile(path, format, today);
          setStatus({ text: `Выгружено: ${baseName(path)}. Файл не зашифрован.`, kind: "info" });
        } catch (error) {
          report(error);
        }
      })();
    },
    [backend, file.fileName, report],
  );

  const close = useCallback(() => {
    backend.vaultClose().then(onClose).catch(report);
  }, [backend, onClose, report]);

  return (
    <div className={styles.screen}>
      <header className={styles.top}>
        <span className={styles.brand}>Реестр</span>
        <span className={styles.file} title={file.path}>
          {file.fileName}
        </span>
        <button
          type="button"
          className={`${styles.close} ${styles.importButton}`}
          onClick={() => {
            setImporting(true);
          }}
          disabled={overlay}
        >
          Импорт из Excel…
        </button>
        <ExportMenu disabled={overlay} onExport={onExport} />
        <button
          type="button"
          className={styles.close}
          onClick={() => {
            setSettingsOpen(true);
          }}
          disabled={overlay}
        >
          Настройки
        </button>
        <button type="button" className={styles.close} onClick={close}>
          Закрыть файл
        </button>
      </header>

      <div className={styles.notices}>
        {notice !== null && (
          <div className={styles.notice} role="status">
            <span className={styles.noticeTitle}>Сроки:</span>
            <ul>
              {notice.slice(0, 3).map((d) => (
                <li key={`${d.source.kind}-${d.date}-${deadlineText(d)}`}>
                  {deadlineText(d)} — {formatDate(d.date)}, {daysText(d.daysLeft)}
                </li>
              ))}
              {notice.length > 3 && <li>и ещё {(notice.length - 3).toString()}</li>}
            </ul>
            <button
              type="button"
              className={styles.close}
              onClick={() => {
                setNotice(null);
                setSheet({ kind: "deadlines" });
              }}
            >
              Все сроки
            </button>
            <button
              type="button"
              className={styles.close}
              onClick={() => {
                setNotice(null);
              }}
            >
              Скрыть
            </button>
          </div>
        )}
      </div>

      {!overlay && sheet.kind === "month" && view !== null && (
        <div className={styles.totals}>
          <TotalsBar
            totals={view.totals}
            grouped={grouped}
            onGroupedChange={(g) => {
              setGrouped(g);
              writeFlag(GROUPED_KEY, g);
            }}
          />
        </div>
      )}

      <main className={styles.content}>
        {settingsOpen && (
          <SettingsPage
            backend={backend}
            view={prefs}
            onViewChange={(next) => {
              setPrefs(next);
              writeView(next);
            }}
            onClose={() => {
              setSettingsOpen(false);
            }}
          />
        )}
        {importing && dictionaries !== null && (
          <ImportWizard
            backend={backend}
            dictionaries={dictionaries}
            onImported={(report) => {
              refreshDictionaries();
              refreshTitles();
              refreshTabs();
              const last = report.months.at(-1);
              if (last !== undefined) {
                setSheet({ kind: "month", month: last });
              }
            }}
            onClose={() => {
              setImporting(false);
            }}
          />
        )}
        {!overlay && sheet.kind === "month" && view !== null && dictionaries !== null && (
          <div className={styles.monthSheet}>
            <div className={styles.grid}>
              {pane === "journal" && view.entries.length === 0 && (
                <p className={styles.emptyHint}>
                  В месяце {view.title} записей пока нет. Новая строка — Ctrl+N или «Новая строка»
                  внизу таблицы; регулярные платежи вносятся из блока справа.
                </p>
              )}
              {pane === "journal" ? (
                <JournalGrid
                  ref={grid}
                  entries={view.entries}
                  titles={titles}
                  categories={ctx.categories}
                  suggest={suggest}
                  fontSize={prefs.fontSize}
                  onChanges={onChanges}
                  onDeleteRows={onDeleteRows}
                />
              ) : pane === "inflows" ? (
                <InflowsSheet
                  view={view}
                  participants={ctx.participants}
                  cards={ctx.cards}
                  fontSize={prefs.fontSize}
                  onOps={onInflowOps}
                  onErrors={onDictErrors}
                  onDeleteRows={onDeleteInflows}
                  onBack={() => {
                    setPane("journal");
                  }}
                />
              ) : (
                <RulesSheet
                  view={view}
                  cards={ctx.cards}
                  categories={ctx.categories}
                  fontSize={prefs.fontSize}
                  onOps={onRuleOps}
                  onErrors={onDictErrors}
                  onDeleteRows={onDeleteRules}
                  onSuggestionsChange={onSuggestionsChange}
                  onBack={() => {
                    setPane("journal");
                  }}
                />
              )}
            </div>
            <aside className={styles.side} aria-label="Блоки месяца">
              {shownBlocks(prefs).map((id) => {
                const openProducts = () => {
                  setSheet({ kind: "products" });
                };
                switch (id) {
                  case "cashback":
                    return <CashbackBlock key={id} totals={view.totals} />;
                  case "regular":
                    return (
                      <RegularPanel
                        key={id}
                        dictionaries={dictionaries}
                        entries={view.entries}
                        onAdd={onAddTemplates}
                        onEditTemplates={() => {
                          setDictKind("template");
                          setSheet({ kind: "dictionaries" });
                        }}
                      />
                    );
                  case "limits":
                    return (
                      <LimitsBlock
                        key={id}
                        view={view}
                        onOpenRules={() => {
                          setPane("rules");
                        }}
                        onCopyRules={(from) => {
                          onRuleOps([{ op: "copyFrom", from }]);
                        }}
                      />
                    );
                  case "credit":
                    return (
                      products !== null && (
                        <CreditBlock key={id} products={products} onOpen={openProducts} />
                      )
                    );
                  case "savings":
                    return (
                      products !== null && (
                        <SavingsBlock key={id} products={products} onOpen={openProducts} />
                      )
                    );
                  case "income":
                    return (
                      <IncomeBlock
                        key={id}
                        view={view}
                        onPlan={onPlan}
                        onError={(message) => {
                          setStatus({ text: message, kind: "error" });
                        }}
                        onOpenInflows={() => {
                          setPane("inflows");
                        }}
                      />
                    );
                  case "deadlines":
                    return (
                      products !== null && (
                        <DeadlinesBlock
                          key={id}
                          products={products}
                          onOpen={() => {
                            setSheet({ kind: "deadlines" });
                          }}
                        />
                      )
                    );
                }
              })}
            </aside>
          </div>
        )}
        {!overlay && sheet.kind === "summary" && (
          <SummarySheet
            backend={backend}
            initialYear={Number(month.slice(0, 4))}
            grouped={grouped}
            onGroupedChange={(g) => {
              setGrouped(g);
              writeFlag(GROUPED_KEY, g);
            }}
            revision={revision}
            fontSize={prefs.fontSize}
            onError={report}
          />
        )}
        {!overlay && sheet.kind === "products" && products !== null && (
          <ProductsSheet
            view={products}
            month={todayIso().slice(0, 7)}
            fontSize={prefs.fontSize}
            onOps={onProductOps}
            onErrors={onDictErrors}
            onDelete={onProductDelete}
            onOpenDictionaries={() => {
              setDictKind("product");
              setSheet({ kind: "dictionaries" });
            }}
          />
        )}
        {!overlay && sheet.kind === "deadlines" && products !== null && (
          <DeadlinesSheet
            view={products}
            cards={ctx.cards}
            month={todayIso().slice(0, 7)}
            fontSize={prefs.fontSize}
            onOps={onProductOps}
            onErrors={onDictErrors}
            onDeleteReminders={(ids) => {
              const n = ids.length;
              onProductDelete(
                `${n.toString()} ${plural(n, "напоминание", "напоминания", "напоминаний")}`,
                [{ op: "deleteDeadlines", ids }],
              );
            }}
          />
        )}
        {!overlay && sheet.kind === "dictionaries" && dictionaries !== null && (
          <DictionariesSheet
            dictionaries={dictionaries}
            fontSize={prefs.fontSize}
            kind={dictKind}
            onKindChange={setDictKind}
            onOps={onDictOps}
            onErrors={onDictErrors}
            confirm={(message, title) => backend.confirm(message, title)}
          />
        )}
      </main>

      <footer className={styles.bottom}>
        <SheetTabs
          tabs={tabs}
          active={sheet}
          urgent={products?.deadlines.filter((d) => d.urgency !== "later").length ?? 0}
          onSelect={(s) => {
            setStatus(null);
            setSheet(s);
          }}
          onAddMonth={addMonth}
        />
        <p
          className={`${styles.status} ${status?.kind === "error" ? styles.error : ""}`}
          role={status?.kind === "error" ? "alert" : "status"}
        >
          {status?.text ?? ""}
        </p>
      </footer>
    </div>
  );
}
