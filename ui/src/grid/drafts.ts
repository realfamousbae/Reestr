import { CompactSelection, type GridSelection } from "@glideapps/glide-data-grid";
import { useCallback, useState } from "react";

const EMPTY: GridSelection = { columns: CompactSelection.empty(), rows: CompactSelection.empty() };

/**
 * The row being typed at the bottom of a grid that is not in the file yet. «Новая строка» or
 * Ctrl+N opens it; the item is added with its first typed cell, after which the row count
 * grows and the draft goes away by itself. Left blank, it disappears as soon as the cursor
 * leaves it, so it never leaves an empty row behind and the next «Новая строка» opens a fresh
 * one. The grid's selection is held here to see the cursor move.
 */
export function useDraftRow(length: number): {
  count: number;
  add: () => Promise<"bottom">;
  selection: GridSelection;
  onSelectionChange: (selection: GridSelection) => void;
} {
  const [at, setAt] = useState<number | null>(null);
  const [selection, setSelection] = useState<GridSelection>(EMPTY);
  const add = useCallback(() => {
    setAt(length);
    return Promise.resolve<"bottom">("bottom");
  }, [length]);
  const onSelectionChange = useCallback(
    (next: GridSelection) => {
      setSelection(next);
      if (next.current?.cell[1] !== length) setAt(null);
    },
    [length],
  );
  return { count: at === length ? 1 : 0, add, selection, onSelectionChange };
}
