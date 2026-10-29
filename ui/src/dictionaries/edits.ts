import type { DictOp } from "../api";
import type { CellChange } from "../grid/edits";
import { collectEdits } from "../grid/table";
import type { DictColumn, DictRow } from "./specs";

export interface DictBatch {
  ops: DictOp[];
  errors: string[];
}

/**
 * Turns changed cells of a dictionary table into operations. Rows past the end become new
 * items; a new item needs its name in the same batch.
 */
export function buildDictOps(
  changes: readonly CellChange[],
  rows: readonly DictRow[],
  columns: readonly DictColumn<DictRow>[],
): DictBatch {
  const { edits, appends, errors } = collectEdits(changes, rows, columns);
  const ops: DictOp[] = edits.map(({ id, edit }) => ({ op: "edit", id, edit }));
  for (const inputs of appends) {
    if (inputs.some((e) => e.field === "name")) {
      ops.push({ op: "append", edits: inputs });
    } else {
      errors.push("Новую строку начните с названия");
    }
  }
  return { ops, errors };
}
