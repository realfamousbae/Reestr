//! The xlsx export (D29, D30): «Служебный», the month sheets and «Свод» in the layout the import
//! reads, with values instead of formulas, then a sheet for each kind of data that has no place
//! on them. Our import reads the month sheets back into the same journal.

use std::collections::HashMap;

use rust_xlsxwriter::{
    ColNum, Color, ExcelDateTime, Format, FormatAlign, FormatBorder, RowNum, Workbook, Worksheet,
    XlsxError,
};
use time::Date;

use super::{Book, BookMonth, ExportError};
use crate::cashback::{CashbackSource, Rounding};
use crate::dict::ProductKind;
use crate::import::SERVICE_SHEET;
use crate::money::Kopecks;
use crate::products::TrancheStatus;
use crate::summary::{FooterLine, SummaryRow, YearSummary};

const SUMMARY_SHEET: &str = "Свод";

const MONTH_NAMES: [&str; 12] = [
    "январь",
    "февраль",
    "март",
    "апрель",
    "май",
    "июнь",
    "июль",
    "август",
    "сентябрь",
    "октябрь",
    "ноябрь",
    "декабрь",
];

/// The band color of the program's headers.
const BAND: u32 = 0x00EC_F1ED;

/// A value of a cell.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Cell {
    Empty,
    Text(String),
    Int(i32),
    Money(Kopecks),
    /// Basis points, shown as a percentage.
    Percent(i64),
    Date(Date),
}

impl Cell {
    fn text(value: &str) -> Self {
        if value.is_empty() {
            Self::Empty
        } else {
            Self::Text(value.to_owned())
        }
    }

    fn opt_text(value: Option<&str>) -> Self {
        value.map_or(Self::Empty, Self::text)
    }

    fn money(value: Option<Kopecks>) -> Self {
        value.map_or(Self::Empty, Self::Money)
    }

    fn percent(value: Option<i64>) -> Self {
        value.map_or(Self::Empty, Self::Percent)
    }

    fn date(value: Option<Date>) -> Self {
        value.map_or(Self::Empty, Self::Date)
    }

    fn flag(value: bool) -> Self {
        Self::text(if value { "да" } else { "нет" })
    }
}

/// The only place where kopecks become the floating-point numbers an xlsx stores.
#[allow(clippy::float_arithmetic, clippy::cast_precision_loss)]
fn rubles(value: Kopecks) -> f64 {
    value.get() as f64 / 100.0
}

/// Basis points as the fraction Excel formats as a percentage: 500 → 0.05.
#[allow(clippy::float_arithmetic, clippy::cast_precision_loss)]
fn fraction(bp: i64) -> f64 {
    bp as f64 / 10_000.0
}

fn row_num(index: usize) -> RowNum {
    RowNum::try_from(index).unwrap_or(RowNum::MAX)
}

fn col_num(index: usize) -> ColNum {
    ColNum::try_from(index).unwrap_or(ColNum::MAX)
}

/// The formats of the workbook; the arrays hold the plain and the bold variant.
struct Styles {
    header: Format,
    bold: Format,
    money: [Format; 2],
    whole: [Format; 2],
    part: [Format; 2],
    date: Format,
}

impl Styles {
    fn new() -> Self {
        let money = Format::new().set_num_format("#,##0.00");
        let whole = Format::new().set_num_format("0%");
        let part = Format::new().set_num_format("0.0#%");
        Self {
            header: Format::new()
                .set_bold()
                .set_text_wrap()
                .set_align(FormatAlign::Center)
                .set_background_color(Color::RGB(BAND))
                .set_border_bottom(FormatBorder::Thin),
            bold: Format::new().set_bold(),
            money: [money.clone(), money.set_bold()],
            whole: [whole.clone(), whole.set_bold()],
            part: [part.clone(), part.set_bold()],
            date: Format::new().set_num_format("dd.mm.yyyy"),
        }
    }
}

fn put(
    ws: &mut Worksheet,
    row: RowNum,
    col: ColNum,
    cell: &Cell,
    s: &Styles,
    bold: bool,
) -> Result<(), XlsxError> {
    let b = usize::from(bold);
    match cell {
        Cell::Empty => {}
        Cell::Text(text) if bold => {
            ws.write_string_with_format(row, col, text, &s.bold)?;
        }
        Cell::Text(text) => {
            ws.write_string(row, col, text)?;
        }
        Cell::Int(n) if bold => {
            ws.write_number_with_format(row, col, *n, &s.bold)?;
        }
        Cell::Int(n) => {
            ws.write_number(row, col, *n)?;
        }
        Cell::Money(k) => {
            ws.write_number_with_format(row, col, rubles(*k), &s.money[b])?;
        }
        Cell::Percent(bp) => {
            let format = if bp % 100 == 0 {
                &s.whole[b]
            } else {
                &s.part[b]
            };
            ws.write_number_with_format(row, col, fraction(*bp), format)?;
        }
        Cell::Date(d) => {
            let excel = u16::try_from(d.year())
                .ok()
                .and_then(|y| ExcelDateTime::from_ymd(y, u8::from(d.month()), d.day()).ok());
            match excel {
                Some(excel) => {
                    ws.write_datetime_with_format(row, col, excel, &s.date)?;
                }
                None => {
                    ws.write_string(row, col, crate::date::format_iso(*d))?;
                }
            }
        }
    }
    Ok(())
}

fn put_row(
    ws: &mut Worksheet,
    row: RowNum,
    first_col: usize,
    cells: &[Cell],
    s: &Styles,
    bold: bool,
) -> Result<(), XlsxError> {
    for (i, cell) in cells.iter().enumerate() {
        put(ws, row, col_num(first_col + i), cell, s, bold)?;
    }
    Ok(())
}

fn header(
    ws: &mut Worksheet,
    row: RowNum,
    first_col: usize,
    titles: &[&str],
    s: &Styles,
) -> Result<(), XlsxError> {
    for (i, title) in titles.iter().enumerate() {
        ws.write_string_with_format(row, col_num(first_col + i), *title, &s.header)?;
    }
    Ok(())
}

/// A sheet of one table: a header row and the rows under it.
fn table(
    wb: &mut Workbook,
    name: &str,
    columns: &[(&str, f64)],
    rows: Vec<Vec<Cell>>,
    s: &Styles,
) -> Result<(), XlsxError> {
    let ws = wb.add_worksheet();
    ws.set_name(name)?;
    let titles: Vec<&str> = columns.iter().map(|(t, _)| *t).collect();
    header(ws, 0, 0, &titles, s)?;
    for (i, (_, width)) in columns.iter().enumerate() {
        ws.set_column_width(col_num(i), *width)?;
    }
    ws.set_freeze_panes(1, 0)?;
    for (i, row) in rows.iter().enumerate() {
        put_row(ws, row_num(i + 1), 0, row, s, false)?;
    }
    Ok(())
}

/// Names of dictionary items by id.
struct Names {
    categories: HashMap<i64, String>,
    groups: HashMap<i64, String>,
    participants: HashMap<i64, String>,
    products: HashMap<i64, String>,
    banks: HashMap<i64, String>,
}

impl Names {
    fn new(book: &Book) -> Self {
        Self {
            categories: book
                .categories
                .iter()
                .map(|c| (c.id, c.name.clone()))
                .collect(),
            groups: book.groups.iter().map(|g| (g.id, g.name.clone())).collect(),
            participants: book
                .participants
                .iter()
                .map(|p| (p.id, p.name.clone()))
                .collect(),
            products: book
                .products
                .iter()
                .map(|p| (p.id, p.short_name.clone()))
                .collect(),
            banks: book.banks.iter().map(|b| (b.id, b.name.clone())).collect(),
        }
    }

    fn of(map: &HashMap<i64, String>, id: Option<i64>) -> Cell {
        Cell::opt_text(id.and_then(|id| map.get(&id)).map(String::as_str))
    }

    fn category(&self, id: Option<i64>) -> Cell {
        Self::of(&self.categories, id)
    }

    fn participant(&self, id: Option<i64>) -> Cell {
        Self::of(&self.participants, id)
    }

    fn product(&self, id: Option<i64>) -> Cell {
        Self::of(&self.products, id)
    }
}

/// «Служебный»: the categories in column B, as month sheet formulas refer to them.
fn service_sheet(wb: &mut Workbook, book: &Book, s: &Styles) -> Result<(), XlsxError> {
    let ws = wb.add_worksheet();
    ws.set_name(SERVICE_SHEET)?;
    ws.write_string_with_format(0, 1, "Справочник", &s.bold)?;
    ws.write_string_with_format(0, 2, "Регулярная трата", &s.bold)?;
    ws.write_string(1, 2, "да")?;
    ws.write_string(2, 2, "нет")?;
    for (i, c) in book.categories.iter().enumerate() {
        ws.write_string(row_num(i + 1), 1, &c.name)?;
    }
    ws.set_column_width(1, 20)?;
    ws.set_column_width(2, 18)?;
    Ok(())
}

/// The journal columns of a month sheet, A–G; H–J (%, cashback, card) and K (MCC) sit under the
/// totals band. Rows 1–2 of K belong to the band, so K is wide enough for a total.
const JOURNAL: [(&str, f64); 11] = [
    ("№", 5.0),
    ("Наименование", 32.0),
    ("Назначение", 14.0),
    ("Регулярная", 11.0),
    ("Участник", 11.0),
    ("Дата", 11.0),
    ("Сумма", 12.0),
    ("Итого, руб.", 12.0),
    ("", 11.0),
    ("", 11.0),
    ("", 12.0),
];

/// Where a month sheet keeps «Итого:» and «Ср. %» of the cashback: L7:M8.
const BLOCK_ROW: RowNum = 6;
const BLOCK_COL: ColNum = 11;

fn month_sheet(
    wb: &mut Workbook,
    m: &BookMonth,
    names: &Names,
    s: &Styles,
) -> Result<(), XlsxError> {
    let ws = wb.add_worksheet();
    ws.set_name(m.month.sheet_name())?;
    let t = &m.totals;

    // Rows 1–2: the column numbers 1–7, then the totals band by merged columns (groups).
    for (i, (title, width)) in JOURNAL.iter().enumerate() {
        ws.set_column_width(col_num(i), *width)?;
        if i < 7 {
            ws.write_string_with_format(0, col_num(i), *title, &s.header)?;
            put(
                ws,
                1,
                col_num(i),
                &Cell::Int(i32::try_from(i + 1).unwrap_or(0)),
                s,
                true,
            )?;
        }
    }
    let mut band: Vec<(&str, Kopecks)> = vec![("Итого, руб.", t.total)];
    band.extend(t.by_group.iter().map(|c| (c.label.as_str(), c.amount)));
    if !t.uncategorized.is_zero() {
        band.push(("Без назначения", t.uncategorized));
    }
    band.push(("Регулярные", t.regular));
    for (i, (label, amount)) in band.iter().enumerate() {
        let col = col_num(7 + i);
        ws.write_string_with_format(0, col, *label, &s.header)?;
        put(ws, 1, col, &Cell::Money(*amount), s, true)?;
        if 7 + i >= JOURNAL.len() {
            ws.set_column_width(col, 12)?;
        }
    }

    // The journal from row 3. Cashback by a rule is written as its amount only: a percentage
    // would be recomputed without the rule's limits when the sheet is imported again (D29).
    for (i, e) in m.entries.iter().enumerate() {
        let rate = e
            .cashback
            .filter(|c| c.source == CashbackSource::ManualPercent)
            .and_then(|c| c.rate)
            .map_or(Cell::Empty, |r| Cell::Percent(i64::from(r.0)));
        let cells = [
            Cell::Int(i32::try_from(i + 1).unwrap_or(i32::MAX)),
            Cell::text(&e.title),
            names.category(e.category_id),
            if e.is_regular {
                Cell::text("да")
            } else {
                Cell::Empty
            },
            names.participant(e.participant_id),
            Cell::date(e.date),
            Cell::Money(e.amount),
            rate,
            Cell::money(e.cashback.map(|c| c.amount)),
            names.product(e.product_id),
            // Text: a number would lose the leading zero of 0742.
            Cell::opt_text(e.mcc.as_ref().map(crate::mcc::Mcc::as_str)),
        ];
        put_row(ws, row_num(i + 2), 0, &cells, s, false)?;
    }

    // The right blocks: the cashback cells, then «Приход / расход».
    let b = &m.balance;
    let block = [
        ("Итого:", Cell::Money(t.cashback)),
        ("Ср. %", Cell::percent(t.cashback_rate_bp)),
        ("", Cell::Empty),
        ("План дохода", Cell::money(b.plan.map(|p| p.amount))),
        ("Приход", Cell::Money(b.inflow)),
        ("Расход", Cell::Money(b.spent)),
        ("Дельта по плану", Cell::money(b.planned)),
        ("Дельта по факту", Cell::money(b.actual)),
        ("Общий баланс", Cell::money(b.overall)),
    ];
    for (i, (label, value)) in block.iter().enumerate() {
        let row = BLOCK_ROW + row_num(i);
        if !label.is_empty() {
            ws.write_string_with_format(row, BLOCK_COL, *label, &s.bold)?;
        }
        put(ws, row, BLOCK_COL + 1, value, s, false)?;
    }
    ws.set_column_width(BLOCK_COL, 17)?;
    ws.set_column_width(BLOCK_COL + 1, 13)?;
    ws.set_freeze_panes(2, 0)?;
    Ok(())
}

fn month_name(row: &SummaryRow) -> &'static str {
    MONTH_NAMES
        .get(usize::from(row.month.month()).saturating_sub(1))
        .copied()
        .unwrap_or("")
}

fn footer_cells<T: Copy>(
    first: [Cell; 4],
    line: &FooterLine<T>,
    cell: impl Fn(T) -> Cell,
    last: [Cell; 2],
) -> Vec<Cell> {
    let mut cells: Vec<Cell> = first.into();
    cells.extend(line.columns.iter().map(|v| cell(*v)));
    cells.push(cell(line.regular));
    cells.push(cell(line.cashback));
    cells.extend(last);
    cells
}

fn empty4() -> [Cell; 4] {
    [Cell::Empty, Cell::Empty, Cell::Empty, Cell::Empty]
}

fn empty2() -> [Cell; 2] {
    [Cell::Empty, Cell::Empty]
}

/// One year of «Свод»: the header, twelve months and the footer. Returns the next free row.
fn year_block(
    ws: &mut Worksheet,
    first_row: RowNum,
    y: &YearSummary,
    s: &Styles,
) -> Result<RowNum, XlsxError> {
    let mut titles = vec![
        "Сумма",
        "План дохода",
        "Баланс в мес.",
        "Прирост к прошлому году",
    ];
    titles.extend(y.columns.iter().map(|c| c.label.as_str()));
    titles.extend(["Регулярные", "Кешбек", "Общий баланс", "Общий % кешбека"]);
    put(ws, first_row, 0, &Cell::Int(y.year), s, true)?;
    header(ws, first_row, 1, &titles, s)?;

    let mut row = first_row + 1;
    for m in &y.months {
        put(ws, row, 0, &Cell::text(month_name(m)), s, false)?;
        let cells: Vec<Cell> = if m.has_data {
            let mut cells = vec![
                Cell::Money(m.total),
                Cell::money(m.plan),
                Cell::money(m.balance),
                Cell::percent(m.growth_bp),
            ];
            cells.extend(m.columns.iter().map(|&c| Cell::Money(c)));
            cells.extend([
                Cell::Money(m.regular),
                Cell::Money(m.cashback),
                Cell::money(m.overall_balance),
                Cell::percent(m.cashback_rate_bp),
            ]);
            cells
        } else {
            vec![Cell::Empty, Cell::money(m.plan)]
        };
        put_row(ws, row, 1, &cells, s, false)?;
        row += 1;
    }

    let f = &y.footer;
    let mut lines: Vec<(&str, Vec<Cell>, bool)> = vec![
        (
            "Сумма",
            footer_cells(
                [Cell::Money(f.total), Cell::Empty, Cell::Empty, Cell::Empty],
                &f.sums,
                Cell::Money,
                [Cell::money(f.overall_balance), Cell::Empty],
            ),
            true,
        ),
        (
            "От общ. %",
            footer_cells(empty4(), &f.shares_bp, Cell::percent, empty2()),
            false,
        ),
        (
            "Среднее",
            footer_cells(
                [
                    Cell::money(f.average_total),
                    Cell::Empty,
                    Cell::Empty,
                    Cell::percent(f.average_total_growth_bp),
                ],
                &f.averages,
                Cell::money,
                empty2(),
            ),
            false,
        ),
    ];
    let growth = &f.average_growth;
    if growth
        .columns
        .iter()
        .chain([&growth.regular, &growth.cashback])
        .any(Option::is_some)
    {
        lines.push((
            "Ср. прирост, ₽",
            footer_cells(empty4(), growth, Cell::money, empty2()),
            false,
        ));
        lines.push((
            "Ср. прирост, %",
            footer_cells(empty4(), &f.average_growth_bp, Cell::percent, empty2()),
            false,
        ));
    }
    for (label, cells, bold) in lines {
        put(ws, row, 0, &Cell::text(label), s, true)?;
        put_row(ws, row, 1, &cells, s, bold)?;
        row += 1;
    }
    Ok(row + 1)
}

fn summary_sheet(wb: &mut Workbook, years: &[YearSummary], s: &Styles) -> Result<(), XlsxError> {
    let ws = wb.add_worksheet();
    ws.set_name(SUMMARY_SHEET)?;
    ws.set_column_width(0, 16)?;
    let widest = years.iter().map(|y| y.columns.len()).max().unwrap_or(0);
    for col in 1..=widest + 8 {
        ws.set_column_width(col_num(col), 12)?;
    }
    let mut row = 0;
    for y in years {
        row = year_block(ws, row, y, s)?;
    }
    Ok(())
}

const fn kind_label(kind: ProductKind) -> &'static str {
    match kind {
        ProductKind::Debit => "дебетовая",
        ProductKind::Credit => "кредитная",
        ProductKind::Savings => "накопительный счёт",
        ProductKind::Deposit => "вклад",
        ProductKind::Cash => "наличные",
    }
}

const fn rounding_label(rounding: Rounding) -> &'static str {
    match rounding {
        Rounding::FloorRub => "вниз до рубля",
        Rounding::FloorKop => "вниз до копейки",
        Rounding::HalfUpRub => "до рубля",
        Rounding::HalfUpKop => "до копейки",
    }
}

const fn status_label(status: TrancheStatus) -> &'static str {
    match status {
        TrancheStatus::Active => "действует",
        TrancheStatus::Repaid => "погашен",
        TrancheStatus::Overdue => "просрочен",
    }
}

/// Sheets for the data that has no place on the month sheets and «Свод» (D30).
#[allow(clippy::too_many_lines)]
fn data_sheets(wb: &mut Workbook, book: &Book, names: &Names, s: &Styles) -> Result<(), XlsxError> {
    table(
        wb,
        "Категории",
        &[
            ("Категория", 20.0),
            ("Группа", 22.0),
            ("Регулярная", 12.0),
            ("Активна", 10.0),
        ],
        book.categories
            .iter()
            .map(|c| {
                vec![
                    Cell::text(&c.name),
                    Names::of(&names.groups, c.group_id),
                    Cell::flag(c.is_regular_default),
                    Cell::flag(c.is_active),
                ]
            })
            .collect(),
        s,
    )?;
    table(
        wb,
        "Участники",
        &[("Участник", 20.0), ("Активен", 10.0)],
        book.participants
            .iter()
            .map(|p| vec![Cell::text(&p.name), Cell::flag(p.is_active)])
            .collect(),
        s,
    )?;
    table(
        wb,
        "Банки",
        &[("Банк", 24.0), ("Короткое название", 18.0)],
        book.banks
            .iter()
            .map(|b| vec![Cell::text(&b.name), Cell::opt_text(b.short_name.as_deref())])
            .collect(),
        s,
    )?;
    let credit: HashMap<i64, _> = book.credit_terms.iter().map(|(p, t)| (*p, t)).collect();
    let savings: HashMap<i64, _> = book.savings_terms.iter().map(|(p, t)| (*p, t)).collect();
    table(
        wb,
        "Карты",
        &[
            ("Карта", 16.0),
            ("Полное название", 24.0),
            ("Банк", 18.0),
            ("Тип", 18.0),
            ("Участник", 12.0),
            ("Округление кэшбэка", 16.0),
            ("Активна", 9.0),
            ("Открыта", 12.0),
            ("Закрыта", 12.0),
            ("Кредитный лимит", 14.0),
            ("Грейс, дней", 10.0),
            ("Доступный остаток", 14.0),
            ("Остаток на дату", 12.0),
            ("Ставка НС", 10.0),
            ("НС закрыт", 12.0),
            ("Открыть снова через, дней", 14.0),
            ("Заметки", 28.0),
        ],
        book.products
            .iter()
            .map(|p| {
                let c = credit.get(&p.id);
                let sv = savings.get(&p.id);
                vec![
                    Cell::text(&p.short_name),
                    Cell::opt_text(p.full_name.as_deref()),
                    Names::of(&names.banks, p.bank_id),
                    Cell::opt_text(p.kind.map(kind_label)),
                    names.participant(p.participant_id),
                    Cell::text(rounding_label(p.cashback_rounding)),
                    Cell::flag(p.is_active),
                    Cell::date(p.opened_at),
                    Cell::date(p.closed_at),
                    Cell::money(c.and_then(|c| c.credit_limit)),
                    c.and_then(|c| c.default_grace_days)
                        .map_or(Cell::Empty, |d| {
                            Cell::Int(i32::try_from(d).unwrap_or(i32::MAX))
                        }),
                    Cell::money(c.and_then(|c| c.manual_available)),
                    Cell::date(c.and_then(|c| c.manual_available_on)),
                    sv.and_then(|t| t.promo_rate)
                        .map_or(Cell::Empty, |r| Cell::Percent(i64::from(r.0))),
                    Cell::date(sv.and_then(|t| t.promo_closed_at)),
                    sv.and_then(|t| t.reopen_after_days)
                        .map_or(Cell::Empty, |d| {
                            Cell::Int(i32::try_from(d).unwrap_or(i32::MAX))
                        }),
                    Cell::opt_text(p.notes.as_deref()),
                ]
            })
            .collect(),
        s,
    )?;
    table(
        wb,
        "Регулярные платежи",
        &[
            ("Наименование", 28.0),
            ("Назначение", 16.0),
            ("Участник", 12.0),
            ("Карта", 14.0),
            ("Сумма", 12.0),
            ("Не в Ср. %", 11.0),
            ("Активен", 9.0),
        ],
        book.templates
            .iter()
            .map(|t| {
                vec![
                    Cell::text(&t.title),
                    names.category(t.category_id),
                    names.participant(t.participant_id),
                    names.product(t.product_id),
                    Cell::money(t.default_amount),
                    Cell::flag(t.exclude_from_rate),
                    Cell::flag(t.is_active),
                ]
            })
            .collect(),
        s,
    )?;
    table(
        wb,
        "Правила кэшбэка",
        &[
            ("Месяц", 9.0),
            ("Карта", 14.0),
            ("Категория", 16.0),
            ("MCC", 7.0),
            ("%", 8.0),
            ("Лимит трат", 13.0),
            ("Лимит кэшбэка", 13.0),
            ("Корректировка", 13.0),
            ("Потрачено", 13.0),
            ("Кэшбэк", 12.0),
            ("Осталось трат", 13.0),
            ("Осталось кэшбэка", 13.0),
            ("Заметки", 24.0),
        ],
        book.rules
            .iter()
            .map(|(r, usage)| {
                vec![
                    Cell::text(&r.month.sheet_name()),
                    names.product(Some(r.product_id)),
                    match (r.category_id, r.mcc) {
                        (None, None) => Cell::text("на всё"),
                        (category, _) => names.category(category),
                    },
                    Cell::opt_text(r.mcc.as_ref().map(crate::mcc::Mcc::as_str)),
                    Cell::Percent(i64::from(r.rate.0)),
                    Cell::money(r.spend_cap),
                    Cell::money(r.cashback_cap),
                    Cell::Money(r.spent_adjustment),
                    Cell::money(usage.map(|u| u.spent)),
                    Cell::money(usage.map(|u| u.cashback)),
                    Cell::money(usage.and_then(|u| u.remaining_spend)),
                    Cell::money(usage.and_then(|u| u.remaining_cashback)),
                    Cell::opt_text(r.notes.as_deref()),
                ]
            })
            .collect(),
        s,
    )?;
    table(
        wb,
        "Поступления",
        &[
            ("Месяц", 9.0),
            ("Дата", 12.0),
            ("Источник", 20.0),
            ("Участник", 12.0),
            ("Карта", 14.0),
            ("Сумма", 13.0),
            ("Заметки", 24.0),
        ],
        book.inflows
            .iter()
            .map(|i| {
                vec![
                    Cell::text(&i.month.sheet_name()),
                    Cell::date(i.date),
                    Cell::text(&i.source),
                    names.participant(i.participant_id),
                    names.product(i.product_id),
                    Cell::Money(i.amount),
                    Cell::opt_text(i.notes.as_deref()),
                ]
            })
            .collect(),
        s,
    )?;
    table(
        wb,
        "План дохода",
        &[("С месяца", 10.0), ("Сумма", 13.0)],
        book.plans
            .iter()
            .map(|(m, amount)| vec![Cell::text(&m.sheet_name()), Cell::Money(*amount)])
            .collect(),
        s,
    )?;
    table(
        wb,
        "Грейсы",
        &[
            ("Карта", 14.0),
            ("Начало", 12.0),
            ("Конец", 12.0),
            ("Корректировка", 13.0),
            ("Долг", 13.0),
            ("Погашено", 13.0),
            ("Осталось", 13.0),
            ("Статус", 12.0),
            ("Заметки", 24.0),
        ],
        book.tranches
            .iter()
            .map(|(t, state)| {
                vec![
                    names.product(Some(t.product_id)),
                    Cell::Date(t.start),
                    Cell::Date(t.end),
                    Cell::Money(t.debt_adjustment),
                    Cell::Money(state.debt),
                    Cell::Money(state.repaid),
                    Cell::Money(state.remaining),
                    Cell::text(status_label(state.status)),
                    Cell::opt_text(t.notes.as_deref()),
                ]
            })
            .collect(),
        s,
    )?;
    let tranches: HashMap<i64, _> = book.tranches.iter().map(|(t, _)| (t.id, t)).collect();
    table(
        wb,
        "Погашения",
        &[
            ("Карта", 14.0),
            ("Грейс", 24.0),
            ("Дата", 12.0),
            ("Сумма", 13.0),
            ("Заметки", 24.0),
        ],
        book.repayments
            .iter()
            .map(|r| {
                let t = tranches.get(&r.grace_period_id);
                vec![
                    names.product(t.map(|t| t.product_id)),
                    t.map_or(Cell::Empty, |t| {
                        Cell::Text(format!(
                            "{} — {}",
                            crate::date::format_dotted(t.start),
                            crate::date::format_dotted(t.end)
                        ))
                    }),
                    Cell::Date(r.date),
                    Cell::Money(r.amount),
                    Cell::opt_text(r.notes.as_deref()),
                ]
            })
            .collect(),
        s,
    )?;
    table(
        wb,
        "Напоминания",
        &[
            ("Дата", 12.0),
            ("Что сделать", 32.0),
            ("Карта", 14.0),
            ("Сделано", 9.0),
        ],
        book.reminders
            .iter()
            .map(|d| {
                vec![
                    Cell::Date(d.date),
                    Cell::text(&d.title),
                    names.product(d.product_id),
                    Cell::flag(d.is_done),
                ]
            })
            .collect(),
        s,
    )?;
    Ok(())
}

fn write(book: &Book) -> Result<Vec<u8>, XlsxError> {
    let mut wb = Workbook::new();
    let s = Styles::new();
    let names = Names::new(book);
    service_sheet(&mut wb, book, &s)?;
    for m in &book.months {
        month_sheet(&mut wb, m, &names, &s)?;
    }
    summary_sheet(&mut wb, &book.years, &s)?;
    data_sheets(&mut wb, book, &names, &s)?;
    wb.save_to_buffer()
}

/// The xlsx export: the bytes of the workbook.
pub fn xlsx(book: &Book) -> Result<Vec<u8>, ExportError> {
    write(book).map_err(|e| ExportError::Xlsx(e.to_string()))
}
