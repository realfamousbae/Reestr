import type { CardSetup, Compared, ImportPreview, MonthCheck } from "../api";
import { formatMoney } from "../format/money";

/** Card settings the wizard starts with: the guesses for new cards, nothing for known ones. */
export function initialSetups(preview: ImportPreview): CardSetup[] {
  return preview.cards.map((c) =>
    c.exists
      ? { shortName: c.shortName, bank: null, kind: null, participant: null }
      : {
          shortName: c.shortName,
          bank: c.guess.bank,
          kind: c.guess.kind,
          participant: c.guess.participant,
        },
  );
}

/** How many rows an import would add. */
export function rowsToAdd(preview: ImportPreview): number {
  return preview.months.reduce((sum, m) => sum + m.rows - m.already, 0);
}

/** «Еда: в листе 1 200, у нас 1 150» for each number that differs. */
export function mismatches(check: MonthCheck): string[] {
  return check.compared
    .filter((c: Compared) => c.expected !== c.actual)
    .map((c) => `${c.label}: в листе ${formatMoney(c.expected)}, у нас ${formatMoney(c.actual)}`);
}
