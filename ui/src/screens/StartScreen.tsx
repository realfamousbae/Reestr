import { type FormEvent, useState } from "react";

import { type Backend, type FileInfo, errorMessage } from "../api";
import { todayIso } from "../format/date";
import { passwordProblem } from "../format/password";
import styles from "./StartScreen.module.css";

interface Props {
  backend: Backend;
  onOpened: (file: FileInfo) => void;
}

type Step = { kind: "choose" } | { kind: "create"; path: string } | { kind: "open"; path: string };

const LAST_FILE_KEY = "reestr.lastFile";

function readLastFile(): string | null {
  try {
    return localStorage.getItem(LAST_FILE_KEY);
  } catch {
    return null;
  }
}

function rememberLastFile(path: string): void {
  try {
    localStorage.setItem(LAST_FILE_KEY, path);
  } catch {
    // Not remembering the path only costs a click next time.
  }
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

export function StartScreen({ backend, onOpened }: Props) {
  const [step, setStep] = useState<Step>({ kind: "choose" });
  const [error, setError] = useState<string | null>(null);
  const lastFile = readLastFile();

  const choose = (pick: () => Promise<string | null>, kind: "create" | "open") => {
    setError(null);
    pick()
      .then((path) => {
        if (path !== null) setStep({ kind, path });
      })
      .catch((err: unknown) => {
        setError(errorMessage(err));
      });
  };

  const opened = (file: FileInfo) => {
    rememberLastFile(file.path);
    onOpened(file);
  };

  return (
    <div className={styles.screen}>
      <div className={styles.page}>
        <h1 className={styles.title}>Реестр</h1>
        <p className={styles.lead}>
          Семейный журнал трат, карт и кэшбэка. Все данные — в одном зашифрованном файле.
        </p>

        {step.kind === "choose" && (
          <div className={styles.actions}>
            <button
              type="button"
              className={styles.primary}
              onClick={() => {
                choose(() => backend.pickNewFile(), "create");
              }}
            >
              Создать файл
            </button>
            <button
              type="button"
              className={styles.secondary}
              onClick={() => {
                choose(() => backend.pickExistingFile(), "open");
              }}
            >
              Открыть файл…
            </button>
            {lastFile !== null && (
              <button
                type="button"
                className={styles.recent}
                onClick={() => {
                  setStep({ kind: "open", path: lastFile });
                }}
                title={lastFile}
              >
                Открыть «{fileName(lastFile)}»
              </button>
            )}
          </div>
        )}

        {step.kind === "create" && (
          <CreateForm
            path={step.path}
            onBack={() => {
              setStep({ kind: "choose" });
            }}
            onSubmit={async (password) => {
              opened(await backend.vaultCreate(step.path, password));
            }}
          />
        )}

        {step.kind === "open" && (
          <OpenForm
            path={step.path}
            onBack={() => {
              setStep({ kind: "choose" });
            }}
            onSubmit={async (password) => {
              opened(await backend.vaultOpen(step.path, password, todayIso()));
            }}
          />
        )}

        {error !== null && (
          <p className={styles.error} role="alert">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}

interface FormProps {
  path: string;
  onBack: () => void;
  onSubmit: (password: string) => Promise<void>;
}

function CreateForm({ path, onBack, onSubmit }: FormProps) {
  const [password, setPassword] = useState("");
  const [repeat, setRepeat] = useState("");
  const [show, setShow] = useState(false);
  const [written, setWritten] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const problem = passwordProblem(password, repeat, written);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (problem !== null) {
      setError(problem);
      return;
    }
    setBusy(true);
    setError(null);
    onSubmit(password).catch((err: unknown) => {
      setError(errorMessage(err));
      setBusy(false);
    });
  };

  return (
    <form className={styles.form} onSubmit={submit} noValidate>
      <h2 className={styles.heading}>Новый файл</h2>
      <p className={styles.path} title={path}>
        {path}
      </p>

      <div className={styles.warning}>
        <p>
          <strong>Пароль нельзя восстановить.</strong> Без него файл не откроет никто, в том числе
          сама программа. Запишите пароль на бумаге и храните её отдельно от компьютера.
        </p>
      </div>

      <label className={styles.field}>
        <span>Пароль</span>
        <input
          type={show ? "text" : "password"}
          autoComplete="new-password"
          value={password}
          onChange={(e) => {
            setPassword(e.target.value);
          }}
          autoFocus
        />
      </label>
      <label className={styles.field}>
        <span>Повторите пароль</span>
        <input
          type={show ? "text" : "password"}
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
        Показать пароль
      </label>
      <label className={styles.check}>
        <input
          type="checkbox"
          checked={written}
          onChange={(e) => {
            setWritten(e.target.checked);
          }}
        />
        Пароль записан на бумаге
      </label>

      {error !== null && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}

      <div className={styles.buttons}>
        <button type="submit" className={styles.primary} disabled={busy}>
          {busy ? "Создаю файл…" : "Создать файл"}
        </button>
        <button type="button" className={styles.secondary} onClick={onBack} disabled={busy}>
          Назад
        </button>
      </div>
    </form>
  );
}

function OpenForm({ path, onBack, onSubmit }: FormProps) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (password === "") {
      setError("Введите пароль.");
      return;
    }
    setBusy(true);
    setError(null);
    onSubmit(password).catch((err: unknown) => {
      setError(errorMessage(err));
      setBusy(false);
    });
  };

  return (
    <form className={styles.form} onSubmit={submit} noValidate>
      <h2 className={styles.heading}>Открыть «{fileName(path)}»</h2>
      <p className={styles.path} title={path}>
        {path}
      </p>
      <label className={styles.field}>
        <span>Пароль</span>
        <input
          type="password"
          autoComplete="current-password"
          value={password}
          onChange={(e) => {
            setPassword(e.target.value);
          }}
          autoFocus
        />
      </label>

      {error !== null && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}

      <div className={styles.buttons}>
        <button type="submit" className={styles.primary} disabled={busy}>
          {busy ? "Открываю…" : "Открыть"}
        </button>
        <button type="button" className={styles.secondary} onClick={onBack} disabled={busy}>
          Назад
        </button>
      </div>
    </form>
  );
}
