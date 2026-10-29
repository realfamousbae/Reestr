import { type FormEvent, useCallback, useEffect, useMemo, useState } from "react";

import {
  type Backend,
  type Dictionaries,
  type EntryView,
  type Month,
  type PhoneMonth,
  type PhoneOpened,
  type SuggestionView,
  errorMessage,
} from "../api";
import { formatDate, todayIso } from "../format/date";
import { formatMoney } from "../format/money";
import { parseMoney } from "../format/money";
import { formatPercent } from "../format/percent";
import { nameKey, plural } from "../format/text";
import {
  type DraftForm,
  appendEdits,
  changedEdits,
  emptyForm,
  fillFromLast,
  formOf,
  monthOf,
  readDraft,
  titleMatches,
} from "./draft";
import styles from "./Phone.module.css";

interface Props {
  backend: Backend;
  opened: PhoneOpened;
  onLock: () => void;
}

/** "2026-10" → "10.26", the name of the month sheet. */
function sheetName(month: Month): string {
  return `${month.slice(5, 7)}.${month.slice(2, 4)}`;
}

function shiftMonth(month: Month, by: number): Month {
  const [y, m] = month.split("-").map(Number);
  const index = (y ?? 0) * 12 + (m ?? 1) - 1 + by;
  return `${Math.floor(index / 12).toString()}-${((index % 12) + 1).toString().padStart(2, "0")}`;
}

/** What the opening did, in words; null when there is nothing to say. */
function openedText(opened: PhoneOpened): string | null {
  if (opened.offline) {
    return "Копия с компьютера не прочиталась: правила и лимиты — с прошлого раза.";
  }
  const start = opened.start;
  if (start === null) return null;
  const parts = ["Получена новая копия с компьютера."];
  if (start.carried > 0) {
    parts.push(
      `${start.carried.toString()} ${plural(start.carried, "ваша строка ждёт", "ваши строки ждут", "ваших строк ждут")} компьютера.`,
    );
  }
  if (start.kept > 0) {
    parts.push(
      `${start.kept.toString()} ${plural(start.kept, "строку", "строки", "строк")} меняли и на компьютере — оставлено, как там.`,
    );
  }
  return parts.join(" ");
}

/** The phone's one screen (D40): a new row with the card hint, and the rows of the month. */
export function PhoneHome({ backend, opened, onLock }: Props) {
  const today = todayIso();
  const [month, setMonth] = useState<Month>(today.slice(0, 7));
  const [data, setData] = useState<PhoneMonth | null>(null);
  const [dictionaries, setDictionaries] = useState<Dictionaries | null>(null);
  const [history, setHistory] = useState<string[]>([]);
  const [form, setForm] = useState<DraftForm>(() => emptyForm(today));
  const [editing, setEditing] = useState<EntryView | null>(null);
  const [titleFocus, setTitleFocus] = useState(false);
  const [hints, setHints] = useState<SuggestionView[]>([]);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ text: string; ok: boolean } | null>(null);
  const [sent, setSent] = useState(opened.sent);
  const notice = openedText(opened);

  const report = useCallback((e: unknown) => {
    setMessage({ text: errorMessage(e), ok: false });
  }, []);

  const loadMonth = useCallback(
    (m: Month) => {
      backend.phoneMonth(m).then(setData).catch(report);
    },
    [backend, report],
  );

  useEffect(() => {
    backend.dictionaries().then(setDictionaries).catch(report);
    backend.journalTitles().then(setHistory).catch(report);
  }, [backend, report]);

  useEffect(() => {
    loadMonth(month);
  }, [month, loadMonth]);

  // «Какой картой платить»: for the category and amount typed so far.
  useEffect(() => {
    const amount = parseMoney(form.amount);
    const target = monthOf(form.date === "" ? null : form.date, month);
    const timer = setTimeout(() => {
      backend
        .phoneSuggest(
          target,
          form.category === "" ? null : form.category,
          amount.ok && amount.value > 0 ? amount.value : null,
        )
        .then(setHints)
        .catch(() => {
          setHints([]);
        });
    }, 250);
    return () => {
      clearTimeout(timer);
    };
  }, [backend, form.amount, form.category, form.date, month]);

  const set = <K extends keyof DraftForm>(key: K, value: DraftForm[K]) => {
    setForm((f) => ({ ...f, [key]: value }));
  };

  /** A title taken from the history fills the empty fields like in the journal. */
  const pickTitle = (title: string) => {
    setForm((f) => ({ ...f, title }));
    setTitleFocus(false);
    backend
      .phoneLast(title)
      .then((last) => {
        if (last !== null) setForm((f) => fillFromLast(f, last));
      })
      .catch(() => {
        // The fields stay as typed.
      });
  };

  const send = useCallback(() => {
    backend
      .phoneSend()
      .then(setSent)
      .catch(() => {
        setSent(false);
      });
  }, [backend]);

  const done = (text: string, target: Month) => {
    setMessage({ text, ok: true });
    setEditing(null);
    setForm((f) => ({ ...emptyForm(today), participant: f.participant }));
    backend.journalTitles().then(setHistory).catch(report);
    if (target === month) loadMonth(month);
    else setMonth(target);
    send();
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const draft = readDraft(form);
    if (!draft.ok) {
      setMessage({ text: draft.message, ok: false });
      return;
    }
    const d = draft.value;
    setBusy(true);
    setMessage(null);
    const run = async () => {
      if (editing !== null) {
        const edits = changedEdits(d, editing);
        if (edits.length > 0) {
          await backend.journalApply(
            month,
            edits.map((edit) => ({ op: "edit" as const, id: editing.id, edit })),
          );
        }
        done(`Сохранено: ${d.title}, ${formatMoney(d.amount)} ₽.`, month);
        return;
      }
      const target = monthOf(d.date, month);
      await backend.journalApply(target, [{ op: "append", edits: appendEdits(d) }]);
      done(`Внесено: ${d.title}, ${formatMoney(d.amount)} ₽.`, target);
    };
    run()
      .catch(report)
      .finally(() => {
        setBusy(false);
      });
  };

  const remove = () => {
    if (editing === null) return;
    const row = editing;
    void (async () => {
      if (!(await backend.confirm(`Удалить «${row.title}»?`, "Удаление строки"))) return;
      try {
        await backend.journalDelete(month, [row.id]);
        done(`Удалено: ${row.title}.`, month);
      } catch (err) {
        report(err);
      }
    })();
  };

  const names = (pick: (d: Dictionaries) => { name: string; active: boolean }[]) =>
    dictionaries === null
      ? []
      : pick(dictionaries)
          .filter((i) => i.active)
          .map((i) => i.name);
  const categories = names((d) => d.categories.map((c) => ({ name: c.name, active: c.isActive })));
  const cards = names((d) => d.products.map((p) => ({ name: p.shortName, active: p.isActive })));
  const participants = names((d) =>
    d.participants.map((p) => ({ name: p.name, active: p.isActive })),
  );

  const matches = useMemo(
    () => (titleFocus ? titleMatches(form.title, history) : []),
    [titleFocus, form.title, history],
  );

  const own = new Set(data?.own ?? []);
  const rows = [...(data?.view.entries ?? [])].reverse();

  return (
    <div className={styles.screen}>
      <header className={styles.bar}>
        <span className={styles.brand}>Реестр</span>
        <div className={styles.months}>
          <button
            type="button"
            className={styles.icon}
            aria-label="Прошлый месяц"
            onClick={() => {
              setMonth((m) => shiftMonth(m, -1));
            }}
          >
            ‹
          </button>
          <span>{sheetName(month)}</span>
          <button
            type="button"
            className={styles.icon}
            aria-label="Следующий месяц"
            onClick={() => {
              setMonth((m) => shiftMonth(m, 1));
            }}
          >
            ›
          </button>
        </div>
        <button type="button" className={styles.plain} onClick={onLock}>
          Закрыть
        </button>
      </header>

      {notice !== null && <p className={styles.notice}>{notice}</p>}
      {!sent && (
        <p className={`${styles.notice} ${styles.problem}`} role="status">
          Не отправлено на Яндекс Диск: строки сохранены на телефоне.
          <button type="button" className={styles.secondary} onClick={send}>
            Отправить
          </button>
        </p>
      )}

      <form className={styles.section} onSubmit={submit} noValidate>
        <h2>{editing === null ? "Новая строка" : "Правка строки"}</h2>
        <label className={styles.field}>
          <span>Наименование</span>
          <input
            value={form.title}
            autoComplete="off"
            enterKeyHint="next"
            onChange={(e) => {
              set("title", e.target.value);
              setTitleFocus(true);
            }}
            onFocus={() => {
              setTitleFocus(true);
            }}
            onBlur={() => {
              // A tap on a suggestion lands after the blur.
              setTimeout(() => {
                setTitleFocus(false);
              }, 150);
            }}
          />
        </label>
        {matches.length > 0 && (
          <ul className={styles.titles} aria-label="Из прошлых строк">
            {matches.map((t) => (
              <li key={t}>
                <button
                  type="button"
                  onMouseDown={(e) => {
                    e.preventDefault();
                  }}
                  onClick={() => {
                    pickTitle(t);
                  }}
                >
                  {t}
                </button>
              </li>
            ))}
          </ul>
        )}
        <label className={`${styles.field} ${styles.amount}`}>
          <span>Сумма, ₽</span>
          <input
            value={form.amount}
            inputMode="decimal"
            autoComplete="off"
            enterKeyHint="done"
            placeholder="0"
            onChange={(e) => {
              set("amount", e.target.value);
            }}
          />
        </label>
        <label className={styles.field}>
          <span>Назначение</span>
          <select
            value={form.category}
            onChange={(e) => {
              set("category", e.target.value);
            }}
          >
            <option value="">—</option>
            {withCurrent(categories, form.category).map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
        </label>
        {hints.length > 0 && (
          <>
            <p className={styles.hintsTitle}>Какой картой платить</p>
            <ul className={styles.hints}>
              {hints.map((h) => (
                <li key={h.card}>
                  <button
                    type="button"
                    aria-pressed={nameKey(form.card) === nameKey(h.card)}
                    onClick={() => {
                      set("card", h.card);
                    }}
                  >
                    <span className={styles.hintCard}>
                      {h.card}
                      <span className={`${styles.hintLeft} ${h.exhausted ? styles.exhausted : ""}`}>
                        {h.exhausted
                          ? "лимит исчерпан"
                          : h.remainingCashback !== null
                            ? `осталось кэшбэка ${formatMoney(h.remainingCashback)} ₽`
                            : h.remainingSpend !== null
                              ? `осталось лимита ${formatMoney(h.remainingSpend)} ₽`
                              : "без лимита"}
                      </span>
                    </span>
                    <span className={styles.hintRate}>{formatPercent(h.rateBp)}</span>
                    <span className={styles.hintSum}>
                      {h.expected === null ? "" : `+${formatMoney(h.expected)}`}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </>
        )}
        <label className={styles.field}>
          <span>Карта</span>
          <select
            value={form.card}
            onChange={(e) => {
              set("card", e.target.value);
            }}
          >
            <option value="">—</option>
            {withCurrent(cards, form.card).map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
        </label>
        <div className={styles.pair}>
          <label className={styles.field}>
            <span>Участник</span>
            <select
              value={form.participant}
              onChange={(e) => {
                set("participant", e.target.value);
              }}
            >
              <option value="">—</option>
              {withCurrent(participants, form.participant).map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
          </label>
          <label className={styles.field}>
            <span>Дата</span>
            <input
              type="date"
              value={form.date}
              onChange={(e) => {
                set("date", e.target.value);
              }}
            />
          </label>
        </div>
        <label className={styles.check}>
          <input
            type="checkbox"
            checked={form.regular}
            onChange={(e) => {
              set("regular", e.target.checked);
            }}
          />
          Регулярная
        </label>
        {message !== null && (
          <p
            className={message.ok ? styles.ok : styles.error}
            role={message.ok ? "status" : "alert"}
          >
            {message.text}
          </p>
        )}
        <div className={styles.actions}>
          {editing !== null && (
            <button
              type="button"
              className={styles.secondary}
              onClick={() => {
                setEditing(null);
                setForm(emptyForm(today));
                setMessage(null);
              }}
            >
              Отмена
            </button>
          )}
          <button type="submit" className={styles.primary} disabled={busy}>
            {editing === null ? "Внести" : "Сохранить"}
          </button>
        </div>
        {editing !== null && (
          <p>
            <button type="button" className={`${styles.plain} ${styles.danger}`} onClick={remove}>
              Удалить строку
            </button>
          </p>
        )}
      </form>

      <div className={styles.rowsHead}>
        <h2>Строки {sheetName(month)}</h2>
        {data !== null && <span>Итого {formatMoney(data.view.totals.total)} ₽</span>}
      </div>
      {rows.length === 0 ? (
        <p className={styles.empty}>В этом месяце строк нет.</p>
      ) : (
        <ul className={styles.rows}>
          {rows.map((r) => {
            const mine = own.has(r.id);
            const meta = [
              r.date === null ? null : formatDate(r.date).slice(0, 5),
              r.category,
              r.card,
              r.cashback === null || r.cashback === 0 ? null : `кэшбэк ${formatMoney(r.cashback)}`,
            ]
              .filter((p) => p !== null)
              .join(", ");
            const body = (
              <>
                <span className={styles.rowTitle}>{r.title}</span>
                <span className={styles.rowSum}>{formatMoney(r.amount)}</span>
                <span className={styles.rowMeta}>
                  {meta}
                  {mine && <span className={styles.waiting}> — ждёт компьютера</span>}
                </span>
              </>
            );
            return (
              <li key={r.id}>
                {mine ? (
                  <button
                    type="button"
                    className={styles.row}
                    onClick={() => {
                      setEditing(r);
                      setForm(formOf(r, today));
                      setMessage(null);
                      window.scrollTo({ top: 0 });
                    }}
                  >
                    {body}
                  </button>
                ) : (
                  <div className={styles.row}>{body}</div>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

/** The active names, plus the current value when it is not among them. */
function withCurrent(names: string[], current: string): string[] {
  return current === "" || names.includes(current) ? names : [current, ...names];
}
