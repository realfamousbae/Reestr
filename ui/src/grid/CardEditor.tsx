import type { ProvideEditorComponent, TextCell } from "@glideapps/glide-data-grid";
import { type KeyboardEvent, useEffect, useLayoutEffect, useRef, useState } from "react";

import type { SuggestionView } from "../api";
import { formatPercent } from "../format/percent";
import { nameKey } from "../format/text";
import { suggestionText } from "../rules/limits";
import styles from "./CardEditor.module.css";
import { CARD_CELL } from "./columns";

export type CardCell = TextCell & { [CARD_CELL]: number };

/**
 * The editor of «Карта» with the card suggestion (D6): under the cell, the cards with a rule for
 * the row's category, best first, used-up limits last and grey. Arrows walk the list and put
 * the card into the cell; Enter or Tab accepts, a click picks. Typing filters the list by the
 * beginning of the name. With no rules for the month the list stays away.
 */
export function cardEditor(
  load: (entry: number) => Promise<SuggestionView[]>,
): ProvideEditorComponent<CardCell> {
  return function CardEditor({ value, onChange, onFinishedEditing, isHighlighted }) {
    const input = useRef<HTMLInputElement>(null);
    const [text, setText] = useState(value.data);
    const [all, setAll] = useState<SuggestionView[]>([]);
    const [active, setActive] = useState(-1);
    const entry = value[CARD_CELL];

    useEffect(() => {
      let live = true;
      load(entry)
        .then((list) => {
          if (live) setAll(list);
        })
        .catch(() => {
          // No list is fine: the cell still takes a typed card.
        });
      return () => {
        live = false;
      };
    }, [entry]);

    useLayoutEffect(() => {
      const el = input.current;
      if (el === null) return;
      el.focus();
      if (isHighlighted) {
        el.select();
      } else {
        el.setSelectionRange(el.value.length, el.value.length);
      }
      // Only on mount.
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    // While walking the list the cell shows the card, so the list keeps the typed filter.
    const [filter, setFilter] = useState(isHighlighted ? "" : value.data);
    const key = nameKey(filter);
    const shown = all.filter((s) => nameKey(s.card).startsWith(key));

    const put = (next: string) => {
      setText(next);
      onChange({ ...value, data: next, displayData: next, copyData: next });
    };

    const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
      if (shown.length === 0 || (e.key !== "ArrowDown" && e.key !== "ArrowUp")) return;
      e.preventDefault();
      e.stopPropagation();
      const step = e.key === "ArrowDown" ? 1 : -1;
      const next = (active + step + shown.length) % shown.length;
      setActive(next);
      const card = shown[next];
      if (card !== undefined) put(card.card);
    };

    return (
      <div className={styles.editor}>
        <input
          ref={input}
          className={styles.input}
          value={text}
          onChange={(e) => {
            setFilter(e.target.value);
            setActive(-1);
            put(e.target.value);
          }}
          onKeyDown={onKeyDown}
          spellCheck={false}
          autoComplete="off"
          aria-label="Карта"
          aria-autocomplete="list"
          aria-controls="card-suggestions"
          aria-activedescendant={active >= 0 ? `card-suggestion-${active.toString()}` : undefined}
        />
        {shown.length > 0 && (
          <>
            <ul id="card-suggestions" className={styles.list} role="listbox">
              {shown.map((s, i) => (
                <li
                  key={s.card}
                  id={`card-suggestion-${i.toString()}`}
                  role="option"
                  aria-selected={i === active}
                  className={s.exhausted ? styles.exhausted : undefined}
                  onMouseDown={(e) => {
                    // Keep focus in the input; the click picks the card.
                    e.preventDefault();
                    onFinishedEditing({ ...value, data: s.card, displayData: s.card }, [0, 0]);
                  }}
                >
                  <span className={styles.card}>{s.card}</span>
                  <span className={styles.rate}>{formatPercent(s.rateBp)}</span>
                  <span className={styles.detail}>{suggestionText(s)}</span>
                </li>
              ))}
            </ul>
            <p className={styles.note}>По правилам месяца и данным журнала</p>
          </>
        )}
      </div>
    );
  };
}
