import { useMemo, useState } from "react";

import type { CreditCardView, ProductOp, ProductsView, SavingsView } from "../api";
import { formatDate, parseDate, todayIso } from "../format/date";
import { formatMoney, moneyEditText, parseMoney } from "../format/money";
import { type Parsed, fail, ok } from "../format/parsed";
import { formatPercent, parsePercent, percentEditText } from "../format/percent";
import { plural } from "../format/text";
import type { CellChange } from "../grid/edits";
import { TableGrid } from "../grid/TableGrid";
import { availableCheckText, savingsText } from "./labels";
import styles from "./ProductsSheet.module.css";
import {
  type Batch,
  bindingOps,
  purchaseColumns,
  purchaseRows,
  repaymentColumns,
  repaymentOps,
  trancheColumns,
  trancheOps,
} from "./tables";
import { TermField } from "../components/TermField";

interface Props {
  view: ProductsView;
  /** "YYYY-MM": short dates typed in the tables are read in this month. */
  month: string;
  fontSize: number;
  onOps: (ops: ProductOp[]) => void;
  onErrors: (messages: string[]) => void;
  /** Rows deleted with Delete; the parent confirms. */
  onDelete: (what: string, ops: ProductOp[]) => void;
  onOpenDictionaries: () => void;
}

type Selected = { kind: "credit"; id: number } | { kind: "savings"; id: number };

function parseOptionalMoney(text: string): Parsed<number | null> {
  if (text.trim() === "") return ok(null);
  const parsed = parseMoney(text);
  if (!parsed.ok) return parsed;
  return parsed.value < 0 ? fail("Лимит не может быть отрицательным") : ok(parsed.value);
}

/** The balance as the bank shows it: over the limit it can be below zero. */
function parseAvailable(text: string): Parsed<number | null> {
  return text.trim() === "" ? ok(null) : parseMoney(text);
}

function parseDays(text: string): Parsed<number | null> {
  const t = text.trim();
  if (t === "") return ok(null);
  const n = Number(t);
  return Number.isInteger(n) && n > 0 ? ok(n) : fail("Число дней — целое и больше нуля");
}

/**
 * «Продукты» (PLAN §7): credit cards with their tranches, repayments and purchases, savings
 * accounts with their promo windows. A card becomes a credit card or a savings account by its
 * type in «Справочники».
 */
export function ProductsSheet({
  view,
  month,
  fontSize,
  onOps,
  onErrors,
  onDelete,
  onOpenDictionaries,
}: Props) {
  const first: Selected | null =
    view.credit[0] !== undefined
      ? { kind: "credit", id: view.credit[0].productId }
      : view.savings[0] !== undefined
        ? { kind: "savings", id: view.savings[0].productId }
        : null;
  const [selected, setSelected] = useState<Selected | null>(first);
  const current = selected ?? first;
  const credit =
    current?.kind === "credit" ? view.credit.find((c) => c.productId === current.id) : undefined;
  const savings =
    current?.kind === "savings" ? view.savings.find((s) => s.productId === current.id) : undefined;

  return (
    <div className={styles.sheet}>
      <nav className={styles.nav} aria-label="Продукты">
        <h2>Кредитные карты</h2>
        {view.credit.length === 0 && <p className={styles.empty}>Нет</p>}
        {view.credit.map((c) => (
          <button
            key={c.productId}
            type="button"
            aria-current={credit?.productId === c.productId ? "page" : undefined}
            onClick={() => {
              setSelected({ kind: "credit", id: c.productId });
            }}
          >
            <span>{c.card}</span>
            <span className={styles.count}>{c.debt === 0 ? "" : formatMoney(c.debt)}</span>
          </button>
        ))}
        <h2>Накопительные и вклады</h2>
        {view.savings.length === 0 && <p className={styles.empty}>Нет</p>}
        {view.savings.map((s) => (
          <button
            key={s.productId}
            type="button"
            aria-current={savings?.productId === s.productId ? "page" : undefined}
            onClick={() => {
              setSelected({ kind: "savings", id: s.productId });
            }}
          >
            <span>{s.card}</span>
            <span className={styles.count}>{s.state.state === "canOpen" ? "можно" : ""}</span>
          </button>
        ))}
        <p className={styles.hint}>
          Карта попадает сюда по типу: «кредитная», «накопительный счёт» или «вклад».{" "}
          <button type="button" className={styles.link} onClick={onOpenDictionaries}>
            Типы карт — в справочниках
          </button>
        </p>
        <p className={styles.total}>
          Долг по кредиткам: <strong>{formatMoney(view.debt)} ₽</strong>
        </p>
      </nav>
      {credit !== undefined && (
        <CreditPanel
          key={credit.productId}
          card={credit}
          month={month}
          fontSize={fontSize}
          onOps={onOps}
          onErrors={onErrors}
          onDelete={onDelete}
        />
      )}
      {savings !== undefined && (
        <SavingsPanel key={savings.productId} account={savings} onOps={onOps} onErrors={onErrors} />
      )}
      {credit === undefined && savings === undefined && (
        <section className={styles.panel}>
          <p className={styles.hint}>
            Отметьте в «Справочниках» → «Карты и счета» тип карты: «кредитная» — для грейсов,
            «накопительный счёт» или «вклад» — для сроков открытия.
          </p>
        </section>
      )}
    </div>
  );
}

function Owner({ bank, participant }: { bank: string | null; participant: string | null }) {
  const parts = [bank, participant].filter((p): p is string => p !== null);
  return parts.length === 0 ? null : <span className={styles.owner}>{parts.join(" · ")}</span>;
}

interface CreditProps {
  card: CreditCardView;
  month: string;
  fontSize: number;
  onOps: (ops: ProductOp[]) => void;
  onErrors: (messages: string[]) => void;
  onDelete: (what: string, ops: ProductOp[]) => void;
}

function CreditPanel({ card, month, fontSize, onOps, onErrors, onDelete }: CreditProps) {
  const tranches = useMemo(() => trancheColumns(month), [month]);
  const repayments = useMemo(() => repaymentColumns(card, month), [card, month]);
  const purchases = useMemo(() => purchaseColumns(card), [card]);
  const submit = (batch: Batch) => {
    onErrors(batch.errors);
    if (batch.ops.length > 0) onOps(batch.ops);
  };
  const error = (message: string) => {
    onErrors([message]);
  };
  const terms = (edit: ProductOp & { op: "creditTerms" }) => {
    onOps([edit]);
  };
  const available = (amount: number | null, on: string | null) => {
    terms({
      op: "creditTerms",
      product: card.productId,
      edit: { field: "manualAvailable", value: { amount, on } },
    });
  };

  return (
    <section className={styles.panel} aria-label={card.card}>
      <header className={styles.head}>
        <h2>{card.card}</h2>
        <Owner bank={card.bank} participant={card.participant} />
        <span className={styles.debt}>
          Долг: <strong>{formatMoney(card.debt)} ₽</strong>
        </span>
      </header>
      <div className={styles.terms}>
        <TermField
          label="Кредитный лимит, ₽"
          value={card.creditLimit === null ? "" : moneyEditText(card.creditLimit)}
          parse={parseOptionalMoney}
          onSave={(value) => {
            terms({
              op: "creditTerms",
              product: card.productId,
              edit: { field: "creditLimit", value },
            });
          }}
          onError={error}
        />
        <TermField
          label="Грейс обычно, дней"
          value={card.graceDays === null ? "" : card.graceDays.toString()}
          parse={parseDays}
          width={80}
          onSave={(value) => {
            terms({
              op: "creditTerms",
              product: card.productId,
              edit: { field: "graceDays", value },
            });
          }}
          onError={error}
        />
        <TermField
          label="Доступный остаток, ₽"
          value={card.manualAvailable === null ? "" : moneyEditText(card.manualAvailable)}
          parse={parseAvailable}
          onSave={(value) => {
            // The date goes with the value: today unless given. Clearing the value clears both.
            available(value, value === null ? null : (card.manualAvailableOn ?? todayIso()));
          }}
          onError={error}
        />
        <TermField
          label="на дату"
          value={card.manualAvailableOn === null ? "" : formatDate(card.manualAvailableOn)}
          parse={(text) => parseDate(text, todayIso().slice(0, 7))}
          placeholder="ДД.ММ.ГГГГ"
          onSave={(value) => {
            available(card.manualAvailable, value);
          }}
          onError={error}
        />
        <button
          type="button"
          className={styles.primary}
          onClick={() => {
            onOps([{ op: "addTranche", product: card.productId, edits: [] }]);
          }}
        >
          Начался новый грейс
        </button>
      </div>
      {card.available !== null && (
        <p className={card.available.difference === 0 ? styles.check : styles.checkDiffers}>
          {availableCheckText(card.available)}
        </p>
      )}
      {card.available === null && card.manualAvailable !== null && (
        <p className={styles.check}>Для сверки с банком укажите кредитный лимит.</p>
      )}

      <h3 className={styles.caption}>
        Грейсы <span>— даты из выписки банка; покупки попадают в грейс по своей дате</span>
      </h3>
      <div className={styles.grid}>
        <TableGrid
          columns={tranches}
          rows={card.tranches}
          fontSize={fontSize}
          newRowHint="Новый грейс"
          onChanges={(changes: CellChange[]) => {
            submit(trancheOps(changes, card, tranches));
          }}
          onDeleteRows={(ids) => {
            onDelete(
              `${ids.length.toString()} ${plural(ids.length, "грейс", "грейса", "грейсов")}`,
              [{ op: "deleteTranches", ids }],
            );
          }}
        />
      </div>

      <div className={styles.pair}>
        <div className={styles.half}>
          <h3 className={styles.caption}>
            Погашения <span>— без грейса идут в тот, что кончается раньше</span>
          </h3>
          <div className={styles.grid}>
            <TableGrid
              columns={repayments}
              rows={card.repayments}
              fontSize={fontSize}
              newRowHint="Новое погашение"
              onChanges={(changes: CellChange[]) => {
                submit(repaymentOps(changes, card, repayments));
              }}
              onDeleteRows={(ids) => {
                onDelete(
                  `${ids.length.toString()} ${plural(ids.length, "погашение", "погашения", "погашений")}`,
                  [{ op: "deleteRepayments", ids }],
                );
              }}
            />
          </div>
        </div>
        <div className={styles.half}>
          <h3 className={styles.caption}>
            Покупки по карте{" "}
            <span>
              — в непогашенных грейсах и недавние без грейса
              {card.unbound > 0 ? ` (без грейса: ${card.unbound.toString()})` : ""}
            </span>
          </h3>
          <div className={styles.grid}>
            <TableGrid
              columns={purchases}
              rows={purchaseRows(card)}
              fontSize={fontSize}
              onChanges={(changes: CellChange[]) => {
                submit(bindingOps(changes, card, purchases));
              }}
              onDeleteRows={() => {
                onErrors(["Покупки удаляются в журнале месяца"]);
              }}
            />
          </div>
        </div>
      </div>
    </section>
  );
}

interface SavingsProps {
  account: SavingsView;
  onOps: (ops: ProductOp[]) => void;
  onErrors: (messages: string[]) => void;
}

function SavingsPanel({ account, onOps, onErrors }: SavingsProps) {
  const product = account.productId;
  const error = (message: string) => {
    onErrors([message]);
  };
  const thisMonth = todayIso().slice(0, 7);
  return (
    <section className={`${styles.panel} ${styles.simple}`} aria-label={account.card}>
      <header className={styles.head}>
        <h2>{account.card}</h2>
        <Owner bank={account.bank} participant={account.participant} />
        <span className={styles.debt}>{savingsText(account.state)}</span>
      </header>
      <div className={styles.terms}>
        <TermField
          label="Ставка по акции, %"
          value={account.promoRateBp === null ? "" : percentEditText(account.promoRateBp)}
          parse={parsePercent}
          width={80}
          onSave={(value) => {
            onOps([{ op: "savingsTerms", product, edit: { field: "promoRate", value } }]);
          }}
          onError={error}
        />
        <TermField
          label="Закрыт"
          value={account.closedAt === null ? "" : formatDate(account.closedAt)}
          parse={(text) => parseDate(text, thisMonth)}
          placeholder="ДД.ММ.ГГГГ"
          onSave={(value) => {
            onOps([{ op: "savingsTerms", product, edit: { field: "closedAt", value } }]);
          }}
          onError={error}
        />
        <TermField
          label="Снова открыть через, дней"
          value={account.reopenAfterDays === null ? "" : account.reopenAfterDays.toString()}
          parse={parseDays}
          width={80}
          onSave={(value) => {
            onOps([{ op: "savingsTerms", product, edit: { field: "reopenAfterDays", value } }]);
          }}
          onError={error}
        />
      </div>
      <p className={styles.hint}>
        Условие вроде «нет накопительных счетов и вкладов 90+ дней»: укажите дату закрытия и число
        дней — «Сроки» напомнят, когда счёт можно открыть снова
        {account.promoRateBp === null ? "" : ` под ${formatPercent(account.promoRateBp)}`}.
      </p>
    </section>
  );
}
