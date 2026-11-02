//! Reading an xlsx workbook: the «Служебный» sheet and the month sheets `MM.YY`.
//!
//! Month sheet layout (see `docs/reference/`): row 1 is the header, row 2 holds the column
//! numbers 1–7 and the totals (`H2 = SUM(G…)`, `I2…U2 = SUMIF(C…; Служебный!Bn; G…)`,
//! `V2 = SUMIF(D…; Служебный!C2; G…)`), data rows start at row 3 in columns A–J:
//! `№ | Наименование | Назначение | Регулярная | Участник | Дата | Сумма | % | Кэшбэк | Карта`.
//! Column K holds «MCC»; a sheet without it imports with empty codes.
//! A label «Итого:» near row 7 sits next to the cashback total. Formula results are read as
//! Excel cached them, so they can be compared with our own totals.

use std::io::Cursor;

use calamine::{Data, ExcelDateTime, ExcelDateTimeType, Range, Reader, open_workbook_auto_from_rs};
use serde::Serialize;
use time::Date;

use crate::cashback::RateBp;
use crate::dict::name_key;
use crate::mcc::{Mcc, parse_mcc};
use crate::money::Kopecks;
use crate::month::Month;

pub const SERVICE_SHEET: &str = "Служебный";

/// The first data row (0-based index of Excel row 3).
const FIRST_DATA_ROW: u32 = 2;

/// One row of a month sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRow {
    /// The row number as Excel shows it.
    pub sheet_row: u32,
    pub title: String,
    pub category: Option<String>,
    pub is_regular: bool,
    pub participant: Option<String>,
    #[serde(with = "crate::date::iso::option")]
    pub date: Option<Date>,
    pub amount: Kopecks,
    /// Column H, when filled: cashback is computed from it.
    pub rate: Option<RateBp>,
    /// Column I when H is empty: a cashback typed as an amount.
    pub cashback: Option<Kopecks>,
    pub card: Option<String>,
    /// Column K of our export.
    pub mcc: Option<Mcc>,
}

/// A totals column of row 2 and what Excel computed for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedColumn {
    /// The header of row 1, e.g. «Лечение».
    pub label: String,
    /// Categories its SUMIF adds up.
    pub categories: Vec<String>,
    pub amount: Kopecks,
}

/// The sheet's own totals, as cached by Excel.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Expected {
    pub total: Option<Kopecks>,
    pub columns: Vec<ExpectedColumn>,
    pub regular: Option<Kopecks>,
    pub cashback: Option<Kopecks>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetData {
    pub sheet: String,
    pub month: Month,
    pub rows: Vec<ImportRow>,
    pub expected: Expected,
}

/// Something in a cell that could not be read as is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Warning {
    pub sheet: String,
    pub row: u32,
    pub message: String,
}

/// A merged totals column: «Лечение/лекарства» = лечение + лекарства.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub name: String,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workbook {
    /// Categories of «Служебный», in sheet order.
    pub categories: Vec<String>,
    pub groups: Vec<Group>,
    /// Month sheets, oldest first.
    pub months: Vec<SheetData>,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    #[error("cannot read the workbook: {0}")]
    Read(String),
    #[error("the workbook has no «Служебный» sheet")]
    NoServiceSheet,
    #[error("the workbook has no month sheets (MM.YY)")]
    NoMonths,
}

/// Reads a workbook from its bytes (.xlsx; .xls and .ods work too).
pub fn read_workbook(bytes: &[u8]) -> Result<Workbook, ImportError> {
    let mut book = open_workbook_auto_from_rs(Cursor::new(bytes))
        .map_err(|e| ImportError::Read(e.to_string()))?;
    let sheet_names = book.sheet_names();
    if !sheet_names.iter().any(|n| n == SERVICE_SHEET) {
        return Err(ImportError::NoServiceSheet);
    }
    let service = book
        .worksheet_range(SERVICE_SHEET)
        .map_err(|e| ImportError::Read(e.to_string()))?;
    let service_names = service_categories(&service);
    let categories: Vec<String> = service_names.iter().map(|(_, n)| n.clone()).collect();

    let mut months = Vec::new();
    let mut warnings = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    for name in &sheet_names {
        let Ok(month) = Month::parse_sheet_name(name) else {
            continue;
        };
        let values = book
            .worksheet_range(name)
            .map_err(|e| ImportError::Read(e.to_string()))?;
        let formulas = book
            .worksheet_formula(name)
            .map_err(|e| ImportError::Read(e.to_string()))?;
        let expected = expected_totals(&values, &formulas, &service_names);
        for column in &expected.columns {
            if column.categories.len() > 1
                && !groups.iter().any(|g| g.categories == column.categories)
            {
                groups.push(Group {
                    name: group_name(&column.categories),
                    categories: column.categories.clone(),
                });
            }
        }
        let rows = read_rows(name, &values, &mut warnings);
        months.push(SheetData {
            sheet: name.clone(),
            month,
            rows,
            expected,
        });
    }
    if months.is_empty() {
        return Err(ImportError::NoMonths);
    }
    months.sort_by_key(|m| m.month);
    Ok(Workbook {
        categories,
        groups,
        months,
        warnings,
    })
}

/// «Лечение/лекарства» from [«лечение», «лекарства»], like the «Свод» headers.
fn group_name(categories: &[String]) -> String {
    let joined = categories.join("/");
    let mut chars = joined.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// (Excel row number, name) of the categories in column B, below the header «Справочник».
fn service_categories(range: &Range<Data>) -> Vec<(u32, String)> {
    let Some((_, end)) = range.end().map(|(r, _)| ((), r)) else {
        return Vec::new();
    };
    (1..=end)
        .filter_map(|row| text(range.get_value((row, 1))).map(|name| (row + 1, name)))
        .collect()
}

fn text(cell: Option<&Data>) -> Option<String> {
    let s = match cell? {
        Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => s.trim().to_owned(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => format_number(*f),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(_) | Data::Error(_) | Data::Empty => return None,
    };
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    (!s.is_empty()).then_some(s)
}

/// A number typed into a text column («7741»): without a trailing «.0».
fn format_number(f: f64) -> String {
    let s = f.to_string();
    s.strip_suffix(".0").map_or(s.clone(), str::to_owned)
}

/// The single place where the workbook's floating-point numbers become kopecks. Returns the
/// kopecks and whether the value had no fractions of a kopeck.
#[allow(clippy::float_arithmetic, clippy::cast_possible_truncation)]
fn kopecks_of(f: f64) -> Option<(Kopecks, bool)> {
    if !f.is_finite() || f.abs() > 1e13 {
        return None;
    }
    let scaled = f * 100.0;
    let rounded = scaled.round();
    Some((Kopecks(rounded as i64), (scaled - rounded).abs() < 1e-6))
}

fn number(cell: Option<&Data>) -> Option<f64> {
    match cell? {
        Data::Float(f) => Some(*f),
        #[allow(clippy::cast_precision_loss)]
        Data::Int(i) => Some(*i as f64),
        Data::String(s) => {
            let cleaned: String = s
                .chars()
                .filter(|c| !c.is_whitespace() && *c != '₽' && *c != '%')
                .map(|c| if c == ',' { '.' } else { c })
                .collect();
            cleaned.parse().ok()
        }
        _ => None,
    }
}

fn money(cell: Option<&Data>) -> Option<(Kopecks, bool)> {
    number(cell).and_then(kopecks_of)
}

/// A rate: Excel stores 5% as 0.05. A plain 5 (typed without the % format) is read as 5%.
#[allow(
    clippy::float_arithmetic,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn rate(cell: Option<&Data>) -> Option<RateBp> {
    let f = number(cell)?;
    let percent_string = matches!(cell, Some(Data::String(s)) if s.contains('%'));
    let bp = if percent_string || f > 1.0 {
        f * 100.0
    } else {
        f * 10_000.0
    };
    (bp.is_finite() && bp > 0.0 && bp <= 10_000.0).then(|| RateBp(bp.round() as u32))
}

fn date(cell: Option<&Data>) -> Result<Option<Date>, ()> {
    let serial = match cell {
        None | Some(Data::Empty) => return Ok(None),
        Some(Data::DateTime(dt)) => *dt,
        Some(Data::Float(f)) => ExcelDateTime::new(*f, ExcelDateTimeType::DateTime, false),
        Some(Data::DateTimeIso(s)) => {
            return crate::date::parse_iso(s.get(..10).unwrap_or(s))
                .map(Some)
                .ok_or(());
        }
        Some(Data::String(s)) => return parse_dotted(s).map(Some).ok_or(()),
        Some(_) => return Err(()),
    };
    let (y, m, d, ..) = serial.to_ymd_hms_milli();
    let month = time::Month::try_from(m).map_err(|_| ())?;
    Date::from_calendar_date(i32::from(y), month, d)
        .map(Some)
        .map_err(|_| ())
}

/// «05.07.2026» or «05.07.26».
fn parse_dotted(s: &str) -> Option<Date> {
    let mut parts = s.trim().split('.');
    let d: u8 = parts.next()?.parse().ok()?;
    let m: u8 = parts.next()?.parse().ok()?;
    let y: i32 = parts.next()?.parse().ok()?;
    let y = if y < 100 { 2000 + y } else { y };
    Date::from_calendar_date(y, time::Month::try_from(m).ok()?, d).ok()
}

/// Column K. It has no header of its own and older sheets keep other things there («Итого:» up
/// to 06.25), so whatever is not a code counts as empty, without a warning.
fn mcc(cell: Option<&Data>) -> Option<Mcc> {
    parse_mcc(text(cell).as_deref()).ok().flatten()
}

fn read_rows(sheet: &str, range: &Range<Data>, warnings: &mut Vec<Warning>) -> Vec<ImportRow> {
    let Some((last_row, _)) = range.end() else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for r in FIRST_DATA_ROW..=last_row {
        let cell = |c: u32| range.get_value((r, c));
        let excel_row = r + 1;
        let mut warn = |message: String| {
            warnings.push(Warning {
                sheet: sheet.to_owned(),
                row: excel_row,
                message,
            });
        };
        let title = text(cell(1)).unwrap_or_default();
        let amount = money(cell(6));
        if title.is_empty() && amount.is_none_or(|(a, _)| a.is_zero()) {
            continue;
        }
        let amount = match amount {
            Some((a, exact)) => {
                if !exact {
                    warn("сумма с долями копейки округлена до копейки".into());
                }
                a
            }
            None => {
                if cell(6).is_some_and(|c| *c != Data::Empty) {
                    warn("сумма не распознана, записан 0".into());
                }
                Kopecks::ZERO
            }
        };
        let date = date(cell(5)).unwrap_or_else(|()| {
            warn("дата не распознана".into());
            None
        });
        let rate = rate(cell(7));
        let cashback = if rate.is_some() {
            None
        } else {
            money(cell(8)).map(|(c, _)| c).filter(|c| !c.is_zero())
        };
        if rate.is_some()
            && let Some((cached, _)) = money(cell(8))
        {
            let computed = crate::cashback::cashback_for(
                amount,
                rate.unwrap_or_default(),
                crate::cashback::Rounding::FloorRub,
            );
            if cached != computed {
                warn(format!(
                    "кэшбэк в файле {} ₽, по проценту выходит {} ₽: взят процент",
                    cached.get() / 100,
                    computed.get() / 100
                ));
            }
        }
        rows.push(ImportRow {
            sheet_row: excel_row,
            title,
            category: text(cell(2)),
            is_regular: text(cell(3)).is_some_and(|v| name_key(&v) == "да"),
            participant: text(cell(4)),
            date,
            amount,
            rate,
            cashback,
            card: text(cell(9)),
            mcc: mcc(cell(10)),
        });
    }
    rows
}

/// Row numbers of «Служебный» referenced by a formula: `Служебный!B5`, `'Служебный'!$B$5`.
fn service_refs(formula: &str, column: char) -> Vec<u32> {
    let mut refs = Vec::new();
    let mut rest = formula;
    while let Some(pos) = rest.find(SERVICE_SHEET) {
        rest = &rest[pos + SERVICE_SHEET.len()..];
        let tail = rest
            .trim_start_matches('\'')
            .trim_start_matches('!')
            .trim_start_matches('$');
        let mut chars = tail.chars();
        if chars.next() == Some(column) {
            let digits: String = chars
                .skip_while(|c| *c == '$')
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(n) = digits.parse() {
                refs.push(n);
            }
        }
    }
    refs
}

fn expected_totals(
    values: &Range<Data>,
    formulas: &Range<String>,
    service: &[(u32, String)],
) -> Expected {
    // «Итого:» is a value next to a label, so it is checked on sheets without formulas too
    // (our own export, D29).
    let mut expected = Expected {
        cashback: cashback_total(values),
        ..Expected::default()
    };
    let Some((_, last_col)) = formulas.end() else {
        return expected;
    };
    let first_col = formulas.start().map_or(0, |(_, c)| c);
    for col in first_col..=last_col {
        let Some(formula) = formulas.get_value((1, col)).filter(|f| !f.is_empty()) else {
            continue;
        };
        let Some((amount, _)) = money(values.get_value((1, col))) else {
            continue;
        };
        let upper = formula.to_uppercase();
        if upper.starts_with("SUM(G") || upper.starts_with("=SUM(G") {
            expected.total = Some(amount);
        } else if upper.contains("SUMIF") {
            let categories: Vec<String> = service_refs(formula, 'B')
                .into_iter()
                .filter_map(|n| {
                    service
                        .iter()
                        .find(|(row, _)| *row == n)
                        .map(|(_, c)| c.clone())
                })
                .collect();
            if !categories.is_empty() {
                expected.columns.push(ExpectedColumn {
                    label: text(values.get_value((0, col))).unwrap_or_default(),
                    categories,
                    amount,
                });
            } else if !service_refs(formula, 'C').is_empty() {
                expected.regular = Some(amount);
            }
        }
    }
    expected
}

/// The value right of the «Итого:» label in the upper right block.
fn cashback_total(values: &Range<Data>) -> Option<Kopecks> {
    for row in 2..20 {
        for col in 9..20 {
            if text(values.get_value((row, col))).as_deref() == Some("Итого:") {
                return money(values.get_value((row, col + 1))).map(|(k, _)| k);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_references() {
        assert_eq!(
            service_refs(
                "SUMIF($C$4:$C$242, Служебный!B5,$G$4:$G$242)+SUMIF($C$4:$C$242, Служебный!B6,$G$4:$G$242)",
                'B'
            ),
            [5, 6]
        );
        assert_eq!(
            service_refs("SUMIF($D$3:$D$242, Служебный!C2,$G$3:$G$242)", 'C'),
            [2]
        );
        assert_eq!(
            service_refs("SUMIF($D$3:$D$242, Служебный!C2,$G$3:$G$242)", 'B'),
            Vec::<u32>::new()
        );
        assert_eq!(
            service_refs("SUMIF(C4:C9,'Служебный'!$B$12,G4:G9)", 'B'),
            [12]
        );
    }

    #[test]
    fn group_names_like_the_summary_sheet() {
        assert_eq!(
            group_name(&["лечение".into(), "лекарства".into()]),
            "Лечение/лекарства"
        );
        assert_eq!(
            group_name(&["одежда".into(), "обувь".into()]),
            "Одежда/обувь"
        );
    }

    #[test]
    fn numbers_from_cells() {
        assert_eq!(kopecks_of(1874.5), Some((Kopecks(187_450), true)));
        assert_eq!(kopecks_of(0.1 + 0.2), Some((Kopecks(30), true)));
        assert_eq!(kopecks_of(33.333), Some((Kopecks(3333), false)));
        assert_eq!(
            money(Some(&Data::String("1 234,56 ₽".into()))),
            Some((Kopecks(123_456), true))
        );
        assert_eq!(rate(Some(&Data::Float(0.05))), Some(RateBp(500)));
        assert_eq!(rate(Some(&Data::Float(0.015))), Some(RateBp(150)));
        assert_eq!(rate(Some(&Data::Float(5.0))), Some(RateBp(500)));
        assert_eq!(rate(Some(&Data::String("6%".into()))), Some(RateBp(600)));
        assert_eq!(rate(Some(&Data::Float(0.0))), None);
        assert_eq!(text(Some(&Data::Float(7741.0))).as_deref(), Some("7741"));
    }

    #[test]
    fn mcc_from_cells() {
        let code = |c: &str| Some(c.parse::<Mcc>().unwrap());
        assert_eq!(mcc(Some(&Data::String("0742".into()))), code("0742"));
        assert_eq!(mcc(Some(&Data::String(" 5411 ".into()))), code("5411"));
        assert_eq!(mcc(Some(&Data::Float(5411.0))), code("5411"));
        assert_eq!(mcc(Some(&Data::Int(5411))), code("5411"));
        for junk in [
            Data::Empty,
            Data::String("Итого:".into()),
            Data::String("54".into()),
            Data::Float(742.0),
            Data::Float(5411.5),
            Data::Bool(true),
        ] {
            assert_eq!(mcc(Some(&junk)), None, "{junk:?}");
        }
        assert_eq!(mcc(None), None);
    }

    #[test]
    fn dates_from_cells() {
        // 46208 is 2026-07-05 in Excel's 1900 date system.
        let excel = Data::DateTime(ExcelDateTime::new(
            46208.0,
            ExcelDateTimeType::DateTime,
            false,
        ));
        assert_eq!(date(Some(&excel)), Ok(crate::date::parse_iso("2026-07-05")));
        assert_eq!(
            date(Some(&Data::Float(46208.0))),
            Ok(crate::date::parse_iso("2026-07-05"))
        );
        assert_eq!(
            date(Some(&Data::String("05.07.26".into()))),
            Ok(crate::date::parse_iso("2026-07-05"))
        );
        assert_eq!(date(Some(&Data::Empty)), Ok(None));
        assert_eq!(date(Some(&Data::String("вчера".into()))), Err(()));
    }
}
