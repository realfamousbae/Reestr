import type { Theme } from "@glideapps/glide-data-grid";

import type { CellThemes } from "../grid/columns";

/** Reads a token from tokens.css, so the canvas grid and the DOM share one palette. */
function token(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

export function gridTheme(fontSize: number): Partial<Theme> {
  return {
    accentColor: token("--accent"),
    accentFg: token("--sheet"),
    accentLight: token("--accent-wash"),
    textDark: token("--ink"),
    textMedium: token("--ink-muted"),
    textLight: token("--ink-faint"),
    textHeader: token("--ink-muted"),
    textHeaderSelected: token("--sheet"),
    bgIconHeader: token("--ink-muted"),
    fgIconHeader: token("--sheet"),
    bgCell: token("--sheet"),
    bgCellMedium: token("--paper"),
    bgHeader: token("--band"),
    bgHeaderHasFocus: token("--band-strong"),
    bgHeaderHovered: token("--band-strong"),
    bgSearchResult: token("--warn-bg"),
    borderColor: token("--ruling"),
    horizontalBorderColor: token("--ruling"),
    headerBottomBorderColor: token("--ruling-strong"),
    drilldownBorder: token("--ruling"),
    linkColor: token("--accent"),
    fontFamily: token("--font"),
    baseFontStyle: `${fontSize}px`,
    headerFontStyle: `${Math.max(12, fontSize - 2)}px`,
    markerFontStyle: `${Math.max(12, fontSize - 2)}px`,
    editorFontSize: `${fontSize}px`,
    cellHorizontalPadding: 8,
    cellVerticalPadding: 3,
    lineHeight: 1.4,
    roundingRadius: 2,
  };
}

export function cellThemes(): CellThemes {
  return {
    derived: { textDark: token("--ink-muted") },
    danger: { textDark: token("--danger") },
    warn: { textDark: token("--warn") },
  };
}
