import { useEffect, useState } from "react";

import { type FileInfo, loadBackend } from "./api";
import { PhoneApp } from "./phone/PhoneApp";
import { StartScreen } from "./screens/StartScreen";
import { WorkbookScreen } from "./screens/WorkbookScreen";

const backend = loadBackend();

/** The phone's screens can be tried on a computer: localStorage "reestr.phone" = "1". */
function phoneTried(): boolean {
  try {
    return localStorage.getItem("reestr.phone") === "1";
  } catch {
    return false;
  }
}

export function App() {
  const [file, setFile] = useState<FileInfo | null>(null);
  const [platform, setPlatform] = useState<string | null>(null);

  useEffect(() => {
    backend
      ?.appPlatform()
      .then(setPlatform)
      .catch(() => {
        setPlatform("unknown");
      });
  }, []);

  if (backend === null) {
    return (
      <p role="alert" style={{ padding: "var(--s5)" }}>
        Это окно — часть программы Реестр. Запустите её через «pnpm dev» или установщик.
      </p>
    );
  }
  if (platform === null) {
    return null;
  }
  if (platform === "android" || phoneTried()) {
    return <PhoneApp backend={backend} android={platform === "android"} />;
  }
  if (file === null) {
    return <StartScreen backend={backend} onOpened={setFile} />;
  }
  return (
    <WorkbookScreen
      backend={backend}
      file={file}
      onClose={() => {
        setFile(null);
      }}
    />
  );
}
