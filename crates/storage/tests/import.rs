//! Importing the workbook fixture of `reestr-core` into a file: totals stored in the file match
//! the sheet formulas, re-importing adds nothing, undo removes the import.

use std::path::PathBuf;

use reestr_core::cashback::effective_rate_bp;
use reestr_core::import::{CardSetup, Workbook, read_workbook};
use reestr_core::{
    DictInput, DictKind, DictOp, Kopecks, Month, ProductKind, month_totals, name_key,
};
use reestr_storage::Vault;
use tempfile::TempDir;

fn workbook() -> Workbook {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/workbook.xlsx");
    read_workbook(&std::fs::read(path).unwrap()).unwrap()
}

fn cards_from_guesses(vault: &Vault, book: &Workbook) -> Vec<CardSetup> {
    vault
        .import_preview(book)
        .unwrap()
        .cards
        .into_iter()
        .map(|c| CardSetup {
            short_name: c.short_name,
            bank: c.guess.bank,
            kind: c.guess.kind,
            participant: c.guess.participant,
        })
        .collect()
}

#[test]
fn imported_totals_match_the_sheets_and_reimport_adds_nothing() {
    let dir = TempDir::new().unwrap();
    let mut v = Vault::create(&dir.path().join("i.reestr"), "pw").unwrap();
    let book = workbook();

    let preview = v.import_preview(&book).unwrap();
    assert_eq!(preview.months.len(), 8);
    assert!(
        preview
            .months
            .iter()
            .all(|m| m.check.matches && m.already == 0)
    );
    let new_categories: Vec<_> = preview
        .categories
        .iter()
        .filter(|c| c.is_new)
        .map(|c| c.name.as_str())
        .collect();
    // The default categories of a new file are the workbook's; only «кино» is new.
    assert_eq!(new_categories, ["кино"]);
    assert!(preview.groups.iter().all(|g| !g.is_new));
    let otp = preview
        .cards
        .iter()
        .find(|c| c.short_name == "Газпром КР")
        .unwrap();
    assert_eq!(otp.guess.kind, Some(ProductKind::Credit));
    assert!(!otp.exists);

    let cards = cards_from_guesses(&v, &book);
    let report = v.import_workbook(&book, &cards).unwrap();
    assert_eq!(report.added, 3 * 53);
    assert_eq!(report.skipped, 0);
    let months: Vec<String> = report.months.iter().map(Month::to_string).collect();
    assert_eq!(months, ["2025-01", "2025-07", "2026-07"]);

    // Totals computed from the file equal what the sheet formulas computed.
    let d = v.dictionaries().unwrap();
    for sheet in book.months.iter().filter(|s| !s.rows.is_empty()) {
        let entries = v.entries(sheet.month).unwrap();
        assert_eq!(entries.len(), sheet.rows.len());
        let totals = month_totals(&entries, &d.categories, &d.groups, &d.templates);
        let e = &sheet.expected;
        assert_eq!(Some(totals.total), e.total, "{}", sheet.sheet);
        assert_eq!(Some(totals.regular), e.regular, "{}", sheet.sheet);
        assert_eq!(Some(totals.cashback), e.cashback, "{}", sheet.sheet);
        for column in &e.columns {
            let keys: Vec<String> = column.categories.iter().map(|c| name_key(c)).collect();
            let ours: Kopecks = totals
                .by_category
                .iter()
                .filter(|c| keys.contains(&name_key(&c.label)))
                .map(|c| c.amount)
                .sum();
            assert_eq!(ours, column.amount, "{} {}", sheet.sheet, column.label);
        }
        // «Ср. %» is the sheet's M7 / (H2 − G3): rent, row 3, is out of the base, the
        // «Интернет» rows in the same category are in (D37).
        let rent: Kopecks = sheet
            .rows
            .iter()
            .filter(|r| r.sheet_row == 3)
            .map(|r| r.amount)
            .sum();
        let (total, cashback) = (e.total.unwrap(), e.cashback.unwrap());
        assert_eq!(
            totals.cashback_rate_bp,
            effective_rate_bp(total - rent, cashback),
            "{}",
            sheet.sheet
        );
    }

    // Cards got what the wizard confirmed.
    let otp = d
        .products
        .iter()
        .find(|p| p.short_name == "Газпром КР")
        .unwrap();
    assert_eq!(otp.kind, Some(ProductKind::Credit));
    let bank = d.banks.iter().find(|b| Some(b.id) == otp.bank_id).unwrap();
    assert_eq!(bank.name, "Газпромбанк");

    // The same workbook again: nothing new.
    let again = v.import_workbook(&book, &cards).unwrap();
    assert_eq!((again.added, again.skipped), (0, 3 * 53));
    let preview = v.import_preview(&book).unwrap();
    assert!(preview.months.iter().all(|m| m.already == m.rows));
}

#[test]
fn undo_removes_the_whole_import() {
    let dir = TempDir::new().unwrap();
    let mut v = Vault::create(&dir.path().join("i.reestr"), "pw").unwrap();
    // The user took rent back into «Ср. %»; the import marks row 3's payment again (D37).
    let rent = v
        .dictionaries()
        .unwrap()
        .templates
        .iter()
        .find(|t| t.title == "Квартплата")
        .unwrap()
        .id;
    v.edit_dictionary(
        DictKind::Template,
        vec![DictOp::Edit {
            id: rent,
            edit: DictInput::ExcludeFromRate(false),
        }],
    )
    .unwrap();
    let before = v.dictionaries().unwrap();
    let book = workbook();
    let cards = cards_from_guesses(&v, &book);
    v.import_workbook(&book, &cards).unwrap();
    assert!(!v.months().unwrap().is_empty());
    let d = v.dictionaries().unwrap();
    assert!(
        d.templates
            .iter()
            .any(|t| t.id == rent && t.exclude_from_rate)
    );

    let undone = v.undo().unwrap().unwrap();
    assert_eq!(undone.label, "import");
    assert!(v.months().unwrap().is_empty());
    assert_eq!(v.dictionaries().unwrap(), before);
}
