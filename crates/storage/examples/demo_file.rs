//! Builds a complete DEMO `.reestr` file for the user-manual screenshots.
//!
//! ```text
//! cargo run -p reestr-storage --example demo_file -- <output.reestr>
//! ```
//!
//! The file password is the constant `demo-reestr` (see [`PASSWORD`]). This is a throwaway
//! demo file with fictional data: never use that password for a real file.
//!
//! What it does: creates the file, imports the synthetic workbook of the core fixtures (its
//! participants, cards and entries are fictional), completes the cards, fills in every feature
//! (cashback rules, income plan and inflows, regular payment templates, credit card tranches and
//! repayments, savings accounts, reminders) and finally wipes the undo history so the demo opens
//! clean. «Today» of the demo is fixed at 2026-12-20.
//!
//! It refuses to overwrite an existing file.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use reestr_core::date::parse_iso;
use reestr_core::import::{CardSetup, read_workbook};
use reestr_core::{
    CreditInput, DeadlineInput, DictInput, DictKind, DictOp, EditInput, InflowInput, InflowOp,
    JournalOp, Kopecks, Month, ProductKind, ProductOp, RateBp, RepaymentInput, RuleInput, RuleOp,
    SavingsInput, TrancheInput, rule_of_entry,
};
use reestr_storage::Vault;
use rusqlite::Connection;
use time::Date;

/// Password of the demo file. Throwaway: the data is fictional.
const PASSWORD: &str = "demo-reestr";

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn rub(rubles: i64) -> Kopecks {
    Kopecks::from_rubles(rubles)
}

fn month(iso: &str) -> Month {
    iso.parse().expect("a valid month")
}

fn date(iso: &str) -> Date {
    parse_iso(iso).expect("a valid date")
}

/// The fixed «today» of the demo.
fn today() -> Date {
    date("2026-12-20")
}

fn main() -> ExitCode {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: demo_file <output.reestr>");
        return ExitCode::from(2);
    };
    if path.exists() {
        eprintln!(
            "{} already exists; refusing to overwrite it",
            path.display()
        );
        return ExitCode::from(1);
    }
    match build(&path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("failed: {error}");
            // Do not leave a half-built demo file behind.
            let _ = std::fs::remove_file(&path);
            ExitCode::from(1)
        }
    }
}

fn build(path: &Path) -> Result {
    let mut vault = Vault::create(path, PASSWORD)?;
    import(&mut vault)?;
    savings(&mut vault)?;
    cashback_rules(&mut vault)?;
    income(&mut vault)?;
    templates(&mut vault)?;
    products(&mut vault)?;
    scan(&vault)?;
    vault.close()?;

    wipe_history(path)?;
    let vault = Vault::open(path, PASSWORD)?;
    scan(&vault)?;
    summary(&vault)?;
    vault.close()?;
    Ok(())
}

/// The cards of the synthetic workbook: (name, bank, participant, kind).
const CARDS: &[(&str, &str, &str, ProductKind)] = &[
    ("Д ВТБ", "ВТБ", "Андрей", ProductKind::Debit),
    ("Райф А", "Райффайзенбанк", "Андрей", ProductKind::Debit),
    ("Райф М", "Райффайзенбанк", "Марина", ProductKind::Debit),
    ("Совком А", "Совкомбанк", "Андрей", ProductKind::Debit),
    ("Озон", "Озон Банк", "Марина", ProductKind::Debit),
    ("Газпром КР", "Газпромбанк", "Андрей", ProductKind::Credit),
    ("Росб 4512", "Росбанк", "Андрей", ProductKind::Debit),
    ("Почта А", "Почта Банк", "Андрей", ProductKind::Credit),
    ("ПСБ 3170", "ПСБ", "Марина", ProductKind::Debit),
];

/// Step 2: the synthetic workbook, with every card set up in the import wizard.
fn import(vault: &mut Vault) -> Result {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/workbook_2026.xlsx");
    let book = read_workbook(&std::fs::read(source)?)?;
    let cards = vault
        .import_preview(&book)?
        .cards
        .into_iter()
        .map(|c| {
            let (_, bank, who, kind) = CARDS
                .iter()
                .find(|(name, ..)| *name == c.short_name)
                .ok_or_else(|| format!("{} is not a demo card", c.short_name))?;
            Ok(CardSetup {
                short_name: c.short_name,
                bank: Some((*bank).into()),
                kind: Some(*kind),
                participant: Some((*who).into()),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    vault.import_workbook(&book, &cards)?;
    Ok(())
}

/// Step 3: two savings products that the workbook does not have.
fn savings(vault: &mut Vault) -> Result {
    let mut ops = Vec::new();
    for (name, bank, who, kind) in [
        ("ВТБ Сейф", "ВТБ", "Марина", ProductKind::Savings),
        ("Совком Вклад", "Совкомбанк", "Андрей", ProductKind::Deposit),
    ] {
        ops.push(DictOp::Append {
            edits: vec![
                DictInput::Name(name.into()),
                DictInput::Bank(Some(bank.into())),
                DictInput::Participant(Some(who.into())),
                DictInput::Kind(Some(kind)),
            ],
        });
    }
    vault.edit_dictionary(DictKind::Product, ops)?;
    Ok(())
}

/// Step 4: cashback rules of the last two months, with the journal following them.
fn cashback_rules(vault: &mut Vault) -> Result {
    let rule =
        |card: &str, category: Option<&str>, bp: u32, spend: Option<i64>, cap: Option<i64>| {
            RuleOp::Append {
                edits: vec![
                    RuleInput::Card(Some(card.into())),
                    RuleInput::Category(category.map(Into::into)),
                    RuleInput::Rate(RateBp(bp)),
                    RuleInput::SpendCap(spend.map(rub)),
                    RuleInput::CashbackCap(cap.map(rub)),
                ],
            }
        };
    for m in [month("2026-11"), month("2026-12")] {
        let ops = vec![
            rule("Райф А", Some("продукты"), 500, Some(30_000), Some(1_000)),
            rule("Совком А", None, 100, None, None),
            rule("Почта А", Some("одежда"), 500, None, Some(300)),
            rule("Райф М", Some("еда"), 700, None, None),
        ];
        vault.apply_rules(m, ops, true)?;

        // Purchases that use the Райф А and Райф М limits: November spends its «продукты»
        // limit up, December is left with room.
        let purchases: &[(&str, &str, &str, &str, i64)] = if m == month("2026-11") {
            &[
                ("Перекрёсток", "продукты", "Райф А", "2026-11-03", 14_300),
                ("Магнит", "продукты", "Райф А", "2026-11-11", 8_900),
                ("Азбука вкуса", "продукты", "Райф А", "2026-11-22", 7_600),
                ("Кофейня", "еда", "Райф М", "2026-11-14", 1_250),
            ]
        } else {
            &[
                ("Перекрёсток", "продукты", "Райф А", "2026-12-02", 9_600),
                ("Магнит", "продукты", "Райф А", "2026-12-09", 5_480),
                ("Азбука вкуса", "продукты", "Райф А", "2026-12-16", 3_150),
                ("Пиццерия", "еда", "Райф М", "2026-12-12", 2_450),
                ("Кофейня", "еда", "Райф М", "2026-12-19", 1_890),
            ]
        };
        let appended = purchases
            .iter()
            .map(|(title, category, card, day, amount)| JournalOp::Append {
                edits: vec![
                    EditInput::Title((*title).into()),
                    EditInput::Category(Some((*category).into())),
                    EditInput::Participant(Some("Андрей".into())),
                    EditInput::Date(Some(date(day))),
                    EditInput::Amount(rub(*amount)),
                    EditInput::Card(Some((*card).into())),
                ],
            })
            .collect();
        vault.apply(m, appended)?;

        // Entries that already have a typed cashback keep it; let the covered ones follow the
        // rules so that the limits show real usage.
        let rules = vault.month_rules(m)?.rules;
        let covered: Vec<_> = vault
            .entries(m)?
            .into_iter()
            .filter(|e| rule_of_entry(&rules, e).is_some())
            .map(|e| JournalOp::Edit {
                id: e.id,
                edit: EditInput::Cashback(None),
            })
            .collect();
        vault.apply(m, covered)?;
    }
    vault.set_card_suggestions(true)?;
    Ok(())
}

/// Step 5: the income plan and the inflows.
fn income(vault: &mut Vault) -> Result {
    vault.set_plan(month("2026-01"), Some(rub(150_000)))?;
    vault.set_plan(month("2026-09"), Some(rub(165_000)))?;

    let inflow = |source: &str, day: &str, who: &str, card: &str, amount: i64| InflowOp::Append {
        edits: vec![
            InflowInput::Source(source.into()),
            InflowInput::Date(Some(date(day))),
            InflowInput::Participant(Some(who.into())),
            InflowInput::Card(Some(card.into())),
            InflowInput::Amount(rub(amount)),
        ],
    };
    vault.apply_inflows(
        month("2026-11"),
        vec![
            inflow("Зарплата", "2026-11-05", "Марина", "Райф М", 52_000),
            inflow("Аванс", "2026-11-10", "Андрей", "Д ВТБ", 38_000),
            inflow("Подработка", "2026-11-17", "Андрей", "Д ВТБ", 18_000),
            inflow("Зарплата", "2026-11-25", "Андрей", "Д ВТБ", 60_000),
        ],
    )?;
    vault.apply_inflows(
        month("2026-12"),
        vec![
            inflow("Зарплата", "2026-12-05", "Марина", "Райф М", 52_000),
            inflow("Аванс", "2026-12-10", "Андрей", "Д ВТБ", 38_000),
            inflow("Подработка", "2026-12-17", "Андрей", "Д ВТБ", 21_500),
            inflow("Зарплата", "2026-12-25", "Андрей", "Д ВТБ", 62_000),
        ],
    )?;
    Ok(())
}

/// Step 6 and 9: regular payment templates, their rows in the last two months, and a row
/// without a category.
fn templates(vault: &mut Vault) -> Result {
    let rent = vault
        .dictionaries()?
        .templates
        .into_iter()
        .find(|t| t.exclude_from_rate)
        .ok_or("the seeded rent template is missing")?;
    // The rent keeps «Не в Ср. %»; only its payer and amount are filled in.
    let mut ops = vec![
        DictOp::Edit {
            id: rent.id,
            edit: DictInput::Participant(Some("Андрей".into())),
        },
        DictOp::Edit {
            id: rent.id,
            edit: DictInput::Card(Some("Д ВТБ".into())),
        },
        DictOp::Edit {
            id: rent.id,
            edit: DictInput::DefaultAmount(Some(rub(7_840))),
        },
    ];
    let new = [
        ("Интернет", "разное", "Андрей", "Д ВТБ", 650),
        ("Связь", "разное", "Марина", "Райф М", 800),
        ("Подписка", "отдых", "Марина", "Озон", 499),
    ];
    for (title, category, who, card, amount) in new {
        ops.push(DictOp::Append {
            edits: vec![
                DictInput::Name(title.into()),
                DictInput::Category(Some(category.into())),
                DictInput::Participant(Some(who.into())),
                DictInput::Card(Some(card.into())),
                DictInput::DefaultAmount(Some(rub(amount))),
            ],
        });
    }
    vault.edit_dictionary(DictKind::Template, ops)?;

    let ids: Vec<i64> = vault
        .dictionaries()?
        .templates
        .iter()
        .filter(|t| new.iter().any(|(title, ..)| *title == t.title))
        .map(|t| t.id)
        .collect();
    for m in [month("2026-11"), month("2026-12")] {
        vault.add_from_templates(m, &ids)?;
    }

    vault.apply(
        month("2026-12"),
        vec![JournalOp::Append {
            edits: vec![
                EditInput::Title("Перевод родителям".into()),
                EditInput::IsRegular(false),
                EditInput::Participant(Some("Андрей".into())),
                EditInput::Date(Some(date("2026-12-18"))),
                EditInput::Amount(rub(5_000)),
                EditInput::Card(Some("Д ВТБ".into())),
            ],
        }],
    )?;
    Ok(())
}

/// Steps 7 and 8: credit cards, savings accounts and reminders.
fn products(vault: &mut Vault) -> Result {
    let id_of = |vault: &Vault, name: &str| -> Result<i64> {
        vault
            .dictionaries()?
            .products
            .into_iter()
            .find(|p| p.short_name == name)
            .map(|p| p.id)
            .ok_or_else(|| format!("no card {name}").into())
    };
    let gazprom = id_of(vault, "Газпром КР")?;
    let pochta = id_of(vault, "Почта А")?;
    let safe = id_of(vault, "ВТБ Сейф")?;
    let deposit = id_of(vault, "Совком Вклад")?;
    let today = today();

    let tranche = |product, start: &str, end: &str, adjustment: i64| ProductOp::AddTranche {
        product,
        edits: vec![
            TrancheInput::Start(date(start)),
            TrancheInput::End(date(end)),
            TrancheInput::Adjustment(rub(adjustment)),
        ],
    };
    vault.apply_products(
        vec![
            ProductOp::CreditTerms {
                product: gazprom,
                edit: CreditInput::CreditLimit(Some(rub(150_000))),
            },
            ProductOp::CreditTerms {
                product: gazprom,
                edit: CreditInput::GraceDays(Some(100)),
            },
            // Overdue, partly repaid.
            tranche(gazprom, "2026-08-10", "2026-11-18", 30_000),
            // Ends in four days: orange in «Сроки».
            tranche(gazprom, "2026-09-15", "2026-12-24", 0),
            // Fresh.
            tranche(gazprom, "2026-12-01", "2027-03-11", 0),
            ProductOp::CreditTerms {
                product: pochta,
                edit: CreditInput::CreditLimit(Some(rub(60_000))),
            },
            ProductOp::CreditTerms {
                product: pochta,
                edit: CreditInput::GraceDays(Some(55)),
            },
            tranche(pochta, "2026-12-05", "2027-01-29", 0),
            ProductOp::SavingsTerms {
                product: safe,
                edit: SavingsInput::PromoRate(Some(RateBp(1_800))),
            },
            ProductOp::SavingsTerms {
                product: safe,
                edit: SavingsInput::ClosedAt(Some(date("2026-10-01"))),
            },
            ProductOp::SavingsTerms {
                product: safe,
                edit: SavingsInput::ReopenAfterDays(Some(90)),
            },
            ProductOp::SavingsTerms {
                product: deposit,
                edit: SavingsInput::PromoRate(Some(RateBp(1_500))),
            },
        ],
        today,
    )?;

    let overdue = vault
        .products(today)?
        .credit
        .into_iter()
        .find(|c| c.product_id == gazprom)
        .and_then(|c| {
            c.tranches
                .into_iter()
                .find(|t| t.start == date("2026-08-10"))
        })
        .ok_or("the overdue tranche is missing")?;
    vault.apply_products(
        vec![
            // Bound to a tranche by hand.
            ProductOp::AddRepayment {
                product: gazprom,
                edits: vec![
                    RepaymentInput::Date(date("2026-11-02")),
                    RepaymentInput::Amount(rub(7_000)),
                    RepaymentInput::Tranche(overdue.id),
                ],
            },
            // No tranche given: goes where a repayment goes by default.
            ProductOp::AddRepayment {
                product: gazprom,
                edits: vec![
                    RepaymentInput::Date(date("2026-12-14")),
                    RepaymentInput::Amount(rub(10_000)),
                ],
            },
            ProductOp::AddDeadline {
                edits: vec![
                    DeadlineInput::Title("Перевыпустить карту".into()),
                    DeadlineInput::Card(Some("Росб 4512".into())),
                    DeadlineInput::Date(date("2026-12-26")),
                ],
            },
            ProductOp::AddDeadline {
                edits: vec![
                    DeadlineInput::Title("Закрыть вклад".into()),
                    DeadlineInput::Card(Some("Совком Вклад".into())),
                    DeadlineInput::Date(date("2027-03-01")),
                ],
            },
            ProductOp::AddDeadline {
                edits: vec![
                    DeadlineInput::Title("Продлить страховку".into()),
                    DeadlineInput::Date(date("2026-12-05")),
                    DeadlineInput::Done(true),
                ],
            },
        ],
        today,
    )?;
    Ok(())
}

/// Panics if the file holds a bank other than the demo's ones.
fn scan(vault: &Vault) -> Result {
    let d = vault.dictionaries()?;
    let banks: Vec<&str> = d.banks.iter().map(|b| b.name.as_str()).collect();
    let mut expected = vec![
        "ВТБ",
        "Райффайзенбанк",
        "Совкомбанк",
        "Озон Банк",
        "Газпромбанк",
        "Росбанк",
        "Почта Банк",
        "ПСБ",
    ];
    let mut actual = banks.clone();
    expected.sort_unstable();
    actual.sort_unstable();
    assert_eq!(actual, expected, "unexpected banks");
    Ok(())
}

/// The undo history of the build is dropped and the file is compacted: the demo opens clean.
fn wipe_history(path: &Path) -> Result {
    let conn = Connection::open(path)?;
    conn.execute_batch(&format!("PRAGMA key = '{PASSWORD}';"))?;
    conn.execute_batch("DELETE FROM change_log; DELETE FROM change_op; VACUUM;")?;
    conn.close().map_err(|(_, e)| e)?;
    Ok(())
}

/// Kopecks as «rubles.kopecks» for the printed summary.
fn money(k: Kopecks) -> String {
    format!("{}.{:02}", k.get() / 100, k.get().abs() % 100)
}

fn summary(vault: &Vault) -> Result {
    let d = vault.dictionaries()?;
    let months = vault.months()?;
    let mut entries = 0;
    for m in &months {
        entries += vault.entries(*m)?.len();
    }
    println!(
        "months: {} ({} .. {}), entries: {entries}",
        months.len(),
        months[0],
        months[months.len() - 1]
    );
    println!(
        "participants: {:?}",
        d.participants.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
    println!(
        "banks: {:?}",
        d.banks.iter().map(|b| &b.name).collect::<Vec<_>>()
    );
    println!("cards:");
    for p in &d.products {
        let kind = p.kind.map_or("?", ProductKind::code);
        println!("  {} [{kind}]", p.short_name);
    }
    println!(
        "templates: {:?}",
        d.templates.iter().map(|t| &t.title).collect::<Vec<_>>()
    );

    let data = vault.products(today())?;
    let tranches: usize = data.credit.iter().map(|c| c.tranches.len()).sum();
    let repayments: usize = data.credit.iter().map(|c| c.repayments.len()).sum();
    println!(
        "tranches: {tranches}, repayments: {repayments}, reminders: {}, savings: {}, debt: {}",
        data.reminders.len(),
        data.savings.len(),
        money(data.debt)
    );
    for card in &data.credit {
        for s in &card.states {
            println!(
                "  tranche {}: debt {} repaid {} remaining {} days_left {} {:?}",
                s.id,
                money(s.debt),
                money(s.repaid),
                money(s.remaining),
                s.days_left,
                s.status
            );
        }
    }
    for dl in &data.deadlines {
        println!(
            "  deadline {} {:?} {:?} ({} days)",
            dl.date, dl.source, dl.urgency, dl.days_left
        );
    }
    for s in &data.savings {
        println!("  savings {} {:?}", s.product_id, s.state);
    }

    let mut inflows = 0;
    let mut rules = 0;
    for m in [month("2026-11"), month("2026-12")] {
        inflows += vault.inflows(m)?.len();
        let r = vault.month_rules(m)?;
        rules += r.rules.len();
        for (rule, usage) in r.rules.iter().zip(&r.usage) {
            println!(
                "  {m} rule {} {:?}: spent {} cashback {} exhausted {}",
                rule.product_id,
                rule.category_id,
                money(usage.spent),
                money(usage.cashback),
                usage.is_exhausted()
            );
        }
    }
    println!(
        "inflows: {inflows}, rules: {rules}, plan 2026-12: {:?}, suggestions: {}",
        vault.plan(month("2026-12"))?.map(|p| p.amount),
        vault.card_suggestions_enabled()?
    );
    Ok(())
}
