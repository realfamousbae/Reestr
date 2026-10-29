// The tables of the «Справочники» sheet: which columns each dictionary has, how a cell is
// shown and how typed text becomes a change for the core.

import type { DictInput, DictKind, Dictionaries, ProductKind, Rounding } from "../api";
import { formatDate, parseDate } from "../format/date";
import { formatMoney, moneyEditText, parseMoney } from "../format/money";
import { type Parsed, fail, ok } from "../format/parsed";
import { matchOption, parseYesNo } from "../format/text";
import type { TableColumn, TableRow } from "../grid/table";

export const KIND_LABELS: Record<ProductKind, string> = {
  debit: "дебетовая",
  credit: "кредитная",
  savings: "накопительный счёт",
  deposit: "вклад",
  cash: "наличные",
};

export const ROUNDING_LABELS: Record<Rounding, string> = {
  floor_rub: "вниз до рубля",
  floor_kop: "вниз до копейки",
  half_up_rub: "до рубля",
  half_up_kop: "до копейки",
};

/** Reads a label of a fixed list: exact, or a unique beginning ("кр" → «кредитная»). */
function parseLabel<T extends string>(
  text: string,
  labels: Record<T, string>,
  column: string,
): Parsed<T | null> {
  if (text.trim() === "") {
    return ok(null);
  }
  const entries = Object.entries(labels) as [T, string][];
  const match = matchOption(
    text,
    entries.map(([, label]) => label),
  );
  const found =
    match?.known === true ? entries.find(([, label]) => label === match.value) : undefined;
  if (found === undefined) {
    const options = entries.map(([, label]) => `«${label}»`).join(", ");
    return fail(`В колонке «${column}» можно: ${options}`);
  }
  return ok(found[0]);
}

/** A row of any dictionary. */
export type DictRow = TableRow;

export type DictColumn<R extends DictRow> = TableColumn<R, DictInput>;

export interface DictSpec<R extends DictRow> {
  kind: DictKind;
  /** The tab title in the dictionaries sheet. */
  title: string;
  /** One line about what this list is for. */
  hint: string;
  rows: (d: Dictionaries) => R[];
  columns: DictColumn<R>[];
}

function names<T extends { id: number }>(items: T[], name: (t: T) => string) {
  const byId = new Map(items.map((i) => [i.id, name(i)]));
  return (id: number | null) => (id === null ? "" : (byId.get(id) ?? ""));
}

const yesNo = (v: boolean) => (v ? "да" : "");

function textColumn<R extends DictRow>(
  key: string,
  title: string,
  width: number,
  get: (r: R) => string | null,
  input: (v: string | null) => DictInput,
): DictColumn<R> {
  return {
    key,
    title,
    width,
    show: (r) => get(r) ?? "",
    parse: (text) => ok(input(text.trim() === "" ? null : text.trim())),
  };
}

function nameColumn<R extends DictRow>(
  title: string,
  width: number,
  get: (r: R) => string,
): DictColumn<R> {
  return {
    key: "name",
    title,
    width,
    show: get,
    parse: (text) =>
      text.trim() === ""
        ? fail("Название не может быть пустым")
        : ok({ field: "name", value: text.trim() }),
  };
}

function flagColumn<R extends DictRow>(
  key: "isActive" | "isRegularDefault" | "excludeFromRate",
  title: string,
  get: (r: R) => boolean,
  width = 104,
): DictColumn<R> {
  return {
    key,
    title,
    width,
    align: "center",
    show: (r) => yesNo(get(r)),
    parse: (text) => {
      const parsed = parseYesNo(text, title);
      return parsed.ok ? ok({ field: key, value: parsed.value }) : parsed;
    },
  };
}

function orderColumn<R extends DictRow & { sortOrder: number }>(): DictColumn<R> {
  return {
    key: "sortOrder",
    title: "Порядок",
    width: 84,
    align: "right",
    show: (r) => r.sortOrder.toString(),
    parse: (text) => {
      const n = Number(text.trim());
      return Number.isInteger(n)
        ? ok({ field: "sortOrder", value: n })
        : fail("Порядок — целое число");
    },
  };
}

/** A reference to another dictionary, typed as a name (created when new, D17). */
function refColumn<R extends DictRow>(
  key: string,
  title: string,
  width: number,
  show: (r: R) => string,
  options: readonly string[],
  input: (v: string | null) => DictInput,
): DictColumn<R> {
  return {
    key,
    title,
    width,
    show,
    parse: (text) => ok(input(matchOption(text, options)?.value ?? null)),
  };
}

function dateColumn<R extends DictRow>(
  key: "openedAt" | "closedAt",
  title: string,
  get: (r: R) => string | null,
): DictColumn<R> {
  const thisMonth = new Date().toISOString().slice(0, 7);
  return {
    key,
    title,
    width: 104,
    align: "right",
    show: (r) => {
      const v = get(r);
      return v === null ? "" : formatDate(v);
    },
    parse: (text) => {
      const parsed = parseDate(text, thisMonth);
      return parsed.ok ? ok({ field: key, value: parsed.value }) : parsed;
    },
  };
}

/** The six dictionaries, in the order of the sheet. */
export function specs(d: Dictionaries): DictSpec<DictRow>[] {
  const groupName = names(d.groups, (g) => g.name);
  const bankName = names(d.banks, (b) => b.name);
  const participantName = names(d.participants, (p) => p.name);
  const categoryName = names(d.categories, (c) => c.name);
  const productName = names(d.products, (p) => p.shortName);
  const groupNames = d.groups.map((g) => g.name);
  const bankNames = d.banks.map((b) => b.name);
  const participantNames = d.participants.map((p) => p.name);
  const categoryNames = d.categories.map((c) => c.name);
  const productNames = d.products.map((p) => p.shortName);

  const categories: DictSpec<(typeof d.categories)[number]> = {
    kind: "category",
    title: "Категории",
    hint: "«Назначение» в журнале. Порядок — порядок колонок в итогах.",
    rows: (x) => x.categories,
    columns: [
      nameColumn("Название", 200, (r) => r.name),
      refColumn(
        "group",
        "Группа",
        200,
        (r) => groupName(r.groupId),
        groupNames,
        (v) => ({
          field: "group",
          value: v,
        }),
      ),
      flagColumn("isRegularDefault", "Регулярная", (r) => r.isRegularDefault),
      orderColumn(),
      flagColumn("isActive", "Активна", (r) => r.isActive),
    ],
  };

  const groups: DictSpec<(typeof d.groups)[number]> = {
    kind: "group",
    title: "Группы",
    hint: "Объединяют категории в итогах: «Лечение/лекарства» = лечение + лекарства.",
    rows: (x) => x.groups,
    columns: [
      nameColumn("Название", 240, (r) => r.name),
      {
        key: "members",
        title: "Категории группы",
        width: 320,
        show: (r) =>
          d.categories
            .filter((c) => c.groupId === r.id)
            .map((c) => c.name)
            .join(", "),
      },
      orderColumn(),
    ],
  };

  const participants: DictSpec<(typeof d.participants)[number]> = {
    kind: "participant",
    title: "Участники",
    hint: "Кто платил: колонка «Участник» в журнале.",
    rows: (x) => x.participants,
    columns: [
      nameColumn("Имя", 200, (r) => r.name),
      orderColumn(),
      flagColumn("isActive", "Активен", (r) => r.isActive),
    ],
  };

  const banks: DictSpec<(typeof d.banks)[number]> = {
    kind: "bank",
    title: "Банки",
    hint: "Банки карт и счетов.",
    rows: (x) => x.banks,
    columns: [
      nameColumn("Название", 240, (r) => r.name),
      textColumn(
        "shortName",
        "Коротко",
        140,
        (r) => r.shortName,
        (v) => ({
          field: "shortName",
          value: v,
        }),
      ),
    ],
  };

  const products: DictSpec<(typeof d.products)[number]> = {
    kind: "product",
    title: "Карты и счета",
    hint: "Колонка «Карта» в журнале. Округление кэшбэка — как его считает банк этой карты.",
    rows: (x) => x.products,
    columns: [
      nameColumn("Как в журнале", 150, (r) => r.shortName),
      refColumn(
        "bank",
        "Банк",
        160,
        (r) => bankName(r.bankId),
        bankNames,
        (v) => ({
          field: "bank",
          value: v,
        }),
      ),
      refColumn(
        "participant",
        "Участник",
        110,
        (r) => participantName(r.participantId),
        participantNames,
        (v) => ({ field: "participant", value: v }),
      ),
      {
        key: "kind",
        title: "Тип",
        width: 150,
        show: (r) => (r.kind === null ? "" : KIND_LABELS[r.kind]),
        parse: (text) => {
          const parsed = parseLabel(text, KIND_LABELS, "Тип");
          return parsed.ok ? ok({ field: "kind", value: parsed.value }) : parsed;
        },
      },
      {
        key: "rounding",
        title: "Округление кэшбэка",
        width: 160,
        show: (r) => ROUNDING_LABELS[r.cashbackRounding],
        parse: (text) => {
          const parsed = parseLabel(text, ROUNDING_LABELS, "Округление кэшбэка");
          if (!parsed.ok) return parsed;
          return ok({ field: "rounding", value: parsed.value ?? "floor_rub" });
        },
      },
      textColumn(
        "fullName",
        "Полное название",
        200,
        (r) => r.fullName,
        (v) => ({
          field: "fullName",
          value: v,
        }),
      ),
      dateColumn("openedAt", "Открыта", (r) => r.openedAt),
      dateColumn("closedAt", "Закрыта", (r) => r.closedAt),
      flagColumn("isActive", "Активна", (r) => r.isActive),
      textColumn(
        "notes",
        "Заметки",
        220,
        (r) => r.notes,
        (v) => ({ field: "notes", value: v }),
      ),
    ],
  };

  const templates: DictSpec<(typeof d.templates)[number]> = {
    kind: "template",
    title: "Регулярные платежи",
    hint: "Шаблоны строк, которые вносятся каждый месяц кнопкой «Внести». «Не в Ср. %» — строки журнала с этим наименованием не входят в базу «Ср. %», как квартплата.",
    rows: (x) => x.templates,
    columns: [
      nameColumn("Наименование", 220, (r) => r.title),
      refColumn(
        "category",
        "Назначение",
        140,
        (r) => categoryName(r.categoryId),
        categoryNames,
        (v) => ({
          field: "category",
          value: v,
        }),
      ),
      refColumn(
        "participant",
        "Участник",
        110,
        (r) => participantName(r.participantId),
        participantNames,
        (v) => ({ field: "participant", value: v }),
      ),
      refColumn(
        "card",
        "Карта",
        130,
        (r) => productName(r.productId),
        productNames,
        (v) => ({
          field: "card",
          value: v,
        }),
      ),
      {
        key: "defaultAmount",
        title: "Сумма",
        width: 120,
        align: "right",
        show: (r) => (r.defaultAmount === null ? "" : formatMoney(r.defaultAmount)),
        edit: (r) => (r.defaultAmount === null ? "" : moneyEditText(r.defaultAmount)),
        parse: (text) => {
          if (text.trim() === "") return ok({ field: "defaultAmount", value: null });
          const parsed = parseMoney(text);
          return parsed.ok ? ok({ field: "defaultAmount", value: parsed.value }) : parsed;
        },
      },
      flagColumn("excludeFromRate", "Не в Ср. %", (r) => r.excludeFromRate, 112),
      orderColumn(),
      flagColumn("isActive", "Активен", (r) => r.isActive),
    ],
  };

  return [
    categories,
    groups,
    participants,
    banks,
    products,
    templates,
  ] as unknown as DictSpec<DictRow>[];
}
