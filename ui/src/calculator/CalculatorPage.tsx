import { useEffect, useId, useRef, useState } from "react";

import { type Backend, type CalcView, errorMessage } from "../api";
import { formatMoney } from "../format/money";
import { formatPercent } from "../format/percent";
import { plural } from "../format/text";
import styles from "./CalculatorPage.module.css";
import { type CalcField, type CalcFields, type CalcRequest, parseCalcFields } from "./input";

interface Props {
  backend: Backend;
  onClose: () => void;
}

const EMPTY: CalcFields = { amount: "", cashback: "", savings: "", days: "" };

/** One sentence naming the better option and by how much. */
function verdict(view: CalcView): string {
  const gap = formatMoney(Math.abs(view.difference));
  switch (view.better) {
    case "cashback":
      return `Выгоднее заплатить картой: кэшбэк больше процентов на ${gap} ₽.`;
    case "savings":
      return `Выгоднее оставить деньги на накопительном: проценты больше кэшбэка на ${gap} ₽.`;
    case "equal":
      return "Всё равно: кэшбэк и проценты одинаковы.";
  }
}

/**
 * «Калькулятор»: pay now by a card with cashback, or keep the money on a savings account for the
 * term. Nothing is stored. The arithmetic is the core's; this screen parses the fields, asks the
 * command again on every change and shows the answer.
 */
export function CalculatorPage({ backend, onClose }: Props) {
  const [fields, setFields] = useState<CalcFields>(EMPTY);
  const [result, setResult] = useState<CalcView | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  /** Numbers the requests, so a slow answer to an old input never replaces a newer one. */
  const latest = useRef(0);
  const ids = { amount: useId(), cashback: useId(), savings: useId(), days: useId() };

  const parsed = parseCalcFields(fields);
  const request = parsed.request;

  // A stable key: `request` is a new object on every render, its numbers are what matters.
  const key = request === null ? null : JSON.stringify(request);

  useEffect(() => {
    const id = ++latest.current;
    if (key === null) {
      setResult(null);
      setFailure(null);
      return;
    }
    const r = JSON.parse(key) as CalcRequest;
    backend
      .calculatorCompare(r.amount, r.cashbackRate, r.savingsRate, r.days)
      .then((view) => {
        if (id !== latest.current) return;
        setResult(view);
        setFailure(null);
      })
      .catch((error: unknown) => {
        if (id !== latest.current) return;
        setResult(null);
        setFailure(errorMessage(error));
      });
  }, [backend, key]);

  const input = (
    name: CalcField,
    label: string,
    hint: string,
    inputMode: "decimal" | "numeric",
  ) => (
    <div className={styles.field}>
      <label htmlFor={ids[name]}>{label}</label>
      <input
        id={ids[name]}
        type="text"
        inputMode={inputMode}
        autoComplete="off"
        value={fields[name]}
        aria-invalid={parsed.errors[name] !== undefined}
        aria-describedby={`${ids[name]}-hint`}
        onChange={(e) => {
          setFields({ ...fields, [name]: e.target.value });
        }}
      />
      <span
        id={`${ids[name]}-hint`}
        className={parsed.errors[name] === undefined ? styles.hint : styles.error}
      >
        {parsed.errors[name] ?? hint}
      </span>
    </div>
  );

  const days = request?.days ?? 0;
  return (
    <div className={styles.page}>
      <header className={styles.bar}>
        <h2>Калькулятор</h2>
        <span className={styles.lead}>
          Заплатить картой с кэшбэком или оставить деньги на накопительном
        </span>
        <button type="button" className={styles.button} onClick={onClose}>
          Закрыть
        </button>
      </header>
      <div className={styles.body}>
        <form
          className={styles.form}
          onSubmit={(e) => {
            e.preventDefault();
          }}
          noValidate
        >
          {input("amount", "Сумма, ₽", "Покупка, она же сумма на накопительном", "decimal")}
          {input("cashback", "Кэшбэк, %", "Например 5 или 1,5", "decimal")}
          {input("savings", "Накопительный, % годовых", "Например 16", "decimal")}
          {input("days", "Срок, дней", "Сколько дней деньги лежали бы на накопительном", "numeric")}
        </form>

        <section className={styles.result} aria-labelledby="calc-result" aria-live="polite">
          <h3 id="calc-result">Результат</h3>
          {failure !== null && (
            <p className={styles.error} role="alert">
              {failure}
            </p>
          )}
          {result === null && failure === null && (
            <p className={styles.hint}>Заполните все четыре поля — результат появится здесь.</p>
          )}
          {result !== null && (
            <>
              <p className={styles.verdict}>{verdict(result)}</p>
              <dl className={styles.facts}>
                <dt>Кэшбэк</dt>
                <dd>{formatMoney(result.cashback)} ₽</dd>
                <dt>Проценты за срок</dt>
                <dd>{formatMoney(result.interest)} ₽</dd>
                <dt>Разница (кэшбэк − проценты)</dt>
                <dd>{formatMoney(result.difference)} ₽</dd>
                <dt>Ставка равновесия</dt>
                <dd>
                  {result.breakEvenBp === null
                    ? "не определена"
                    : `${formatPercent(result.breakEvenBp)} годовых`}
                </dd>
              </dl>
              <p className={styles.hint}>
                {result.breakEvenBp === null
                  ? "Ставка равновесия не определена при сроке 0 дней или нулевой сумме."
                  : `Если накопительный даёт больше ${formatPercent(result.breakEvenBp)} годовых, то за ${days.toString()} ${plural(days, "день", "дня", "дней")} проценты перекроют кэшбэк.`}
              </p>
            </>
          )}
        </section>
        <p className={styles.note}>
          Налог и капитализация не учитываются: проценты простые, по формуле сумма × ставка × дни /
          365. Кэшбэк округляется вниз до рубля, как в правилах кэшбэка, проценты — до копейки.
        </p>
      </div>
    </div>
  );
}
