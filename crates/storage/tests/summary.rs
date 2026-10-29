//! Inflows, the income plan and «Свод» (PLAN §8, stage 3b). The acceptance test compares «Свод»
//! of 2026 with the source sheet, computed by LibreOffice from its formulas, wherever the
//! sheet has no error.

use std::io::Cursor;
use std::path::PathBuf;

use calamine::{Data, Range, Reader, open_workbook_auto_from_rs};
use reestr_core::date::parse_iso;
use reestr_core::import::{CardSetup, read_workbook};
use reestr_core::{EditInput, InflowInput, InflowOp, JournalOp, Kopecks, Month, name_key};
use reestr_storage::Vault;
use tempfile::TempDir;

fn m(s: &str) -> Month {
    s.parse().unwrap()
}

fn rub(r: i64) -> Kopecks {
    Kopecks::from_rubles(r)
}

#[test]
fn inflows_plan_and_the_month_balance() {
    let dir = TempDir::new().unwrap();
    let mut v = Vault::create(&dir.path().join("s.reestr"), "pw").unwrap();
    v.apply(
        m("2026-07"),
        vec![JournalOp::Append {
            edits: vec![
                EditInput::Title("Пятёрочка".into()),
                EditInput::Amount(rub(30_000)),
                EditInput::Cashback(Some(rub(500))),
            ],
        }],
    )
    .unwrap();
    v.set_plan(m("2026-01"), Some(rub(60_000))).unwrap();
    let created = v
        .apply_inflows(
            m("2026-07"),
            vec![
                InflowOp::Append {
                    edits: vec![
                        InflowInput::Source("Зарплата".into()),
                        InflowInput::Amount(rub(50_000)),
                        InflowInput::Participant(Some("Андрей".into())),
                        InflowInput::Date(parse_iso("2026-07-05")),
                    ],
                },
                InflowOp::Append {
                    edits: vec![
                        InflowInput::Source("Аванс".into()),
                        InflowInput::Amount(rub(2_000)),
                    ],
                },
            ],
        )
        .unwrap();
    assert_eq!(created.len(), 1);

    // The plan set in January carries over to July.
    let b = v.month_balance(m("2026-07")).unwrap();
    assert_eq!(b.plan.unwrap().set_in, m("2026-01"));
    assert_eq!(b.inflow, rub(52_000));
    assert_eq!(b.planned, Some(rub(30_000)));
    assert_eq!(b.actual, Some(rub(22_000)));
    assert_eq!(b.overall, Some(rub(30_500)));

    // A plan of its own from July on; removing it brings January's back.
    v.set_plan(m("2026-07"), Some(rub(45_000))).unwrap();
    assert_eq!(v.plan(m("2026-09")).unwrap().unwrap().amount, rub(45_000));
    assert_eq!(v.plan(m("2026-06")).unwrap().unwrap().amount, rub(60_000));
    v.set_plan(m("2026-07"), None).unwrap();
    assert_eq!(v.plan(m("2026-09")).unwrap().unwrap().amount, rub(60_000));
    let undone = v.undo().unwrap().unwrap();
    assert_eq!(undone.label, "plan");
    assert_eq!(undone.months, ["2026-07"]);
    assert_eq!(v.plan(m("2026-09")).unwrap().unwrap().amount, rub(45_000));

    // Inflows are edited and deleted as usual; the year summary sees them.
    let inflows = v.inflows(m("2026-07")).unwrap();
    assert_eq!(inflows[0].source, "Зарплата");
    v.apply_inflows(
        m("2026-07"),
        vec![
            InflowOp::Edit {
                id: inflows[0].id,
                edit: InflowInput::Amount(rub(51_000)),
            },
            InflowOp::Delete {
                ids: vec![inflows[1].id],
            },
        ],
    )
    .unwrap();
    let s = v.year_summary(2026, false).unwrap();
    assert_eq!(s.months[6].inflow, rub(51_000));
    assert_eq!(s.months[6].plan, Some(rub(45_000)));
    assert_eq!(v.summary_years().unwrap(), [2026]);

    // An inflow is money that came in: zero is refused and nothing is written.
    assert!(
        v.apply_inflows(
            m("2026-07"),
            vec![InflowOp::Edit {
                id: inflows[0].id,
                edit: InflowInput::Amount(Kopecks::ZERO),
            }],
        )
        .is_err()
    );
    assert_eq!(v.inflows(m("2026-07")).unwrap()[0].amount, rub(51_000));
}

/// A cell of the fixture's «Свод» as Excel cached it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Cached {
    Number(f64),
    Whole(i64),
    Error,
    Empty,
}

fn cached(range: &Range<Data>, row: u32, col: u32) -> Cached {
    match range.get_value((row, col)) {
        Some(Data::Float(f)) => Cached::Number(*f),
        Some(Data::Int(i)) => Cached::Whole(*i),
        Some(Data::Error(_)) => Cached::Error,
        _ => Cached::Empty,
    }
}

// The only floats here: Excel's cached results, turned into kopecks and basis points to be
// compared with integer money (the core never uses floats).
#[allow(clippy::float_arithmetic, clippy::cast_possible_truncation)]
fn kopecks(value: f64) -> Kopecks {
    Kopecks((value * 100.0).round() as i64)
}

#[allow(clippy::float_arithmetic, clippy::cast_possible_truncation)]
fn bp(value: f64) -> i64 {
    (value * 10_000.0).round() as i64
}

impl Cached {
    fn kopecks(self) -> Option<Kopecks> {
        match self {
            Self::Number(f) => Some(kopecks(f)),
            Self::Whole(i) => Some(Kopecks::from_rubles(i)),
            Self::Error | Self::Empty => None,
        }
    }

    fn bp(self) -> Option<i64> {
        match self {
            Self::Number(f) => Some(bp(f)),
            Self::Whole(i) => Some(i * 10_000),
            Self::Error | Self::Empty => None,
        }
    }
}

fn text(range: &Range<Data>, row: u32, col: u32) -> String {
    match range.get_value((row, col)) {
        Some(Data::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// Compares what the sheet has with what we computed; counts compared cells.
struct Check {
    compared: usize,
    mismatches: Vec<String>,
}

impl Check {
    fn money(&mut self, what: &str, sheet: Cached, ours: Option<Kopecks>) {
        if let Some(expected) = sheet.kopecks() {
            self.compared += 1;
            if Some(expected) != ours {
                self.mismatches
                    .push(format!("{what}: sheet {expected:?}, ours {ours:?}"));
            }
        }
    }

    fn rate(&mut self, what: &str, sheet: Cached, ours: Option<i64>) {
        if let Some(expected) = sheet.bp() {
            self.compared += 1;
            if Some(expected) != ours {
                self.mismatches
                    .push(format!("{what}: sheet {expected} bp, ours {ours:?}"));
            }
        }
    }
}

#[test]
fn summary_2026_matches_his_sheet() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/workbook_2026.xlsx");
    let bytes = std::fs::read(path).unwrap();
    let book = read_workbook(&bytes).unwrap();

    let dir = TempDir::new().unwrap();
    let mut v = Vault::create(&dir.path().join("s.reestr"), "pw").unwrap();
    let cards: Vec<CardSetup> = v
        .import_preview(&book)
        .unwrap()
        .cards
        .into_iter()
        .map(|c| CardSetup {
            short_name: c.short_name,
            bank: c.guess.bank,
            kind: c.guess.kind,
            participant: c.guess.participant,
        })
        .collect();
    v.import_workbook(&book, &cards).unwrap();
    // «Оклад» 55 000 in every month of the sheet: set once, it carries over.
    v.set_plan(m("2025-01"), Some(rub(55_000))).unwrap();
    let ours = v.year_summary(2026, true).unwrap();

    let mut sheets = open_workbook_auto_from_rs(Cursor::new(bytes)).unwrap();
    let range = sheets.worksheet_range("Свод").unwrap();
    // The 2026 block: its header row has 2026 in column C.
    let header = (0..range.height() as u32)
        .find(|r| cached(&range, *r, 2).kopecks() == Some(Kopecks::from_rubles(2026)))
        .expect("the 2026 block");
    // Columns H… up to «Регулярные» are the categories and groups of the month header.
    let (d, e, f, h) = (3, 4, 5, 7);
    let regular = (h..30)
        .find(|c| name_key(&text(&range, header, *c)) == "регулярные")
        .unwrap();
    let (cashback, overall, rate) = (regular + 1, regular + 2, regular + 3);
    let sheet_columns = (h..regular)
        .map(|c| name_key(&text(&range, header, c)).replace(' ', ""))
        .collect::<Vec<_>>();
    // Headers may be spelled differently (a typo, a short form): match by the first three letters.
    for (i, label) in sheet_columns.iter().enumerate() {
        let ourlabel = name_key(&ours.columns[i].label).replace(' ', "");
        assert_eq!(
            label.chars().take(3).collect::<String>(),
            ourlabel.chars().take(3).collect::<String>(),
            "column {i}"
        );
    }

    let mut check = Check {
        compared: 0,
        mismatches: Vec::new(),
    };
    for (i, row) in ours.months.iter().enumerate() {
        let r = header + 1 + i as u32;
        let what = |col: &str| format!("{} {col}", row.month);
        check.money(&what("Сумма"), cached(&range, r, d), Some(row.total));
        check.money(&what("Оклад"), cached(&range, r, e), row.plan);
        check.rate(&what("Прирост"), cached(&range, r, f), row.growth_bp);
        for (c, amount) in row.columns.iter().enumerate().take(sheet_columns.len()) {
            check.money(
                &what(&sheet_columns[c]),
                cached(&range, r, h + c as u32),
                Some(*amount),
            );
        }
        check.money(
            &what("Регулярные"),
            cached(&range, r, regular),
            Some(row.regular),
        );
        check.money(
            &what("Кешбек"),
            cached(&range, r, cashback),
            Some(row.cashback),
        );
        check.money(
            &what("Общий баланс"),
            cached(&range, r, overall),
            row.overall_balance,
        );
        check.rate(
            &what("% кешбека"),
            cached(&range, r, rate),
            row.cashback_rate_bp,
        );
    }

    // The footer: «Сумма», «От общ. %», «Среднее», «Ср. прирост» and the growth in percent.
    let first_footer = header + 13;
    let label = |r: u32| text(&range, r, 6);
    assert_eq!(label(first_footer), "Сумма");
    assert_eq!(label(first_footer + 1), "От общ. %");
    assert_eq!(label(first_footer + 2), "Среднее");
    assert_eq!(label(first_footer + 3), "Ср. прирост");
    let footer = &ours.footer;
    // The «Сумма» row holds the average month in column E, «От общ. %» the year's total.
    check.money(
        "среднее в месяц",
        cached(&range, first_footer, e),
        footer.average_total,
    );
    check.rate(
        "рост среднего",
        cached(&range, first_footer, f),
        footer.average_total_growth_bp,
    );
    check.money(
        "итого за год",
        cached(&range, first_footer + 1, e),
        Some(footer.total),
    );
    for (c, name) in sheet_columns.iter().enumerate() {
        let col = h + c as u32;
        check.money(
            &format!("сумма {name}"),
            cached(&range, first_footer, col),
            Some(footer.sums.columns[c]),
        );
        check.rate(
            &format!("доля {name}"),
            cached(&range, first_footer + 1, col),
            footer.shares_bp.columns[c],
        );
        check.money(
            &format!("среднее {name}"),
            cached(&range, first_footer + 2, col),
            footer.averages.columns[c],
        );
        check.money(
            &format!("прирост {name}"),
            cached(&range, first_footer + 3, col),
            footer.average_growth.columns[c],
        );
        check.rate(
            &format!("прирост % {name}"),
            cached(&range, first_footer + 4, col),
            footer.average_growth_bp.columns[c],
        );
    }
    for (col, name, sum, share, avg) in [
        (
            regular,
            "регулярные",
            footer.sums.regular,
            footer.shares_bp.regular,
            footer.averages.regular,
        ),
        (
            cashback,
            "кешбек",
            footer.sums.cashback,
            footer.shares_bp.cashback,
            footer.averages.cashback,
        ),
    ] {
        check.money(
            &format!("сумма {name}"),
            cached(&range, first_footer, col),
            Some(sum),
        );
        check.rate(
            &format!("доля {name}"),
            cached(&range, first_footer + 1, col),
            share,
        );
        check.money(
            &format!("среднее {name}"),
            cached(&range, first_footer + 2, col),
            avg,
        );
    }
    check.money(
        "общий баланс за год",
        cached(&range, first_footer + 3, overall),
        footer.overall_balance,
    );

    assert!(check.mismatches.is_empty(), "{:#?}", check.mismatches);
    // Every cell of the 2026 block without an error: 12 months × (Сумма, Оклад, 13 columns,
    // Регулярные, Кешбек, Общий баланс, %), the growth of January and July (the only months
    // 2025 has), and the footer: the average month and the year's total, 13 columns × (sum,
    // share, average), «Регулярные» and «Кешбек» × 3, the year's overall balance. The growth
    // rows are #REF! in the fixture: 2025 is incomplete.
    assert_eq!(check.compared, 12 * 19 + 2 + 2 + 13 * 3 + 2 * 3 + 1);
}
