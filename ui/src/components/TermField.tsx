import { useState } from "react";

import type { Parsed } from "../format/parsed";
import styles from "./TermField.module.css";

interface Props<T> {
  label: string;
  /** The text the field shows when it is not being edited. */
  value: string;
  parse: (text: string) => Parsed<T>;
  onSave: (value: T) => void;
  onError: (message: string) => void;
  width?: number;
  placeholder?: string;
}

/** A labelled term of a product, saved on Enter or when the field loses focus. */
export function TermField<T>({
  label,
  value,
  parse,
  onSave,
  onError,
  width = 120,
  placeholder,
}: Props<T>) {
  // The typed text while editing; null shows the saved value.
  const [draft, setDraft] = useState<string | null>(null);

  const save = () => {
    if (draft === null || draft === value) {
      setDraft(null);
      return;
    }
    const parsed = parse(draft);
    if (parsed.ok) {
      onSave(parsed.value);
      setDraft(null);
    } else {
      onError(parsed.message);
    }
  };

  return (
    <label className={styles.field}>
      <span>{label}</span>
      <input
        value={draft ?? value}
        style={{ width }}
        placeholder={placeholder}
        onChange={(e) => {
          setDraft(e.target.value);
        }}
        onBlur={save}
        onKeyDown={(e) => {
          if (e.key === "Enter") save();
          if (e.key === "Escape") setDraft(null);
        }}
      />
    </label>
  );
}
