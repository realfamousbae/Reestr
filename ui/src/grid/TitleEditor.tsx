import type { ProvideEditorComponent, TextCell } from "@glideapps/glide-data-grid";
import { type ChangeEvent, type KeyboardEvent, useLayoutEffect, useRef, useState } from "react";

import { complete } from "../format/text";
import styles from "./TitleEditor.module.css";

/**
 * The editor of «Наименование» with Excel-style AutoComplete: while typing, the rest of the
 * most recent matching title appears selected; typing on replaces it, Enter or Tab accepts
 * it, Backspace removes it. The grid's overlay handles Enter, Tab and Escape.
 */
export function titleEditor(history: readonly string[]): ProvideEditorComponent<TextCell> {
  return function TitleEditor({ value, onChange, isHighlighted }) {
    const input = useRef<HTMLInputElement>(null);
    const [text, setText] = useState(() => {
      // Opened by typing a letter: complete right away.
      const initial = value.data;
      return isHighlighted ? initial : (complete(initial, history) ?? initial);
    });
    const typedLength = useRef(isHighlighted ? -1 : value.data.length);

    useLayoutEffect(() => {
      const el = input.current;
      if (el === null) return;
      el.focus();
      if (typedLength.current < 0) {
        el.select();
      } else {
        el.setSelectionRange(typedLength.current, text.length);
      }
      // Only on mount: later selections are set in onInput.
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    const update = (next: string, selectFrom?: number) => {
      setText(next);
      onChange({ ...value, data: next, displayData: next, copyData: next });
      if (selectFrom !== undefined) {
        requestAnimationFrame(() => input.current?.setSelectionRange(selectFrom, next.length));
      }
    };

    const onInput = (e: ChangeEvent<HTMLInputElement>) => {
      const el = e.target;
      const typed = el.value;
      const native = e.nativeEvent as InputEvent;
      const atEnd = el.selectionStart === typed.length;
      const inserting = native.inputType.startsWith("insert");
      const completion = inserting && atEnd ? complete(typed, history) : null;
      if (completion !== null) {
        update(completion, typed.length);
      } else {
        update(typed);
      }
    };

    const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
      // Backspace over a selected completion removes only the completion.
      const el = e.currentTarget;
      if (
        e.key === "Backspace" &&
        el.selectionStart !== null &&
        el.selectionEnd === text.length &&
        el.selectionStart < text.length
      ) {
        e.preventDefault();
        update(text.slice(0, el.selectionStart));
      }
    };

    return (
      <input
        ref={input}
        className={styles.input}
        value={text}
        onChange={onInput}
        onKeyDown={onKeyDown}
        spellCheck={false}
        autoComplete="off"
        aria-label="Наименование"
      />
    );
  };
}
