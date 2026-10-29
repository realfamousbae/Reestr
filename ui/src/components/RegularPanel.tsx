import type { Dictionaries, EntryView } from "../api";
import { formatMoney } from "../format/money";
import { nameKey } from "../format/text";
import styles from "./RegularPanel.module.css";
import { Block } from "./SideBlocks";

interface Props {
  dictionaries: Dictionaries;
  entries: readonly EntryView[];
  onAdd: (templateIds: number[]) => void;
  onEditTemplates: () => void;
}

/**
 * «Регулярные платежи»: the templates, each with «Внести» to add it as a row of the month.
 * A template already present in the month (same title) is marked as entered.
 */
export function RegularPanel({ dictionaries, entries, onAdd, onEditTemplates }: Props) {
  const templates = dictionaries.templates.filter((t) => t.isActive);
  const present = new Set(entries.map((e) => nameKey(e.title)));
  const cardName = (id: number | null) =>
    dictionaries.products.find((p) => p.id === id)?.shortName ?? null;
  const missing = templates.filter((t) => !present.has(nameKey(t.title))).map((t) => t.id);

  return (
    <Block
      id="regular"
      title="Регулярные платежи"
      actions={
        missing.length > 0 && (
          <button
            type="button"
            className={styles.all}
            onClick={() => {
              onAdd(missing);
            }}
          >
            Внести недостающие
          </button>
        )
      }
    >
      {templates.length === 0 ? (
        <p className={styles.empty}>
          Шаблонов пока нет.{" "}
          <button type="button" className={styles.link} onClick={onEditTemplates}>
            Добавить в справочниках
          </button>
        </p>
      ) : (
        <ul className={styles.list}>
          {templates.map((t) => {
            const done = present.has(nameKey(t.title));
            const card = cardName(t.productId);
            return (
              <li key={t.id} className={done ? styles.done : undefined}>
                <span className={styles.title}>{t.title}</span>
                <span className={styles.meta}>
                  {t.defaultAmount !== null && <span>{formatMoney(t.defaultAmount)}</span>}
                  {card !== null && <span>{card}</span>}
                </span>
                {done ? (
                  <span className={styles.mark}>внесён</span>
                ) : (
                  <button
                    type="button"
                    className={styles.add}
                    onClick={() => {
                      onAdd([t.id]);
                    }}
                  >
                    Внести
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </Block>
  );
}
