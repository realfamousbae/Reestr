import "@glideapps/glide-data-grid/dist/index.css";

import {
  DataEditor,
  type EditListItem,
  type GridCell,
  GridCellKind,
  type GridColumn,
  type GridSelection,
  type Item,
} from "@glideapps/glide-data-grid";
import { useCallback, useMemo } from "react";

import { cellThemes, gridTheme } from "../theme/gridTheme";
import { useDraftRow } from "./drafts";
import { type CellChange, pasteChanges } from "./edits";
import type { TableColumn, TableRow } from "./table";

interface Props<R extends TableRow> {
  columns: readonly TableColumn<R, unknown>[];
  rows: readonly R[];
  fontSize: number;
  /** The hint of the trailing row a new item starts in; without it rows are not added here. */
  newRowHint?: string;
  /** Typed cells and pastes, as one batch; the parent turns them into operations. */
  onChanges: (changes: CellChange[]) => void;
  /** Rows selected by their numbers and deleted with Delete; the parent confirms. */
  onDeleteRows: (ids: number[]) => void;
}

/** An editable table of items (a dictionary, the month's rules); a new item starts at the end. */
export function TableGrid<R extends TableRow>({
  columns,
  rows,
  fontSize,
  newRowHint,
  onChanges,
  onDeleteRows,
}: Props<R>) {
  // The row being typed into that does not exist in the file yet (see useDraftRow).
  const draft = useDraftRow(rows.length);

  const gridColumns = useMemo<GridColumn[]>(
    () => columns.map((c) => ({ id: c.key, title: c.title, width: c.width })),
    [columns],
  );
  const theme = useMemo(() => gridTheme(fontSize), [fontSize]);
  const themes = useMemo(() => cellThemes(), []);

  const getCellContent = useCallback(
    ([col, row]: Item): GridCell => {
      const column = columns[col];
      if (column === undefined) {
        return { kind: GridCellKind.Loading, allowOverlay: false };
      }
      const item = rows[row];
      const shown = item === undefined ? "" : column.show(item);
      const edit = item === undefined ? "" : (column.edit ?? column.show)(item);
      const editable = column.parse !== undefined;
      const status = item === undefined ? undefined : column.status?.(item);
      const themeOverride =
        status === "danger"
          ? themes.danger
          : status === "warn"
            ? themes.warn
            : editable
              ? undefined
              : themes.derived;
      return {
        kind: GridCellKind.Text,
        data: edit,
        displayData: shown,
        copyData: edit,
        allowOverlay: editable,
        readonly: !editable,
        contentAlign: column.align ?? "left",
        ...(themeOverride === undefined ? {} : { themeOverride }),
      };
    },
    [columns, rows, themes],
  );

  const onCellsEdited = useCallback(
    (items: readonly EditListItem[]) => {
      onChanges(
        items.flatMap(({ location: [col, row], value }) =>
          value.kind === GridCellKind.Text ? [{ col, row, text: value.data }] : [],
        ),
      );
      return true;
    },
    [onChanges],
  );

  const onPaste = useCallback(
    (target: Item, values: readonly (readonly string[])[]) => {
      onChanges(pasteChanges(target, values).filter((c) => c.col < columns.length));
      return false;
    },
    [onChanges, columns],
  );

  const onDelete = useCallback(
    (selection: GridSelection) => {
      const ids = selection.rows.toArray().flatMap((r) => {
        const item = rows[r];
        return item === undefined ? [] : [item.id];
      });
      if (ids.length > 0) {
        onDeleteRows(ids);
        return false;
      }
      return selection.rows.length === 0;
    },
    [rows, onDeleteRows],
  );

  return (
    <DataEditor
      width="100%"
      height="100%"
      columns={gridColumns}
      rows={rows.length + draft.count}
      gridSelection={draft.selection}
      onGridSelectionChange={draft.onSelectionChange}
      getCellContent={getCellContent}
      getCellsForSelection
      onCellsEdited={onCellsEdited}
      onPaste={onPaste}
      onDelete={onDelete}
      {...(newRowHint === undefined
        ? {}
        : {
            onRowAppended: draft.add,
            trailingRowOptions: { hint: newRowHint, sticky: true, tint: true },
          })}
      rowMarkers={{ kind: "clickable-number", width: 48 }}
      cellActivationBehavior="double-click"
      smoothScrollX
      smoothScrollY
      rowHeight={Math.round(fontSize * 2.1)}
      headerHeight={Math.round(fontSize * 2.4)}
      theme={theme}
    />
  );
}
