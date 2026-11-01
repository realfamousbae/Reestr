import { useState } from "react";

import { type FileInfo, loadBackend } from "./api";
import { StartScreen } from "./screens/StartScreen";
import { WorkbookScreen } from "./screens/WorkbookScreen";

const backend = loadBackend();

export function App() {
  const [file, setFile] = useState<FileInfo | null>(null);

  if (backend === null) {
    return (
      <p role="alert" style={{ padding: "var(--s5)" }}>
        Это окно — часть программы Реестр. Запустите её через «pnpm dev» или установщик.
      </p>
    );
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
