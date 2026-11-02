import type { DataEditorProps } from "@glideapps/glide-data-grid";

/** The title of the row-number column. Glide has no prop for it: its marker title is "". */
const MARKER_TITLE = "№";

/**
 * Draws «№» over the row numbers (both grids). Glide calls `drawHeader` for the marker column
 * too, with `columnIndex` -1 (it subtracts the marker offset). The default header is always
 * drawn first, so the select-all checkbox keeps working; the label fades out as that checkbox
 * fades in on hover, and is skipped while the checkbox is ticked.
 */
export const drawMarkerHeader: NonNullable<DataEditorProps["drawHeader"]> = (args, draw) => {
  draw();
  if (args.columnIndex !== -1) return;
  const { ctx, theme, rect, hoverAmount, column } = args;
  const checked = (column as { rowMarkerChecked?: boolean }).rowMarkerChecked === true;
  if (checked) return;
  ctx.save();
  ctx.globalAlpha = 1 - hoverAmount;
  ctx.fillStyle = args.isSelected ? theme.textHeaderSelected : theme.textHeader;
  ctx.font = `${theme.headerFontStyle} ${theme.fontFamily}`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText(MARKER_TITLE, rect.x + rect.width / 2, rect.y + rect.height / 2);
  ctx.restore();
};
