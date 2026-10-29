// Turning grid edits and clipboard pastes into journal operations for the core.

import type { EntryEdit, EntryView, JournalOp } from "../api";
import { COLUMNS, type CellContext, type ColumnKey, parseCell } from "./columns";

export interface CellChange {
  col: number;
  row: number;
  text: string;
}

export interface Batch {
  ops: JournalOp[];
  /** Messages for cells that could not be read; those cells are left unchanged. */
  errors: string[];
}

/**
 * Builds one batch from changed cells. Rows at or beyond `entries.length` become new rows at
 * the bottom, in order. When a row gets both «%» and «Кэшбэк», the percentage wins: in the
 * workbook the cashback column is a formula of it.
 */
export function buildBatch(
  changes: readonly CellChange[],
  entries: readonly EntryView[],
  ctx: CellContext,
): Batch {
  const byRow = new Map<number, Map<ColumnKey, string>>();
  for (const change of changes) {
    const spec = COLUMNS[change.col];
    if (spec === undefined || change.row < 0) {
      continue;
    }
    const row = byRow.get(change.row) ?? new Map<ColumnKey, string>();
    row.set(spec.key, change.text);
    byRow.set(change.row, row);
  }

  const ops: JournalOp[] = [];
  const errors: string[] = [];
  const appends: { row: number; edits: EntryEdit[] }[] = [];

  for (const [rowIndex, cells] of [...byRow.entries()].sort(([a], [b]) => a - b)) {
    if (cells.has("rate") && (cells.get("rate") ?? "").trim() !== "") {
      cells.delete("cashback");
    }
    const edits: EntryEdit[] = [];
    // Column order matters: the amount comes before the rate it is multiplied by.
    for (const spec of COLUMNS) {
      const text = cells.get(spec.key);
      if (text === undefined) {
        continue;
      }
      const parsed = parseCell(spec.key, text, ctx);
      if (parsed.ok) {
        edits.push(parsed.value);
      } else {
        errors.push(parsed.message);
      }
    }
    if (edits.length === 0) {
      continue;
    }
    const entry = entries[rowIndex];
    if (entry === undefined) {
      // A new row starts only with something in it: blank pasted lines add nothing.
      if ([...cells.values()].every((text) => text.trim() === "")) continue;
      appends.push({ row: rowIndex, edits });
    } else {
      for (const edit of edits) {
        ops.push({ op: "edit", id: entry.id, edit });
      }
    }
  }

  for (const append of appends) {
    ops.push({ op: "append", edits: append.edits });
  }
  return { ops, errors };
}

/** Expands a pasted block (rows × columns of text) into cell changes starting at `target`. */
export function pasteChanges(
  target: readonly [col: number, row: number],
  values: readonly (readonly string[])[],
): CellChange[] {
  const [col0, row0] = target;
  const changes: CellChange[] = [];
  values.forEach((line, dy) => {
    line.forEach((text, dx) => {
      const col = col0 + dx;
      if (col < COLUMNS.length) {
        changes.push({ col, row: row0 + dy, text });
      }
    });
  });
  return changes;
}
