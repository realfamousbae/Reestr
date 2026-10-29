import { type FormEvent, useCallback, useEffect, useState } from "react";

import { type Backend, type PhoneOpened, type PhoneSetup, errorCode, errorMessage } from "../api";
import { PhoneHome } from "./PhoneHome";
import styles from "./Phone.module.css";

/** The file closes when the phone was put away this long: the next opening gets a new copy. */
const LOCK_AFTER_MS = 5 * 60 * 1000;

type Stage =
  | { kind: "loading" }
  | { kind: "setup"; setup: PhoneSetup }
  | { kind: "unlock"; setup: PhoneSetup }
  | { kind: "home"; setup: PhoneSetup; opened: PhoneOpened };

interface Props {
  backend: Backend;
  /** On Android the system picker; on a computer, where the screens are tried, a dialog. */
  android: boolean;
}

/** Reestr on the phone (D38–D40): the exchange files, the password, then the entry screen. */
export function PhoneApp({ backend, android }: Props) {
  const [stage, setStage] = useState<Stage>({ kind: "loading" });
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    backend
      .phoneSetup()
      .then((setup) => {
        setStage(
          setup.copy === null || setup.reply === null
            ? { kind: "setup", setup }
            : { kind: "unlock", setup },
        );
      })
      .catch((e: unknown) => {
        setError(errorMessage(e));
      });
  }, [backend]);

  const lock = useCallback(
    (setup: PhoneSetup) => {
      backend
        .vaultClose()
        .catch(() => {
          // The file is opened again with the password anyway.
        })
        .finally(() => {
          setStage({ kind: "unlock", setup });
        });
    },
    [backend],
  );

  // A phone put away for a while asks for the password again.
  useEffect(() => {
    if (stage.kind !== "home") return;
    let hiddenAt: number | null = null;
    const onVisibility = () => {
      if (document.hidden) {
        hiddenAt = Date.now();
      } else if (hiddenAt !== null && Date.now() - hiddenAt > LOCK_AFTER_MS) {
        lock(stage.setup);
      }
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [stage, lock]);

  if (error !== null) {
    return (
      <div className={styles.page}>
        <p className={styles.error} role="alert">
          {error}
        </p>
      </div>
    );
  }
  switch (stage.kind) {
    case "loading":
      return null;
    case "setup":
      return (
        <SetupScreen
          backend={backend}
          android={android}
          setup={stage.setup}
          onDone={(setup) => {
            setStage({ kind: "unlock", setup });
          }}
        />
      );
    case "unlock":
      return (
        <UnlockScreen
          backend={backend}
          setup={stage.setup}
          onOpened={(opened) => {
            setStage({ kind: "home", setup: stage.setup, opened });
          }}
          onFiles={() => {
            setStage({ kind: "setup", setup: stage.setup });
          }}
        />
      );
    case "home":
      return (
        <PhoneHome
          backend={backend}
          opened={stage.opened}
          onLock={() => {
            lock(stage.setup);
          }}
        />
      );
  }
}

function SetupScreen({
  backend,
  android,
  setup,
  onDone,
}: {
  backend: Backend;
  android: boolean;
  setup: PhoneSetup;
  onDone: (setup: PhoneSetup) => void;
}) {
  const [current, setCurrent] = useState(setup);
  const [error, setError] = useState<string | null>(null);

  const choose = (which: "copy" | "reply") => {
    void (async () => {
      setError(null);
      try {
        let path: string | null = null;
        if (!android) {
          path = await backend.pickAnyFile("Файл обмена с телефоном");
          if (path === null) return;
        }
        setCurrent(await backend.phoneChoose(which, path));
      } catch (e) {
        setError(errorMessage(e));
      }
    })();
  };

  const name = (picked: PhoneSetup["copy"]) =>
    picked === null ? "Не выбран" : `Выбран: ${picked.name ?? picked.uri}`;

  return (
    <div className={styles.page}>
      <h1>Реестр на телефоне</h1>
      <p>
        Строки, которые вы внесёте здесь, попадут в журнал на компьютере через Яндекс Диск. Файлы
        для обмена кладёт на Диск Реестр на компьютере: «Настройки → Телефон».
      </p>
      <ol className={styles.steps}>
        <li>
          <strong>Копия с компьютера</strong>
          <span className={styles.muted}>Файл «… — для телефона.reestr» на Яндекс Диске.</span>
          <span className={styles.picked}>{name(current.copy)}</span>
          <button
            type="button"
            className={styles.secondary}
            onClick={() => {
              choose("copy");
            }}
          >
            Выбрать файл
          </button>
        </li>
        <li>
          <strong>Файл для компьютера</strong>
          <span className={styles.muted}>Файл «… — с телефона.reestr» рядом с ним.</span>
          <span className={styles.picked}>{name(current.reply)}</span>
          <button
            type="button"
            className={styles.secondary}
            onClick={() => {
              choose("reply");
            }}
          >
            Выбрать файл
          </button>
        </li>
      </ol>
      {error !== null && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      <div className={styles.actions}>
        <button
          type="button"
          className={styles.primary}
          disabled={current.copy === null || current.reply === null}
          onClick={() => {
            onDone(current);
          }}
        >
          Дальше
        </button>
      </div>
    </div>
  );
}

function UnlockScreen({
  backend,
  setup,
  onOpened,
  onFiles,
}: {
  backend: Backend;
  setup: PhoneSetup;
  onOpened: (opened: PhoneOpened) => void;
  onFiles: () => void;
}) {
  const [password, setPassword] = useState("");
  const [other, setOther] = useState("");
  /** After a password change on the computer the second password is asked for. */
  const [ask, setAsk] = useState<{ which: "old" | "new"; first: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (password === "" && ask === null) {
      setError("Введите пароль.");
      return;
    }
    // Whatever was asked, the new password opens the copy and the old one the phone's file.
    const [first, second] =
      ask === null
        ? [password, null]
        : ask.which === "old"
          ? [ask.first, other]
          : [other, ask.first];
    setBusy(true);
    setError(null);
    backend
      .phoneOpen(first, second)
      .then((opened) => {
        setPassword("");
        setOther("");
        onOpened(opened);
      })
      .catch((err: unknown) => {
        const code = errorCode(err);
        if (ask === null && (code === "phone_old_password" || code === "phone_new_password")) {
          setAsk({ which: code === "phone_old_password" ? "old" : "new", first: password });
          setPassword("");
          return;
        }
        setError(errorMessage(err));
      })
      .finally(() => {
        setBusy(false);
      });
  };

  return (
    <form className={styles.page} onSubmit={submit} noValidate>
      <h1>Реестр</h1>
      <p className={styles.muted}>{setup.copy?.name ?? "Копия с компьютера"}</p>
      {ask === null ? (
        <label className={styles.field}>
          <span>Пароль</span>
          <input
            type="password"
            autoComplete="current-password"
            value={password}
            onChange={(e) => {
              setPassword(e.target.value);
            }}
          />
        </label>
      ) : (
        <>
          <p>
            {ask.which === "old"
              ? "На компьютере сменили пароль. Введите и прежний: им зашифрованы строки этого телефона."
              : "На компьютере сменили пароль. Введите новый."}
          </p>
          <label className={styles.field}>
            <span>{ask.which === "old" ? "Прежний пароль" : "Новый пароль"}</span>
            <input
              type="password"
              autoComplete="off"
              value={other}
              onChange={(e) => {
                setOther(e.target.value);
              }}
            />
          </label>
        </>
      )}
      {error !== null && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
      <div className={styles.actions}>
        <button type="submit" className={styles.primary} disabled={busy}>
          {busy ? "Открываю…" : "Открыть"}
        </button>
      </div>
      <p>
        <button type="button" className={styles.plain} onClick={onFiles}>
          Выбрать файлы обмена заново
        </button>
      </p>
    </form>
  );
}
