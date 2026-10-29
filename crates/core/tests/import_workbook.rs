//! Stage 1 acceptance (PLAN §12): a workbook in the source structure imports, and the
//! totals of every month equal what its formulas computed, to the kopeck.
//!
//! `fixtures/workbook.xlsx` is a synthetic workbook with fictional entries, written by
//! `tools/fixtures/make_workbook.py` and recalculated by LibreOffice (an independent formula
//! engine), so the cached values are what Excel would show.

use std::path::PathBuf;

use reestr_core::import::{check_sheet, import_keys, read_workbook};
use reestr_core::{Kopecks, RateBp, Rounding};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(path).unwrap()
}

#[test]
fn totals_match_the_sheet_formulas_to_the_kopeck() {
    let book = read_workbook(&fixture("workbook.xlsx")).unwrap();
    let filled: Vec<_> = book.months.iter().filter(|m| !m.rows.is_empty()).collect();
    assert_eq!(
        filled.iter().map(|m| m.sheet.as_str()).collect::<Vec<_>>(),
        ["01.25", "07.25", "07.26"]
    );
    for sheet in &book.months {
        let check = check_sheet(sheet, &book.categories, Rounding::FloorRub);
        let wrong: Vec<_> = check.compared.iter().filter(|c| !c.matches()).collect();
        assert!(check.matches, "{}: {wrong:#?}", sheet.sheet);
        if !sheet.rows.is_empty() {
            // Итого, 13 category columns, Регулярные, Кэшбэк.
            assert_eq!(check.compared.len(), 16, "{}", sheet.sheet);
        }
    }
}

#[test]
fn the_check_notices_a_different_rounding() {
    // Rounding cashback to kopecks instead of ROUNDDOWN to rubles must show up as a mismatch.
    let book = read_workbook(&fixture("workbook.xlsx")).unwrap();
    let july = book.months.iter().find(|m| m.sheet == "07.26").unwrap();
    let check = check_sheet(july, &book.categories, Rounding::FloorKop);
    assert!(!check.matches);
    let wrong: Vec<_> = check
        .compared
        .iter()
        .filter(|c| !c.matches())
        .map(|c| c.label.as_str())
        .collect();
    assert_eq!(wrong, ["Кэшбэк"]);
}

#[test]
fn dictionaries_of_the_workbook() {
    let book = read_workbook(&fixture("workbook.xlsx")).unwrap();
    assert_eq!(book.categories.len(), 16);
    assert_eq!(book.categories[0], "еда");
    assert_eq!(book.categories[13], "Транспорт");
    let groups: Vec<_> = book.groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(
        groups,
        ["Лечение/лекарства", "Красота/косметика", "Одежда/обувь"]
    );
    assert_eq!(book.groups[0].categories, ["лечение", "лекарства"]);
    // «Свод» and «Служебный» are not months; every MM.YY sheet is, oldest first.
    let sheets: Vec<_> = book.months.iter().map(|m| m.sheet.as_str()).collect();
    assert_eq!(
        sheets,
        [
            "01.25", "02.25", "03.25", "04.25", "06.25", "07.25", "06.26", "07.26"
        ]
    );
}

#[test]
fn rows_are_read_like_the_sheet_shows_them() {
    let book = read_workbook(&fixture("workbook.xlsx")).unwrap();
    let july = book.months.iter().find(|m| m.sheet == "07.26").unwrap();

    let rent = &july.rows[0];
    assert_eq!(rent.sheet_row, 3);
    assert_eq!(
        (
            rent.title.as_str(),
            rent.category.as_deref(),
            rent.is_regular
        ),
        ("Квартплата", Some("квартиры"), true)
    );
    assert_eq!(rent.amount, Kopecks::from_rubles(7840));
    assert_eq!(rent.date, reestr_core::date::parse_iso("2026-07-01"));
    assert_eq!(rent.card.as_deref(), Some("Д ВТБ"));

    let certificate = july.rows.iter().find(|r| r.title == "Сертификат").unwrap();
    assert_eq!(certificate.rate, None);
    assert_eq!(certificate.cashback, Some(Kopecks::from_rubles(200)));

    let refund = july
        .rows
        .iter()
        .find(|r| r.title == "Возврат куртки")
        .unwrap();
    assert_eq!(refund.amount, Kopecks(-299_990));
    assert_eq!(refund.rate, Some(RateBp(500)));

    let later = july
        .rows
        .iter()
        .find(|r| r.title == "Записать позже")
        .unwrap();
    assert_eq!(later.amount, Kopecks::ZERO);
    assert!(book.warnings.is_empty(), "{:#?}", book.warnings);
}

#[test]
fn import_keys_are_unique_and_stable() {
    let a = read_workbook(&fixture("workbook.xlsx")).unwrap();
    let b = read_workbook(&fixture("workbook.xlsx")).unwrap();
    for (x, y) in a.months.iter().zip(&b.months) {
        let keys = import_keys(x);
        assert_eq!(keys, import_keys(y));
        let mut unique = keys.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), keys.len(), "{}", x.sheet);
    }
}
