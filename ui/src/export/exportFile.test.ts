import { describe, expect, it } from "vitest";

import { baseName, exportName, withExtension } from "./exportFile";

describe("export file names", () => {
  it("names the export after the file and the day", () => {
    expect(exportName("Семья.reestr", "xlsx", "2026-10-27")).toBe("Семья 2026-10-27.xlsx");
    expect(exportName("Бюджет.REESTR", "json", "2026-10-27")).toBe("Бюджет 2026-10-27.json");
    expect(exportName(".reestr", "xlsx", "2026-10-27")).toBe("Реестр 2026-10-27.xlsx");
  });

  it("adds the extension only when it is missing", () => {
    expect(withExtension("/tmp/выгрузка", "xlsx")).toBe("/tmp/выгрузка.xlsx");
    expect(withExtension("/tmp/выгрузка.XLSX", "xlsx")).toBe("/tmp/выгрузка.XLSX");
    expect(withExtension("C:\\Users\\Олег\\копия.json", "json")).toBe(
      "C:\\Users\\Олег\\копия.json",
    );
  });

  it("shows the file name of a path", () => {
    expect(baseName("/home/oleg/Семья 2026-10-27.xlsx")).toBe("Семья 2026-10-27.xlsx");
    expect(baseName("C:\\Users\\Олег\\копия.json")).toBe("копия.json");
  });
});
