// Settings of the view (D34): the font size of the tables and the blocks on the right of a
// month, in their order. They belong to this computer, not to the file.

export const FONT_SIZES = [13, 14, 15, 16, 17, 18, 20] as const;
export const DEFAULT_FONT_SIZE = 15;

export type BlockId =
  "cashback" | "regular" | "limits" | "credit" | "savings" | "income" | "deadlines";

/** The blocks of a month sheet in their default order (PLAN §8), «Сроки» last. */
export const BLOCKS: readonly { id: BlockId; title: string }[] = [
  { id: "cashback", title: "Кэшбэк" },
  { id: "regular", title: "Регулярные платежи" },
  { id: "limits", title: "Лимиты по картам" },
  { id: "credit", title: "Кредитки / грейс" },
  { id: "savings", title: "Накопительные счета" },
  { id: "income", title: "Приход / расход" },
  { id: "deadlines", title: "Сроки" },
];

export interface ViewSettings {
  fontSize: number;
  /** Every block, in the order shown. */
  order: BlockId[];
  hidden: BlockId[];
  /** Show «1 200,00» instead of «1 200» (formatMoney). */
  alwaysKopecks: boolean;
}

const KEY = "reestr.view";

export const DEFAULT_VIEW: ViewSettings = {
  fontSize: DEFAULT_FONT_SIZE,
  order: BLOCKS.map((b) => b.id),
  hidden: [],
  alwaysKopecks: false,
};

function isBlock(value: unknown): value is BlockId {
  return BLOCKS.some((b) => b.id === value);
}

/**
 * Settings read back from storage: unknown blocks are dropped, blocks added by a newer
 * version go to the end, a font size outside the list falls back to the default.
 */
export function normalizeView(raw: unknown): ViewSettings {
  if (typeof raw !== "object" || raw === null) return DEFAULT_VIEW;
  const r = raw as Record<string, unknown>;
  const fontSize = FONT_SIZES.find((s) => s === r.fontSize) ?? DEFAULT_FONT_SIZE;
  const listed = Array.isArray(r.order) ? r.order.filter(isBlock) : [];
  const order = [...new Set(listed)];
  for (const b of BLOCKS) if (!order.includes(b.id)) order.push(b.id);
  const hidden = Array.isArray(r.hidden) ? [...new Set(r.hidden.filter(isBlock))] : [];
  return { fontSize, order, hidden, alwaysKopecks: r.alwaysKopecks === true };
}

export function readView(): ViewSettings {
  try {
    const raw = localStorage.getItem(KEY);
    return raw === null ? DEFAULT_VIEW : normalizeView(JSON.parse(raw));
  } catch {
    return DEFAULT_VIEW;
  }
}

export function writeView(view: ViewSettings): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(view));
  } catch {
    // A view preference; losing it is harmless.
  }
}

/** The blocks to show, in order. */
export function shownBlocks(view: ViewSettings): BlockId[] {
  return view.order.filter((id) => !view.hidden.includes(id));
}

/** Moves a block one place up (−1) or down (+1). */
export function moveBlock(view: ViewSettings, id: BlockId, step: -1 | 1): ViewSettings {
  const i = view.order.indexOf(id);
  const other = view.order[i + step];
  if (i < 0 || other === undefined) return view;
  const order = [...view.order];
  order[i] = other;
  order[i + step] = id;
  return { ...view, order };
}

export function toggleBlock(view: ViewSettings, id: BlockId): ViewSettings {
  const hidden = view.hidden.includes(id)
    ? view.hidden.filter((h) => h !== id)
    : [...view.hidden, id];
  return { ...view, hidden };
}
