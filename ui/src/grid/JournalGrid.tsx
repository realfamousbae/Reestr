import "@glideapps/glide-data-grid/dist/index.css";

import {
  DataEditor,
  type DataEditorRef,
  type EditListItem,
  type GridCell,
  type GridColumn,
  GridCellKind,
  type GridSelection,
  type Item,
  type ProvideEditorCallback,
} from "@glideapps/glide-data-grid";
import { forwardRef, useCallback, useMemo, useState } from "react";

import type { EntryView, SuggestionView } from "../api";
import { cellThemes, gridTheme } from "../theme/gridTheme";
import { cardEditor } from "./CardEditor";
import { categoryEditor } from "./CategoryEditor";
import { CARD_CELL, CATEGORY_CELL, COLUMNS, TITLE_CELL, cellContent } from "./columns";
import { useDraftRow } from "./drafts";
import { type CellChange, pasteChanges } from "./edits";
import { titleEditor } from "./TitleEditor";

interface Props {
  entries: readonly EntryView[];
  /** Past titles, most recent first, for autocomplete of «Наименование». */
  titles: readonly string[];
  /** The active categories, for the list of «Назначение». */
  categories: readonly string[];
  /** Card suggestions for a row (D6); null when they are off. */
  suggest: ((entry: number) => Promise<SuggestionView[]>) | null;
  fontSize: number;
  /** Typed cells, fills and pastes, as one batch. */
  onChanges: (changes: CellChange[]) => void;
  /** Rows selected by their numbers and deleted with Delete; the parent confirms. */
  onDeleteRows: (ids: number[]) => void;
}

/** The month journal: an Excel-like grid over the entries of one month. */
export const JournalGrid = forwardRef<DataEditorRef, Props>(function JournalGrid(
  { entries, titles, categories, suggest, fontSize, onChanges, onDeleteRows },
  ref,
) {
  // «Новая строка» and Ctrl+N open a row at the bottom that is not in the file yet (useDraftRow).
  const draft = useDraftRow(entries.length);
  const [widths, setWidths] = useState<Record<string, number>>({});
  const columns = useMemo<GridColumn[]>(
    () => COLUMNS.map((c) => ({ id: c.key, title: c.title, width: widths[c.key] ?? c.width })),
    [widths],
  );
  const theme = useMemo(() => gridTheme(fontSize), [fontSize]);
  const themes = useMemo(() => cellThemes(), []);

  const getCellContent = useCallback(
    ([col, row]: Item): GridCell => {
      const spec = COLUMNS[col];
      if (spec === undefined) {
        return { kind: GridCellKind.Loading, allowOverlay: false };
      }
      return cellContent(entries[row], spec, themes);
    },
    [entries, themes],
  );

  const provideEditor = useMemo<ProvideEditorCallback<GridCell>>(() => {
    type Editor = ReturnType<ProvideEditorCallback<GridCell>>;
    const TitleEditor = titleEditor(titles) as unknown as Editor;
    const CategoryEditor = categoryEditor(categories) as unknown as Editor;
    const CardEditor = suggest === null ? undefined : (cardEditor(suggest) as unknown as Editor);
    return (cell) => {
      if (TITLE_CELL in cell) return TitleEditor;
      if (CATEGORY_CELL in cell) return CategoryEditor;
      if (CARD_CELL in cell) return CardEditor;
      return undefined;
    };
  }, [titles, categories, suggest]);

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
      onChanges(pasteChanges(target, values));
      return false;
    },
    [onChanges],
  );

  const onDelete = useCallback(
    (selection: GridSelection) => {
      const rows = selection.rows.toArray();
      if (rows.length === 0) {
        // Plain cells: let the grid clear them; the clears arrive as edits.
        return true;
      }
      const ids = rows.flatMap((r) => {
        const entry = entries[r];
        return entry === undefined ? [] : [entry.id];
      });
      if (ids.length > 0) {
        onDeleteRows(ids);
      }
      return false;
    },
    [entries, onDeleteRows],
  );

  const onColumnResize = useCallback((column: GridColumn, newSize: number) => {
    const id = column.id;
    if (id !== undefined) {
      setWidths((w) => ({ ...w, [id]: newSize }));
    }
  }, []);

  return (
    <DataEditor
      ref={ref}
      width="100%"
      height="100%"
      columns={columns}
      rows={entries.length + draft.count}
      gridSelection={draft.selection}
      onGridSelectionChange={draft.onSelectionChange}
      getCellContent={getCellContent}
      getCellsForSelection
      onCellsEdited={onCellsEdited}
      provideEditor={provideEditor}
      onPaste={onPaste}
      onDelete={onDelete}
      onRowAppended={draft.add}
      trailingRowOptions={{ hint: "Новая строка", sticky: true, tint: true }}
      rowMarkers={{ kind: "clickable-number", width: 48 }}
      onColumnResize={onColumnResize}
      keybindings={{ search: true, downFill: true }}
      cellActivationBehavior="double-click"
      fillHandle
      smoothScrollX
      smoothScrollY
      rowHeight={Math.round(fontSize * 2.1)}
      headerHeight={Math.round(fontSize * 2.4)}
      theme={theme}
    />
  );
});
