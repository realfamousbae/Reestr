"""Builds the synthetic workbooks used by the import and «Свод» acceptance tests.

The workbook is written from scratch in the layout the importer reads (PLAN §9): a «Служебный»
sheet with the categories, month sheets `MM.YY` with the journal in A–J from row 3 and SUMIF
totals in row 2, the «Итого:»/«Ср. %» cashback cells, and a «Свод» sheet with a block per year.
Every name, card and amount in it is fictional. LibreOffice then recalculates the formulas, so
the file carries Excel-style cached totals that our import is checked against to the kopeck
(PLAN §12, stages 1 and 3).

With `--summary` it builds the fixture of the «Свод» acceptance test: every month of 2026 has a
sheet, so the 2026 block of «Свод» resolves; 2025 has only some of its months, as an old
workbook would.

Requirements (only to regenerate the fixtures, not for CI): Python 3 (standard library only) and
LibreOffice (`soffice`).

    python3 tools/fixtures/make_workbook.py            # crates/core/tests/fixtures/workbook.xlsx
    python3 tools/fixtures/make_workbook.py --summary  # crates/core/tests/fixtures/workbook_2026.xlsx
"""

from __future__ import annotations

import datetime as dt
import random
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
from dataclasses import dataclass
from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "crates" / "core" / "tests" / "fixtures" / "workbook.xlsx"
SUMMARY_OUTPUT = ROOT / "crates" / "core" / "tests" / "fixtures" / "workbook_2026.xlsx"

SERVICE = "Служебный"
SUMMARY = "Свод"
# The month sheets of a workbook kept for a while: some months are missing.
SHEETS = ["01.25", "02.25", "03.25", "04.25", "06.25", "07.25", "06.26", "07.26"]
SUMMARY_SHEETS = SHEETS[:6] + [f"{m:02d}.26" for m in range(1, 13)]
# Sheets before 07.25 keep the cashback total at K7/L7, the later ones at L7/M7.
EARLY = {"01.25", "02.25", "03.25", "04.25", "06.25"}

# Months to fill; the other month sheets stay empty.
FILLED = ["01.25", "07.25", "07.26"]
ROWS_PER_MONTH = 48
# The «Свод» fixture: every month of 2026, smaller, plus 01.25 and 07.25 for the growth column.
SUMMARY_FILLED = ["01.25", "07.25"] + [f"{m:02d}.26" for m in range(1, 13)]
SUMMARY_ROWS = 20
LAST_ROW = 300

# «Служебный», column B from row 2: the categories in the order the app also uses.
CATEGORIES = [
    "еда", "продукты", "обучение", "лечение", "лекарства", "автомобиль", "семья", "красота",
    "косметика", "одежда", "обувь", "разное", "отдых", "Транспорт", "квартиры", "дача",
]
# Totals columns of a month sheet, I…U: (header, categories). Several categories make a group.
COLUMNS = [
    ("Еда", ["еда"]),
    ("Продукты", ["продукты"]),
    ("Обучение", ["обучение"]),
    ("Лечение/\nлекарства", ["лечение", "лекарства"]),
    ("Автомобиль", ["автомобиль"]),
    ("Транспорт", ["Транспорт"]),
    ("Семья", ["семья"]),
    ("Красота/\nкосметика", ["красота", "косметика"]),
    ("Одежда/\nобувь", ["одежда", "обувь"]),
    ("Разное", ["разное"]),
    ("Отдых", ["отдых"]),
    ("Квартиры", ["квартиры"]),
    ("Дача", ["дача"]),
]
# Rent (row 3) is counted only in these columns and in «Регулярные»; the others start at row 4.
FROM_ROW_3 = {"квартиры", "дача"}
MONTH_NAMES = [
    "январь", "февраль", "март", "апрель", "май", "июнь",
    "июль", "август", "сентябрь", "октябрь", "ноябрь", "декабрь",
]
SALARY = 55000

PURCHASES = [
    ("Пятёрочка", "продукты"),
    ("Лента", "продукты"),
    ("Перекрёсток", "Продукты"),  # SUMIF ignores case
    ("Обед на работе", "еда"),
    ("Кафе", "еда"),
    ("Аптека", "лекарства"),
    ("Стоматолог", "лечение"),
    ("Бензин", "автомобиль"),
    ("Мойка", "автомобиль"),
    ("Метро", "Транспорт"),
    ("Такси", "транспорт"),
    ("Подарок", "семья"),
    ("Стрижка", "красота"),
    ("Крем", "косметика"),
    ("Куртка", "одежда"),
    ("Кроссовки", "обувь"),
    ("Курсы английского", "обучение"),
    ("Билеты", "отдых"),
    ("Интернет", "квартиры"),
    ("Семена", "дача"),
    ("Мелочи", "разное"),
]
CARDS = ["Газпром КР", "ПСБ 3170", "Росб 4512", "Озон", "Райф А", "Райф М", "Почта А", "Д ВТБ", "Совком А"]
RATES = [None, None, 0.01, 0.015, 0.03, 0.05, 0.05, 0.06, 0.1]
PEOPLE = ["Андрей", "Марина"]


@dataclass(frozen=True)
class Formula:
    text: str


Cells = dict[str, object]


def month_of(sheet: str) -> tuple[int, int]:
    mm, yy = sheet.split(".")
    return 2000 + int(yy), int(mm)


def col(index: int) -> str:
    """Column letter of a 0-based index (A…Z is enough here)."""
    return chr(ord("A") + index)


def cashback_cells(sheet: str) -> tuple[str, str]:
    """The «Итого:» label and the cashback total next to it."""
    return ("K", "L") if sheet in EARLY else ("L", "M")


def service_sheet() -> Cells:
    cells: Cells = {"B1": "Справочник", "C1": "Регулярная трата", "C2": "да", "C3": "нет"}
    for i, name in enumerate(CATEGORIES):
        cells[f"B{2 + i}"] = name
    return cells


def sumif(category: str) -> str:
    first = 3 if category in FROM_ROW_3 else 4
    ref = f"{SERVICE}!B{2 + CATEGORIES.index(category)}"
    return f"SUMIF($C${first}:$C${LAST_ROW},{ref},$G${first}:$G${LAST_ROW})"


def month_sheet(sheet: str) -> Cells:
    headers = ["№ п.п.", "Наименование", "Назначение", "Регулярная да/нет", "Участник", "Дата",
               "Сумма", "Итого, руб."]
    cells: Cells = {f"{col(i)}1": h for i, h in enumerate(headers)}
    cells["H2"] = Formula(f"SUM(G3:G{LAST_ROW})")
    for i, (header, categories) in enumerate(COLUMNS):
        c = col(8 + i)
        cells[f"{c}1"] = header
        cells[f"{c}2"] = Formula("+".join(sumif(x) for x in categories))
    cells["V1"] = "Регулярные"
    cells["V2"] = Formula(f"SUMIF($D$3:$D${LAST_ROW},{SERVICE}!C2,$G$3:$G${LAST_ROW})")
    label, total = cashback_cells(sheet)
    cells[f"{label}7"] = "Итого:"
    cells[f"{total}7"] = Formula(f"SUM(I4:I{LAST_ROW})")
    cells[f"{label}8"] = "Ср. %"
    cells[f"{total}8"] = Formula(f"{total}7/(H2-G3)")
    return cells


def put_row(cells: Cells, row: int, title, category=None, regular=None, who=None, day=None,
            amount=None, rate=None, cashback=None, card=None) -> None:
    values = [row - 2, title, category, regular, who, day, amount, rate, cashback, card]
    for i, value in enumerate(values):
        if value is not None:
            cells[f"{col(i)}{row}"] = value


def fill(cells: Cells, sheet: str, rnd: random.Random, rows: int) -> None:
    year, month = month_of(sheet)
    on = lambda day: dt.date(year, month, day)  # noqa: E731
    rounddown = lambda row: Formula(f"ROUNDDOWN(G{row}*H{row},0)")  # noqa: E731
    # Row 3: rent, regular, no cashback (D16).
    put_row(cells, 3, "Квартплата", "квартиры", "да", "Андрей", on(1), 7840, card="Д ВТБ")

    for i in range(rows):
        row = 4 + i
        title, category = rnd.choice(PURCHASES)
        rate = rnd.choice(RATES)
        amount = round(rnd.uniform(50, 9000), rnd.choice([0, 0, 2]))
        regular = "да" if category in ("квартиры", "обучение") else rnd.choice([None, "нет"])
        put_row(cells, row, title, category, regular, rnd.choice(PEOPLE), on(rnd.randint(1, 28)),
                amount, rate, rounddown(row), rnd.choice(CARDS))

    # Edge cases at the end of the month.
    end = 4 + rows
    # A certificate with cashback typed as an amount (no %), like 666 ₽ → 200 ₽.
    put_row(cells, end, "Сертификат", "разное", None, "Марина", on(15), 666, None, 200, "Райф М")
    # A refund: a negative amount with a rate; ROUNDDOWN rounds toward zero.
    put_row(cells, end + 1, "Возврат куртки", "одежда", None, "Андрей", on(20), -2999.9, 0.05,
            rounddown(end + 1), "Газпром КР")
    # A category missing from «Служебный»: only in Итого.
    put_row(cells, end + 2, "Кино", "кино", None, "Марина", on(21), 900, None, rounddown(end + 2),
            "Озон")
    # A title without an amount.
    put_row(cells, end + 3, "Записать позже", "еда")


def summary_sheet(sheets: list[str]) -> Cells:
    """«Свод»: a block per year, linked to the month sheets; a missing sheet gives an error."""
    headers = ["Сумма", "Оклад", "Прирост", None] + [h for h, _ in COLUMNS] + [
        "Регулярные", "Кэшбэк", "Общий баланс", "Общий % кэшбэка"]
    cells: Cells = {"A1": "№ п.п.", "B1": "Год", "C1": "Месяц", "D1": "Сумма",
                    "E1": "Баланс в мес./\nИтого за год", "F1": "Прирост относительно прошлого года"}
    for i, h in enumerate(headers[4:]):
        cells[f"{col(7 + i)}1"] = h
    block = 19  # rows from a year's header to the next one's
    for b, year in enumerate((2025, 2026)):
        header = 3 + b * block
        cells[f"C{header}"] = year
        for i, h in enumerate(headers):
            if h is not None:
                cells[f"{col(3 + i)}{header}"] = h
        rows = range(header + 1, header + 13)
        for month, r in enumerate(rows, start=1):
            sheet = f"{month:02d}.{year % 100:02d}"
            ref = f"'{sheet}'!"
            cells[f"A{r}"] = b * 12 + month
            cells[f"B{r}"] = year
            cells[f"C{r}"] = MONTH_NAMES[month - 1]
            cells[f"D{r}"] = Formula(f"{ref}H2")
            cells[f"E{r}"] = SALARY
            if b > 0:
                cells[f"F{r}"] = Formula(f"D{r}/D{r - block}-1")
            for i in range(len(COLUMNS) + 1):  # the columns and «Регулярные»: I$2…V$2
                cells[f"{col(7 + i)}{r}"] = Formula(f"{ref}{col(8 + i)}$2")
            _, total = cashback_cells(sheet)
            cells[f"V{r}"] = Formula(f"{ref}{total}7")
            cells[f"W{r}"] = Formula(f"E{r}+V{r}-D{r}")
            cells[f"X{r}"] = Formula(f"V{r}/D{r}")
            if sheet not in sheets:
                # A month without a sheet: the links break, as in an old workbook.
                for c in "DHIJKLMNOPQRSTUV":
                    cells[f"{c}{r}"] = Formula("#REF!")
        first, last = rows[0], rows[-1]
        span = lambda c: f"{c}{first}:{c}{last}"  # noqa: E731
        foot = header + 13
        cells[f"E{foot}"] = Formula(f"AVERAGE({span('D')})")
        if b > 0:
            cells[f"F{foot}"] = Formula(f"E{foot}/E{foot - block}-1")
        cells[f"E{foot + 1}"] = Formula(f"SUM({span('D')})")
        cells[f"F{foot + 1}"] = Formula(f"SUM(H{foot}:T{foot})")
        for i, label in enumerate(["Сумма", "От общ. %", "Среднее", "Ср. прирост"]):
            cells[f"G{foot + i}"] = label
        for c in "HIJKLMNOPQRSTUV":
            cells[f"{c}{foot}"] = Formula(f"SUM({span(c)})")
            cells[f"{c}{foot + 1}"] = Formula(f"{c}{foot}/$E{foot + 1}")
            cells[f"{c}{foot + 2}"] = Formula(f"AVERAGE({span(c)})")
        for c in "HIJKLMNOPQRST":
            if b > 0:
                cells[f"{c}{foot + 3}"] = Formula(f"{c}{foot + 2}-{c}{foot + 2 - block}")
                cells[f"{c}{foot + 4}"] = Formula(f"{c}{foot + 3}/{c}{foot + 2 - block}")
        cells[f"W{foot + 3}"] = Formula(f"SUM({span('W')})")
    return cells


# --- A minimal xlsx writer (inline strings, a date style, formulas without cached values) ---

def cell_xml(ref: str, value: object) -> str:
    if isinstance(value, Formula):
        return f'<c r="{ref}"><f>{escape(value.text)}</f></c>'
    if isinstance(value, str):
        return f'<c r="{ref}" t="inlineStr"><is><t xml:space="preserve">{escape(value)}</t></is></c>'
    if isinstance(value, dt.date):
        serial = (value - dt.date(1899, 12, 30)).days
        return f'<c r="{ref}" s="1"><v>{serial}</v></c>'
    return f'<c r="{ref}"><v>{value!r}</v></c>'


def sheet_xml(cells: Cells) -> str:
    def key(ref: str) -> tuple[int, int]:
        letters, digits = re.fullmatch(r"([A-Z]+)(\d+)", ref).groups()
        return int(digits), ord(letters) - ord("A")

    rows: dict[int, list[str]] = {}
    for ref in sorted(cells, key=key):
        rows.setdefault(key(ref)[0], []).append(cell_xml(ref, cells[ref]))
    body = "".join(f'<row r="{r}">{"".join(cs)}</row>' for r, cs in sorted(rows.items()))
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        f"<sheetData>{body}</sheetData></worksheet>"
    )


def write_xlsx(path: Path, sheets: list[tuple[str, Cells]]) -> None:
    main = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
    rel = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    pkg = "http://schemas.openxmlformats.org/package/2006/relationships"
    ct_sheet = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"
    overrides = "".join(
        f'<Override PartName="/xl/worksheets/sheet{i}.xml" ContentType="{ct_sheet}"/>'
        for i in range(1, len(sheets) + 1)
    )
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr(
            "[Content_Types].xml",
            '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
            '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/>'
            '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
            '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>'
            f"{overrides}</Types>",
        )
        z.writestr(
            "_rels/.rels",
            f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="{pkg}">'
            f'<Relationship Id="rId1" Type="{rel}/officeDocument" Target="xl/workbook.xml"/>'
            "</Relationships>",
        )
        sheet_list = "".join(
            f'<sheet name="{escape(name)}" sheetId="{i}" r:id="rId{i}"/>'
            for i, (name, _) in enumerate(sheets, start=1)
        )
        z.writestr(
            "xl/workbook.xml",
            f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="{main}" '
            f'xmlns:r="{rel}"><sheets>{sheet_list}</sheets><calcPr fullCalcOnLoad="1"/></workbook>',
        )
        links = "".join(
            f'<Relationship Id="rId{i}" Type="{rel}/worksheet" Target="worksheets/sheet{i}.xml"/>'
            for i in range(1, len(sheets) + 1)
        )
        styles_id = len(sheets) + 1
        z.writestr(
            "xl/_rels/workbook.xml.rels",
            f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="{pkg}">'
            f'{links}<Relationship Id="rId{styles_id}" Type="{rel}/styles" Target="styles.xml"/>'
            "</Relationships>",
        )
        z.writestr(
            "xl/styles.xml",
            f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="{main}">'
            '<fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>'
            '<fills count="1"><fill><patternFill patternType="none"/></fill></fills>'
            '<borders count="1"><border/></borders>'
            '<cellStyleXfs count="1"><xf/></cellStyleXfs>'
            '<cellXfs count="2"><xf/><xf numFmtId="14" applyNumberFormat="1"/></cellXfs>'
            "</styleSheet>",
        )
        for i, (_, cells) in enumerate(sheets, start=1):
            z.writestr(f"xl/worksheets/sheet{i}.xml", sheet_xml(cells))


def main() -> int:
    soffice = shutil.which("soffice")
    if soffice is None:
        print("LibreOffice (soffice) is required", file=sys.stderr)
        return 1
    summary = "--summary" in sys.argv[1:]
    rnd = random.Random(20261103 if summary else 20261027)
    names = SUMMARY_SHEETS if summary else SHEETS
    filled = SUMMARY_FILLED if summary else FILLED
    rows = SUMMARY_ROWS if summary else ROWS_PER_MONTH
    output = SUMMARY_OUTPUT if summary else OUTPUT

    sheets: list[tuple[str, Cells]] = [(SERVICE, service_sheet())]
    for name in names:
        cells = month_sheet(name)
        if name in filled:
            fill(cells, name, rnd, rows)
        sheets.append((name, cells))
    sheets.append((SUMMARY, summary_sheet(names)))

    with tempfile.TemporaryDirectory() as tmp:
        raw = Path(tmp) / "workbook.xlsx"
        write_xlsx(raw, sheets)
        out = Path(tmp) / "out"
        subprocess.run(
            [
                soffice,
                f"-env:UserInstallation=file://{tmp}/profile",
                "--headless",
                "--calc",
                "--convert-to",
                "xlsx",
                "--outdir",
                str(out),
                str(raw),
            ],
            check=True,
            capture_output=True,
        )
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(out / "workbook.xlsx", output)
    print(f"wrote {output.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
