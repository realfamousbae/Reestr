import type { ProvideEditorComponent, TextCell } from "@glideapps/glide-data-grid";
import { type KeyboardEvent, useLayoutEffect, useRef, useState } from "react";

import { isKnown, nameKey } from "../format/text";
import styles from "./CardEditor.module.css";
import { CATEGORY_CELL } from "./columns";

export type CategoryCell = TextCell & { [CATEGORY_CELL]: true };

/** Names that begin with the typed text first, then names that contain it. */
export function categoryOptions(typed: string, categories: readonly string[]): string[] {
  const key = nameKey(typed);
  if (key === "") return [...categories];
  const starts = categories.filter((c) => nameKey(c).startsWith(key));
  const contains = categories.filter(
    (c) => !nameKey(c).startsWith(key) && nameKey(c).includes(key),
  );
  return [...starts, ...contains];
}

/**
 * The editor of «Назначение»: the categories of the dictionary under the cell. Typing filters
 * them, arrows walk the list and put the name into the cell, Enter or Tab accepts, a click
 * picks. A new name is allowed: it is added to the dictionary (D17), and the list says so.
 */
export function categoryEditor(
  categories: readonly string[],
): ProvideEditorComponent<CategoryCell> {
  return function CategoryEditor({ value, onChange, onFinishedEditing, isHighlighted }) {
    const input = useRef<HTMLInputElement>(null);
    const [text, setText] = useState(value.data);
    const [filter, setFilter] = useState(isHighlighted ? "" : value.data);
    const [active, setActive] = useState(-1);
    const shown = categoryOptions(filter, categories);

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
      const name = shown[next];
      if (name !== undefined) put(name);
    };

    const typed = filter.trim();
    const isNew = typed !== "" && !isKnown(typed, categories) && shown.length === 0;
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
          aria-label="Назначение"
          aria-autocomplete="list"
          aria-controls="category-options"
          aria-activedescendant={active >= 0 ? `category-option-${active.toString()}` : undefined}
        />
        {shown.length > 0 && (
          <ul id="category-options" className={`${styles.list} ${styles.scroll}`} role="listbox">
            {shown.map((name, i) => (
              <li
                key={name}
                id={`category-option-${i.toString()}`}
                role="option"
                aria-selected={i === active}
                className={styles.plain}
                onMouseDown={(e) => {
                  // Keep focus in the input; the click picks the category.
                  e.preventDefault();
                  onFinishedEditing({ ...value, data: name, displayData: name }, [0, 0]);
                }}
              >
                {name}
              </li>
            ))}
          </ul>
        )}
        {isNew && <p className={styles.note}>Новое назначение — добавится в справочник</p>}
      </div>
    );
  };
}
