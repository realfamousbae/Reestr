import { type FormEvent, useEffect, useState } from "react";

import { type AboutView, type Backend, errorMessage } from "../api";
import { formatDate } from "../format/date";
import { passwordProblem } from "../format/password";
import { cipherText, createdText, formatSize } from "./about";
import styles from "./SettingsPage.module.css";
import {
  BLOCKS,
  DEFAULT_VIEW,
  FONT_SIZES,
  type ViewSettings,
  moveBlock,
  toggleBlock,
} from "./view";

interface Props {
  backend: Backend;
  view: ViewSettings;
  onViewChange: (view: ViewSettings) => void;
  onClose: () => void;
}

/** «Настройки»: the view of this computer (D34), the file, its copies (D32), its password (D33). */
export function SettingsPage({ backend, view, onViewChange, onClose }: Props) {
  const [about, setAbout] = useState<AboutView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    backend
      .fileAbout()
      .then(setAbout)
      .catch((e: unknown) => {
        setError(errorMessage(e));
      });
  }, [backend]);

  return (
    <div className={styles.page}>
      <header className={styles.bar}>
        <h2>Настройки</h2>
        <button type="button" className={styles.button} onClick={onClose}>
          Закрыть
        </button>
      </header>
      <div className={styles.body}>
        <ViewSection view={view} onChange={onViewChange} />
        {error !== null && (
          <p className={styles.error} role="alert">
            {error}
          </p>
        )}
        {about !== null && <FileSection about={about} />}
        {about !== null && <CopiesSection about={about} />}
        <PasswordSection backend={backend} />
      </div>
    </div>
  );
}

function ViewSection({
  view,
  onChange,
}: {
  view: ViewSettings;
  onChange: (view: ViewSettings) => void;
}) {
  const title = (id: string) => BLOCKS.find((b) => b.id === id)?.title ?? id;
  return (
    <section className={styles.section} aria-labelledby="view-heading">
      <h3 id="view-heading">Вид на этом компьютере</h3>
      <div className={styles.field}>
        <span id="font-label">Размер шрифта таблиц</span>
        <div className={styles.sizes} role="radiogroup" aria-labelledby="font-label">
          {FONT_SIZES.map((size) => (
            <button
              key={size}
              type="button"
              role="radio"
              aria-checked={view.fontSize === size}
              onClick={() => {
                onChange({ ...view, fontSize: size });
              }}
            >
              {size}
            </button>
          ))}
        </div>
        <p className={styles.sample} style={{ fontSize: view.fontSize }} aria-hidden="true">
          <span>Пятёрочка</span>
          <span>продукты</span>
          <span>05.07.2026</span>
          <span className={styles.sampleSum}>1 874,50</span>
        </p>
      </div>
      <div className={styles.field}>
        <label className={styles.check}>
          <input
            type="checkbox"
            checked={view.alwaysKopecks}
            onChange={(e) => {
              onChange({ ...view, alwaysKopecks: e.target.checked });
            }}
          />
          Всегда показывать копейки
        </label>
      </div>
      <div className={styles.field}>
        <span id="blocks-label">Блоки справа на листе месяца</span>
        <ol className={styles.blocks} aria-labelledby="blocks-label">
          {view.order.map((id, i) => (
            <li key={id}>
              <label className={styles.check}>
                <input
                  type="checkbox"
                  checked={!view.hidden.includes(id)}
                  onChange={() => {
                    onChange(toggleBlock(view, id));
                  }}
                />
                {title(id)}
              </label>
              <span className={styles.move}>
                <button
                  type="button"
                  aria-label={`${title(id)}: выше`}
                  disabled={i === 0}
                  onClick={() => {
                    onChange(moveBlock(view, id, -1));
                  }}
                >
                  ↑
                </button>
                <button
                  type="button"
                  aria-label={`${title(id)}: ниже`}
                  disabled={i === view.order.length - 1}
                  onClick={() => {
                    onChange(moveBlock(view, id, 1));
                  }}
                >
                  ↓
                </button>
              </span>
            </li>
          ))}
        </ol>
        <button
          type="button"
          className={styles.link}
          onClick={() => {
            onChange({
              ...DEFAULT_VIEW,
              fontSize: view.fontSize,
              alwaysKopecks: view.alwaysKopecks,
            });
          }}
        >
          Вернуть порядок, как в книге
        </button>
      </div>
    </section>
  );
}

function FileSection({ about }: { about: AboutView }) {
  const f = about.file;
  return (
    <section className={styles.section} aria-labelledby="file-heading">
      <h3 id="file-heading">О файле</h3>
      <dl className={styles.facts}>
        <dt>Файл</dt>
        <dd>{f.fileName}</dd>
        <dt>Где лежит</dt>
        <dd className={styles.path}>{f.path}</dd>
        <dt>Размер</dt>
        <dd>{formatSize(about.size)}</dd>
        <dt>Создан</dt>
        <dd>
          {createdText(f.createdAt)}, Реестр {f.createdWith || "—"}
        </dd>
        <dt>Версия данных</dt>
        <dd>{f.schemaVersion}</dd>
        <dt>Шифрование</dt>
        <dd>{cipherText(about.cipher)}</dd>
      </dl>
    </section>
  );
}

function CopiesSection({ about }: { about: AboutView }) {
  const copies = [...about.copies].reverse();
  return (
    <section className={styles.section} aria-labelledby="copies-heading">
      <h3 id="copies-heading">Копии</h3>
      {about.copiesDir === null ? (
        <p className={styles.note}>
          Этот файл открыт из папки копий, поэтому сам он копий не делает.
        </p>
      ) : (
        <>
          <p className={styles.note}>
            При первом открытии за день файл копируется в папку{" "}
            <span className={styles.path}>{about.copiesDir}</span>. Хранятся {about.copiesKept}{" "}
            последних копий. Копия зашифрована паролем, который был у файла в тот день.
          </p>
          {copies.length === 0 ? (
            <p className={styles.note}>Копий пока нет.</p>
          ) : (
            <table className={styles.table}>
              <thead>
                <tr>
                  <th>Дата</th>
                  <th>Файл</th>
                  <th className={styles.num}>Размер</th>
                </tr>
              </thead>
              <tbody>
                {copies.map((c) => (
                  <tr key={c.fileName}>
                    <td>{formatDate(c.date)}</td>
                    <td>{c.fileName}</td>
                    <td className={styles.num}>{formatSize(c.size)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          <p className={styles.note}>
            Чтобы вернуться к копии: нажмите «Закрыть файл», затем на первом экране — «Открыть
            файл…» и выберите копию. Если в ней всё верно, закройте программу и положите копию на
            место основного файла.
          </p>
        </>
      )}
    </section>
  );
}

function PasswordSection({ backend }: { backend: Backend }) {
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [repeat, setRepeat] = useState("");
  const [show, setShow] = useState(false);
  const [written, setWritten] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ text: string; ok: boolean } | null>(null);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const problem =
      current === "" ? "Введите текущий пароль." : passwordProblem(next, repeat, written);
    if (problem !== null) {
      setMessage({ text: problem, ok: false });
      return;
    }
    setBusy(true);
    setMessage(null);
    backend
      .changePassword(current, next)
      .then(() => {
        setCurrent("");
        setNext("");
        setRepeat("");
        setWritten(false);
        setMessage({
          text: "Пароль изменён. Копии, сделанные раньше, открываются старым паролем.",
          ok: true,
        });
      })
      .catch((err: unknown) => {
        setMessage({ text: errorMessage(err), ok: false });
      })
      .finally(() => {
        setBusy(false);
      });
  };

  const type = show ? "text" : "password";
  return (
    <section className={styles.section} aria-labelledby="password-heading">
      <h3 id="password-heading">Пароль</h3>
      <form className={styles.form} onSubmit={submit} noValidate>
        <div className={styles.warning}>
          <p>
            <strong>Новый пароль тоже нельзя восстановить.</strong> Запишите его на бумаге. Копии,
            сделанные раньше, останутся со старым паролем — не выбрасывайте и его запись.
          </p>
        </div>
        <label className={styles.input}>
          <span>Текущий пароль</span>
          <input
            type={type}
            autoComplete="current-password"
            value={current}
            onChange={(e) => {
              setCurrent(e.target.value);
            }}
          />
        </label>
        <label className={styles.input}>
          <span>Новый пароль</span>
          <input
            type={type}
            autoComplete="new-password"
            value={next}
            onChange={(e) => {
              setNext(e.target.value);
            }}
          />
        </label>
        <label className={styles.input}>
          <span>Новый пароль ещё раз</span>
          <input
            type={type}
            autoComplete="new-password"
            value={repeat}
            onChange={(e) => {
              setRepeat(e.target.value);
            }}
          />
        </label>
        <label className={styles.check}>
          <input
            type="checkbox"
            checked={show}
            onChange={(e) => {
              setShow(e.target.checked);
            }}
          />
          Показать пароли
        </label>
        <label className={styles.check}>
          <input
            type="checkbox"
            checked={written}
            onChange={(e) => {
              setWritten(e.target.checked);
            }}
          />
          Новый пароль записан на бумаге
        </label>
        {message !== null && (
          <p
            className={message.ok ? styles.ok : styles.error}
            role={message.ok ? "status" : "alert"}
          >
            {message.text}
          </p>
        )}
        <div>
          <button type="submit" className={styles.primary} disabled={busy}>
            {busy ? "Меняю пароль…" : "Сменить пароль"}
          </button>
        </div>
      </form>
    </section>
  );
}
