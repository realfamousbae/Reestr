// Tables other than the journal (dictionaries, cashback rules): columns that show a row and read
// typed text, and the step from changed cells to edits.

import type { Parsed } from "../format/parsed";
import type { CellChange } from "./edits";

/** A row of any table: what all of them share. */
export interface TableRow {
  id: number;
}

export interface TableColumn<R extends TableRow, I> {
  key: string;
  title: string;
  width: number;
  align?: "left" | "right" | "center";
  show: (row: R) => string;
  /** The text in the editor and on the clipboard; `show` by default. */
  edit?: (row: R) => string;
  /** Undefined: the column is read-only. */
  parse?: (text: string) => Parsed<I>;
  /** A state shown in color: a used-up limit, an overdue or close deadline. */
  status?: (row: R) => "danger" | "warn" | undefined;
}

export interface TableEdits<I> {
  edits: { id: number; edit: I }[];
  /** Rows past the end: new items, each with its inputs, in row order. */
  appends: I[][];
  /** Messages for cells that could not be read; those cells are left unchanged. */
  errors: string[];
}

/** Reads changed cells; rows at or past the end of `rows` become new items. */
export function collectEdits<R extends TableRow, I>(
  changes: readonly CellChange[],
  rows: readonly R[],
  columns: readonly TableColumn<R, I>[],
): TableEdits<I> {
  const edits: { id: number; edit: I }[] = [];
  const errors: string[] = [];
  const appends = new Map<number, I[]>();

  for (const change of [...changes].sort((a, b) => a.row - b.row || a.col - b.col)) {
    const column = columns[change.col];
    if (column?.parse === undefined) {
      continue;
    }
    const parsed = column.parse(change.text);
    if (!parsed.ok) {
      errors.push(parsed.message);
      continue;
    }
    const row = rows[change.row];
    if (row === undefined) {
      const inputs = appends.get(change.row) ?? [];
      inputs.push(parsed.value);
      appends.set(change.row, inputs);
    } else {
      edits.push({ id: row.id, edit: parsed.value });
    }
  }
  return { edits, appends: [...appends.values()], errors };
}
