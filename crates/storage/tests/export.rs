//! Export (PLAN §9, D29–D31): the xlsx reads back through our import into the same journal,
//! «Свод» and the data sheets are there, the json carries everything stored.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;

use calamine::{Data, Reader, open_workbook_auto_from_rs};
use reestr_core::date::parse_iso;
use reestr_core::import::{CardSetup, Workbook, read_workbook};
use reestr_core::{
    DeadlineInput, EditInput, InflowInput, InflowOp, JournalOp, Kopecks, Month, ProductOp, RateBp,
    RuleInput, RuleOp, TrancheInput, month_totals,
};
use reestr_storage::Vault;
use tempfile::TempDir;

fn rub(r: i64) -> Kopecks {
    Kopecks::from_rubles(r)
}

fn cards(vault: &Vault, book: &Workbook) -> Vec<CardSetup> {
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

/// An entry as the sheet shows it: names instead of ids, cashback as an amount.
type Row = (
    String,
    Option<String>,
    bool,
    Option<String>,
    Option<String>,
    i64,
    i64,
    Option<String>,
);

fn rows(vault: &Vault, month: Month) -> Vec<Row> {
    let d = vault.dictionaries().unwrap();
    let name = |pairs: Vec<(i64, String)>, id: Option<i64>| {
        let map: HashMap<i64, String> = pairs.into_iter().collect();
        id.and_then(|id| map.get(&id).cloned())
    };
    vault
        .entries(month)
        .unwrap()
        .into_iter()
        .map(|e| {
            (
                e.title,
                name(
                    d.categories
                        .iter()
                        .map(|c| (c.id, c.name.clone()))
                        .collect(),
                    e.category_id,
                ),
                e.is_regular,
                name(
                    d.participants
                        .iter()
                        .map(|p| (p.id, p.name.clone()))
                        .collect(),
                    e.participant_id,
                ),
                e.date.map(reestr_core::date::format_iso),
                e.amount.get(),
                e.cashback.map_or(0, |c| c.amount.get()),
                name(
                    d.products
                        .iter()
                        .map(|p| (p.id, p.short_name.clone()))
                        .collect(),
                    e.product_id,
                ),
            )
        })
        .collect()
}

/// A cell as kopecks; the workbook stores rubles as floating point.
#[allow(clippy::float_arithmetic, clippy::cast_possible_truncation)]
fn kopecks(cell: Option<&Data>) -> Option<i64> {
    match cell? {
        Data::Float(f) => Some((f * 100.0).round() as i64),
        Data::Int(i) => Some(i * 100),
        _ => None,
    }
}

/// A file with every kind of data: the imported fixture, cashback by a rule with a limit on a
/// card the fixture does not use (so nothing else spends the limit), inflows, a plan, a
/// tranche and a reminder.
fn filled(dir: &TempDir) -> Vault {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/workbook.xlsx");
    let book = read_workbook(&std::fs::read(path).unwrap()).unwrap();
    let mut v = Vault::create(&dir.path().join("a.reestr"), "pw").unwrap();
    let cards = cards(&v, &book);
    v.import_workbook(&book, &cards).unwrap();

    let month = *v.months().unwrap().last().unwrap();
    v.apply_rules(
        month,
        vec![RuleOp::Append {
            edits: vec![
                RuleInput::Card(Some("Халва".into())),
                RuleInput::Category(None),
                RuleInput::Rate(RateBp(500)),
                RuleInput::CashbackCap(Some(rub(200))),
            ],
        }],
        true,
    )
    .unwrap();
    let purchase = |title: &str, amount: i64| JournalOp::Append {
        edits: vec![
            EditInput::Title(title.into()),
            EditInput::Category(Some("продукты".into())),
            EditInput::Amount(rub(amount)),
            EditInput::Card(Some("Халва".into())),
            EditInput::Date(parse_iso(&format!("{month}-20"))),
        ],
    };
    // 5% of 3 000 is 150 ₽; the second purchase gets what is left of the 200 ₽ limit.
    v.apply(
        month,
        vec![purchase("Гипермаркет", 3_000), purchase("Рынок", 4_000)],
    )
    .unwrap();
    v.apply_inflows(
        month,
        vec![InflowOp::Append {
            edits: vec![
                InflowInput::Amount(rub(95_000)),
                InflowInput::Source("Зарплата".into()),
            ],
        }],
    )
    .unwrap();
    v.set_plan(month, Some(rub(120_000))).unwrap();
    let otp = v
        .dictionaries()
        .unwrap()
        .products
        .iter()
        .find(|p| p.short_name == "Газпром КР")
        .unwrap()
        .id;
    let today = parse_iso("2026-10-27").unwrap();
    v.apply_products(
        vec![
            ProductOp::AddTranche {
                product: otp,
                edits: vec![
                    TrancheInput::Start(parse_iso(&format!("{month}-01")).unwrap()),
                    TrancheInput::End(parse_iso(&format!("{month}-28")).unwrap()),
                ],
            },
            ProductOp::AddDeadline {
                edits: vec![
                    DeadlineInput::Date(parse_iso("2026-11-15").unwrap()),
                    DeadlineInput::Title("Перевыпустить карту".into()),
                ],
            },
        ],
        today,
    )
    .unwrap();
    v
}

#[test]
fn xlsx_reads_back_into_the_same_journal() {
    let dir = TempDir::new().unwrap();
    let a = filled(&dir);
    let today = parse_iso("2026-10-27").unwrap();
    let months = a.months().unwrap();
    let last = *months.last().unwrap();
    let rule_rows: Vec<i64> = a
        .entries(last)
        .unwrap()
        .iter()
        .filter(|e| e.title == "Гипермаркет" || e.title == "Рынок")
        .map(|e| e.cashback.unwrap().amount.get())
        .collect();
    assert_eq!(rule_rows, [rub(150).get(), rub(50).get()]);

    let bytes = a.export_xlsx(today).unwrap();

    // The sheets: the three kinds of the source workbook in its order, then the data the source workbook does not have.
    let mut sheets = open_workbook_auto_from_rs(Cursor::new(bytes.clone())).unwrap();
    let mut expected = vec!["Служебный".to_owned()];
    expected.extend(months.iter().map(|m| m.sheet_name()));
    expected.extend(
        [
            "Свод",
            "Категории",
            "Участники",
            "Банки",
            "Карты",
            "Регулярные платежи",
            "Правила кэшбэка",
            "Поступления",
            "План дохода",
            "Грейсы",
            "Погашения",
            "Напоминания",
        ]
        .map(str::to_owned),
    );
    assert_eq!(sheets.sheet_names(), expected);

    // «Свод»: the month totals of the first year with entries, by groups like the source sheet.
    let year = a.summary_years().unwrap().into_iter().next().unwrap();
    let ours = a.year_summary(year, true).unwrap();
    let summary = sheets.worksheet_range("Свод").unwrap();
    assert_eq!(
        kopecks(summary.get_value((0, 0))),
        Some(i64::from(year) * 100)
    );
    for (i, m) in ours.months.iter().enumerate() {
        let cell = summary.get_value((u32::try_from(i).unwrap() + 1, 1));
        if m.has_data {
            assert_eq!(kopecks(cell), Some(m.total.get()), "{}", m.month);
        } else {
            assert!(matches!(cell, None | Some(Data::Empty)), "{}", m.month);
        }
    }
    let rules = sheets.worksheet_range("Правила кэшбэка").unwrap();
    assert_eq!(
        rules.get_value((1, 2)),
        Some(&Data::String("на всё".into()))
    );
    assert_eq!(kopecks(rules.get_value((1, 8))), Some(rub(200).get()));

    // Our import reads the export back: no warnings, its cashback total matches every month.
    let book = read_workbook(&bytes).unwrap();
    assert!(book.warnings.is_empty(), "{:?}", book.warnings);
    let mut b = Vault::create(&dir.path().join("b.reestr"), "pw").unwrap();
    let preview = b.import_preview(&book).unwrap();
    assert_eq!(preview.months.len(), months.len());
    for m in &preview.months {
        assert!(
            m.check.matches && !m.check.compared.is_empty(),
            "{}: {:?}",
            m.sheet,
            m.check.compared
        );
    }
    let setup = cards(&b, &book);
    b.import_workbook(&book, &setup).unwrap();
    assert_eq!(b.months().unwrap(), months);
    let (da, db) = (a.dictionaries().unwrap(), b.dictionaries().unwrap());
    for month in months {
        assert_eq!(rows(&b, month), rows(&a, month), "{month}");
        let ta = month_totals(
            &a.entries(month).unwrap(),
            &da.categories,
            &da.groups,
            &da.templates,
        );
        let tb = month_totals(
            &b.entries(month).unwrap(),
            &db.categories,
            &db.groups,
            &db.templates,
        );
        assert_eq!(
            (tb.total, tb.regular, tb.cashback, tb.uncategorized),
            (ta.total, ta.regular, ta.cashback, ta.uncategorized),
            "{month}"
        );
        let by_label = |t: &reestr_core::MonthTotals| {
            t.by_category
                .iter()
                .map(|c| (c.label.clone(), c.amount))
                .collect::<Vec<_>>()
        };
        assert_eq!(by_label(&tb), by_label(&ta), "{month}");
    }
}

#[test]
fn json_carries_everything_stored() {
    let dir = TempDir::new().unwrap();
    let a = filled(&dir);
    let today = parse_iso("2026-10-27").unwrap();
    let text = a.export_json(today).unwrap();
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();

    assert_eq!(doc["format"], "reestr");
    assert_eq!(doc["version"], 1);
    assert_eq!(doc["schemaVersion"], reestr_storage::SCHEMA_VERSION);
    let count = |key: &str| doc[key].as_array().map_or(0, Vec::len);
    let entries: usize = a
        .months()
        .unwrap()
        .into_iter()
        .map(|m| a.entries(m).unwrap().len())
        .sum();
    assert_eq!(count("entries"), entries);
    let d = a.dictionaries().unwrap();
    assert_eq!(count("categories"), d.categories.len());
    assert_eq!(count("products"), d.products.len());
    assert_eq!(count("participants"), d.participants.len());
    assert_eq!(count("cashbackRules"), 1);
    assert_eq!(count("inflows"), 1);
    assert_eq!(count("incomePlans"), 1);
    assert_eq!(count("gracePeriods"), 1);
    assert_eq!(count("reminders"), 1);
    assert_eq!(doc["inflows"][0]["amount"], rub(95_000).get());
    assert_eq!(doc["incomePlans"][0]["amount"], rub(120_000).get());
    // Stored data only: no computed totals and no change history (D31).
    assert!(doc.get("totals").is_none() && doc.get("changeLog").is_none());
    let rule_entry = doc["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["title"] == "Рынок")
        .unwrap();
    assert_eq!(rule_entry["cashback"]["source"], "rule");
    assert_eq!(rule_entry["cashback"]["amount"], rub(50).get());
}
