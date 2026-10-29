import { useEffect, useRef, useState } from "react";

import type { ExportFormat } from "../api";
import { EXPORT_FORMATS } from "../export/exportFile";
import styles from "./ExportMenu.module.css";

interface Props {
  disabled: boolean;
  onExport: (format: ExportFormat) => void;
}

/** «Экспорт…» in the header: a short menu of the two formats. */
export function ExportMenu({ disabled, onExport }: Props) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className={styles.root} ref={root}>
      <button
        type="button"
        className={styles.button}
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => {
          setOpen(!open);
        }}
      >
        Экспорт…
      </button>
      {open && (
        <ul className={styles.menu} role="menu" aria-label="Экспорт">
          {EXPORT_FORMATS.map((f) => (
            <li key={f.format} role="none">
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setOpen(false);
                  onExport(f.format);
                }}
              >
                <span className={styles.label}>{f.label}</span>
                <span className={styles.hint}>{f.hint}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
