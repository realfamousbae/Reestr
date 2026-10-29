//! Cashback rules in the file (PLAN §6, D21): automatic cashback, limits in journal order,
//! the recompute question, copying, undo, card suggestions.

use reestr_core::{
    Cashback, CashbackSource, DictKind, DictOp, EditInput, Entry, JournalOp, Kopecks, Month,
    RateBp, RuleError, RuleInput, RuleOp,
};
use reestr_storage::{MonthRules, StorageError, Vault};
use tempfile::TempDir;

const PW: &str = "test-password";

fn new_vault(dir: &TempDir) -> Vault {
    Vault::create(&dir.path().join("r.reestr"), PW).unwrap()
}

fn july() -> Month {
    "2026-07".parse().unwrap()
}

fn june() -> Month {
    "2026-06".parse().unwrap()
}

fn rub(r: i64) -> Kopecks {
    Kopecks::from_rubles(r)
}

fn rule(card: &str, category: Option<&str>, bp: u32) -> Vec<RuleInput> {
    vec![
        RuleInput::Card(Some(card.into())),
        RuleInput::Category(category.map(Into::into)),
        RuleInput::Rate(RateBp(bp)),
    ]
}

fn add_rules(vault: &mut Vault, month: Month, rules: Vec<Vec<RuleInput>>) {
    let ops = rules
        .into_iter()
        .map(|edits| RuleOp::Append { edits })
        .collect();
    vault.apply_rules(month, ops, true).unwrap();
}

fn rule_id(rules: &MonthRules, index: usize) -> i64 {
    rules.rules[index].id
}

fn purchase(category: &str, card: &str, amount: i64) -> Vec<EditInput> {
    vec![
        EditInput::Title("покупка".into()),
        EditInput::Category(Some(category.into())),
        EditInput::Card(Some(card.into())),
        EditInput::Amount(rub(amount)),
    ]
}

fn append(vault: &mut Vault, month: Month, edits: Vec<EditInput>) -> i64 {
    vault
        .apply(month, vec![JournalOp::Append { edits }])
        .unwrap()
        .appended[0]
}

fn edit(vault: &mut Vault, id: i64, edit: EditInput) {
    vault
        .apply(july(), vec![JournalOp::Edit { id, edit }])
        .unwrap();
}

fn entry(vault: &Vault, id: i64) -> Entry {
    let month = [july(), june()]
        .into_iter()
        .find(|m| vault.entries(*m).unwrap().iter().any(|e| e.id == id))
        .unwrap();
    vault
        .entries(month)
        .unwrap()
        .into_iter()
        .find(|e| e.id == id)
        .unwrap()
}

fn cashback(vault: &Vault, id: i64) -> Option<Kopecks> {
    entry(vault, id).cashback.map(|c| c.amount)
}

fn source(vault: &Vault, id: i64) -> Option<CashbackSource> {
    entry(vault, id).cashback.map(|c| c.source)
}

#[test]
fn rules_fill_cashback_and_limits_follow_row_order() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    let mut groceries = rule("Райф А", Some("продукты"), 500);
    groceries.push(RuleInput::SpendCap(Some(rub(3000))));
    add_rules(&mut v, july(), vec![groceries, rule("Райф А", None, 100)]);

    let first = append(&mut v, july(), purchase("продукты", "Райф А", 2000));
    let second = append(&mut v, july(), purchase("продукты", "Райф А", 2000));
    let food = append(&mut v, july(), purchase("еда", "Райф А", 1000));
    let other_card = append(&mut v, july(), purchase("еда", "Газпром КР", 1000));
    assert_eq!(cashback(&v, first), Some(rub(100)));
    assert_eq!(source(&v, first), Some(CashbackSource::Rule));
    // 1 000 ₽ left under the cap.
    assert_eq!(cashback(&v, second), Some(rub(50)));
    assert_eq!(entry(&v, second).display_rate_bp(), Some(250));
    // «на всё».
    assert_eq!(cashback(&v, food), Some(rub(10)));
    // No rule for this card.
    assert_eq!(cashback(&v, other_card), None);

    // Deleting the first row gives the second its full cashback; undo brings both back.
    v.delete_entries(&[first]).unwrap();
    assert_eq!(cashback(&v, second), Some(rub(100)));
    v.undo().unwrap();
    assert_eq!(cashback(&v, first), Some(rub(100)));
    assert_eq!(cashback(&v, second), Some(rub(50)));

    // A smaller first purchase frees the limit for the second.
    edit(&mut v, first, EditInput::Amount(rub(1000)));
    assert_eq!(cashback(&v, first), Some(rub(50)));
    assert_eq!(cashback(&v, second), Some(rub(100)));
    let rules = v.month_rules(july()).unwrap();
    assert_eq!(rules.usage[0].spent, rub(3000));
    assert_eq!(rules.usage[0].remaining_spend, Some(Kopecks::ZERO));
    assert!(rules.usage[0].is_exhausted());
    assert_eq!(rules.usage[1].spent, rub(1000));

    // Moving the second purchase to another category: it falls under «на всё».
    edit(&mut v, second, EditInput::Category(Some("разное".into())));
    assert_eq!(cashback(&v, second), Some(rub(20)));
}

#[test]
fn typed_cashback_stays_and_clearing_returns_to_the_rule() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    let mut groceries = rule("Райф А", Some("продукты"), 500);
    groceries.push(RuleInput::CashbackCap(Some(rub(150))));
    add_rules(&mut v, july(), vec![groceries]);

    let typed = append(&mut v, july(), purchase("продукты", "Райф А", 2000));
    edit(&mut v, typed, EditInput::Cashback(Some(rub(120))));
    let next = append(&mut v, july(), purchase("продукты", "Райф А", 2000));
    assert_eq!(source(&v, typed), Some(CashbackSource::ManualAmount));
    // The typed 120 ₽ use the cap: 30 ₽ are left.
    assert_eq!(cashback(&v, next), Some(rub(30)));

    // A typed percentage stays a percentage.
    edit(&mut v, typed, EditInput::Rate(Some(RateBp(300))));
    assert_eq!(cashback(&v, typed), Some(rub(60)));
    assert_eq!(cashback(&v, next), Some(rub(90)));

    // Clearing the cell: back to the rule, and the cap goes in row order again.
    edit(&mut v, typed, EditInput::Rate(None));
    assert_eq!(source(&v, typed), Some(CashbackSource::Rule));
    assert_eq!(cashback(&v, typed), Some(rub(100)));
    assert_eq!(cashback(&v, next), Some(rub(50)));

    // Zero typed as an amount: no cashback on purpose.
    edit(&mut v, typed, EditInput::Cashback(Some(Kopecks::ZERO)));
    assert_eq!(cashback(&v, typed), Some(Kopecks::ZERO));
    assert_eq!(source(&v, typed), Some(CashbackSource::ManualAmount));
}

#[test]
fn a_rule_change_asks_and_recomputes_or_keeps() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    add_rules(&mut v, july(), vec![rule("Райф А", Some("продукты"), 500)]);
    let a = append(&mut v, july(), purchase("продукты", "Райф А", 2000));
    let b = append(&mut v, july(), purchase("продукты", "Райф А", 1000));
    let id = rule_id(&v.month_rules(july()).unwrap(), 0);
    let to_seven = || {
        vec![RuleOp::Edit {
            id,
            edit: RuleInput::Rate(RateBp(700)),
        }]
    };

    // The question: two rows would change. Asking changes nothing.
    assert_eq!(v.rules_impact(july(), to_seven()).unwrap(), 2);
    assert_eq!(cashback(&v, a), Some(rub(100)));
    assert!(v.can_undo());

    // «Пересчитать».
    let outcome = v.apply_rules(july(), to_seven(), true).unwrap();
    assert_eq!(outcome.changed, 2);
    assert_eq!(cashback(&v, a), Some(rub(140)));
    assert_eq!(cashback(&v, b), Some(rub(70)));
    // Undo takes back the rule and the cashback together.
    v.undo().unwrap();
    assert_eq!(v.month_rules(july()).unwrap().rules[0].rate, RateBp(500));
    assert_eq!(cashback(&v, a), Some(rub(100)));

    // «Оставить как есть»: the rows keep 5% as typed cashback; new rows get 7%.
    let outcome = v.apply_rules(july(), to_seven(), false).unwrap();
    assert_eq!(outcome.changed, 0);
    assert_eq!(cashback(&v, a), Some(rub(100)));
    assert_eq!(
        entry(&v, a).cashback,
        Some(Cashback {
            amount: rub(100),
            rate: Some(RateBp(500)),
            source: CashbackSource::ManualPercent
        })
    );
    let c = append(&mut v, july(), purchase("продукты", "Райф А", 1000));
    assert_eq!(cashback(&v, c), Some(rub(70)));

    // A note changes no cashback: no question.
    let note = vec![RuleOp::Edit {
        id,
        edit: RuleInput::Notes(Some("акция до 31.07".into())),
    }];
    assert_eq!(v.rules_impact(july(), note).unwrap(), 0);
}

#[test]
fn a_new_rule_fills_blank_cashback_only_when_asked() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    let a = append(&mut v, july(), purchase("продукты", "Райф А", 2000));
    let typed = append(&mut v, july(), purchase("продукты", "Райф А", 1000));
    edit(&mut v, typed, EditInput::Cashback(Some(rub(15))));
    assert_eq!(cashback(&v, a), None);

    let new_rule = || {
        vec![RuleOp::Append {
            edits: rule("Райф А", None, 300),
        }]
    };
    assert_eq!(v.rules_impact(july(), new_rule()).unwrap(), 1);
    v.apply_rules(july(), new_rule(), false).unwrap();
    assert_eq!(cashback(&v, a), None);
    v.undo().unwrap();

    v.apply_rules(july(), new_rule(), true).unwrap();
    assert_eq!(cashback(&v, a), Some(rub(60)));
    assert_eq!(cashback(&v, typed), Some(rub(15)));

    // Deleting the rule takes its cashback away (after the same question).
    let id = rule_id(&v.month_rules(july()).unwrap(), 0);
    let delete = || vec![RuleOp::Delete { ids: vec![id] }];
    assert_eq!(v.rules_impact(july(), delete()).unwrap(), 1);
    v.apply_rules(july(), delete(), true).unwrap();
    assert_eq!(cashback(&v, a), None);
    assert!(v.month_rules(july()).unwrap().rules.is_empty());
}

#[test]
fn copy_rules_from_the_previous_month() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    let mut groceries = rule("Райф А", Some("продукты"), 500);
    groceries.push(RuleInput::SpendCap(Some(rub(30_000))));
    groceries.push(RuleInput::Adjustment(rub(1200)));
    groceries.push(RuleInput::Notes(Some("до 1 000 ₽".into())));
    add_rules(
        &mut v,
        june(),
        vec![groceries, rule("Газпром КР", None, 150)],
    );
    add_rules(&mut v, july(), vec![rule("Газпром КР", None, 200)]);

    let july_rules = v.month_rules(july()).unwrap();
    assert_eq!(july_rules.copy_from, Some(june()));
    assert_eq!(v.month_rules(june()).unwrap().copy_from, None);

    v.apply_rules(july(), vec![RuleOp::CopyFrom { from: june() }], true)
        .unwrap();
    let rules = v.month_rules(july()).unwrap().rules;
    assert_eq!(rules.len(), 2);
    // The July rule for Газпром КР stays as it was.
    assert_eq!(rules[0].rate, RateBp(200));
    let copied = &rules[1];
    assert_eq!(copied.month, july());
    assert_eq!(copied.rate, RateBp(500));
    assert_eq!(copied.spend_cap, Some(rub(30_000)));
    assert_eq!(copied.spent_adjustment, Kopecks::ZERO);
    assert_eq!(copied.notes.as_deref(), Some("до 1 000 ₽"));

    // Undo shows the month of the rules.
    let replayed = v.undo().unwrap().unwrap();
    assert_eq!(replayed.label, "rules");
    assert_eq!(replayed.months, ["2026-07"]);
    assert_eq!(v.month_rules(july()).unwrap().rules.len(), 1);
}

#[test]
fn rule_errors() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    add_rules(&mut v, july(), vec![rule("Райф А", Some("продукты"), 500)]);
    let again = vec![RuleOp::Append {
        edits: rule("райф а", Some("Продукты"), 300),
    }];
    assert!(matches!(
        v.apply_rules(july(), again, true),
        Err(StorageError::DuplicateRule)
    ));
    // The same card «на всё» is another rule; «на всё» typed is the same as empty.
    let everything = vec![RuleOp::Append {
        edits: rule("Райф А", Some("на всё"), 100),
    }];
    v.apply_rules(july(), everything, true).unwrap();
    let rules = v.month_rules(july()).unwrap();
    assert_eq!(rules.rules[1].category_id, None);
    let id = rule_id(&rules, 1);

    let set = |edit| vec![RuleOp::Edit { id, edit }];
    assert!(matches!(
        v.apply_rules(
            july(),
            set(RuleInput::Category(Some("продукты".into()))),
            true
        ),
        Err(StorageError::DuplicateRule)
    ));
    assert!(matches!(
        v.apply_rules(july(), set(RuleInput::Rate(RateBp(10_001))), true),
        Err(StorageError::Rule(RuleError::RateTooHigh(10_001)))
    ));
    assert!(matches!(
        v.apply_rules(july(), set(RuleInput::SpendCap(Some(-rub(1)))), true),
        Err(StorageError::Rule(RuleError::NegativeLimit))
    ));
    assert!(matches!(
        v.apply_rules(july(), set(RuleInput::Card(None)), true),
        Err(StorageError::EmptyName)
    ));
    // A failed action leaves nothing behind.
    assert_eq!(v.month_rules(july()).unwrap().rules[1].rate, RateBp(100));

    // Cards and categories used by rules cannot be deleted.
    let d = v.dictionaries().unwrap();
    let card = d.products[0].id;
    assert!(matches!(
        v.edit_dictionary(DictKind::Product, vec![DictOp::Delete { ids: vec![card] }]),
        Err(StorageError::InUse { count: 2 })
    ));
    let groceries = d
        .categories
        .iter()
        .find(|c| c.name == "продукты")
        .unwrap()
        .id;
    assert!(matches!(
        v.edit_dictionary(
            DictKind::Category,
            vec![DictOp::Delete {
                ids: vec![groceries]
            }]
        ),
        Err(StorageError::InUse { count: 1 })
    ));
}

#[test]
fn card_suggestions_and_their_setting() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    assert!(!v.card_suggestions_enabled().unwrap());
    assert!(v.card_suggestions(july(), None).unwrap().is_empty());

    let mut raif = rule("Райф А", Some("продукты"), 1000);
    raif.push(RuleInput::CashbackCap(Some(rub(100))));
    add_rules(
        &mut v,
        july(),
        vec![
            raif,
            rule("Газпром КР", Some("продукты"), 500),
            rule("Альфа", None, 150),
        ],
    );
    // The first rules switch suggestions on until the user decides.
    assert!(v.card_suggestions_enabled().unwrap());
    let d = v.dictionaries().unwrap();
    let card = |name: &str| d.products.iter().find(|p| p.short_name == name).unwrap().id;

    // 900 ₽ of groceries already went on Райф А: 10 ₽ of cashback left there.
    append(&mut v, july(), purchase("продукты", "Райф А", 900));
    let row = append(&mut v, july(), purchase("продукты", "Газпром КР", 1000));
    let s = v.card_suggestions(july(), Some(row)).unwrap();
    let order: Vec<_> = s.iter().map(|c| (c.product_id, c.expected)).collect();
    // The row itself does not use any limit for its own suggestion.
    assert_eq!(
        order,
        vec![
            (card("Газпром КР"), Some(rub(50))),
            (card("Альфа"), Some(rub(15))),
            (card("Райф А"), Some(rub(10)))
        ]
    );
    // A new row has no category: only «на всё» rules.
    let s = v.card_suggestions(july(), None).unwrap();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].product_id, card("Альфа"));

    // Inactive cards are not suggested.
    v.edit_dictionary(
        DictKind::Product,
        vec![DictOp::Edit {
            id: card("Альфа"),
            edit: reestr_core::DictInput::IsActive(false),
        }],
    )
    .unwrap();
    assert!(v.card_suggestions(july(), None).unwrap().is_empty());

    v.set_card_suggestions(false).unwrap();
    assert!(!v.card_suggestions_enabled().unwrap());
    v.set_card_suggestions(true).unwrap();
    assert!(v.card_suggestions_enabled().unwrap());
}

#[test]
fn rent_is_out_of_the_cashback_rate() {
    // A new file has «Квартплата» in «квартиры», out of «Ср. %» (D37).
    let dir = TempDir::new().unwrap();
    let v = new_vault(&dir);
    let d = v.dictionaries().unwrap();
    let excluded: Vec<_> = d
        .templates
        .iter()
        .filter(|t| t.exclude_from_rate)
        .map(|t| {
            let category = d.categories.iter().find(|c| Some(c.id) == t.category_id);
            (t.title.as_str(), category.map(|c| c.name.as_str()))
        })
        .collect();
    assert_eq!(excluded, [("Квартплата", Some("квартиры"))]);
}

/// D23: a month of the family's kind with limits worked out by hand.
///
/// Rules of 07.26:
/// - Райф А, продукты 5%, spending up to 15 000 ₽, adjustment 2 000 ₽ (bought in the bank's
///   app, not in the journal);
/// - Райф А, на всё 1%;
/// - Газпром КР, еда 10%, at most 300 ₽ of cashback;
/// - ПСБ 3170, на всё 3%, rounding to kopecks.
#[test]
fn acceptance_month_with_hand_computed_limits() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    let mut groceries = rule("Райф А", Some("продукты"), 500);
    groceries.push(RuleInput::SpendCap(Some(rub(15_000))));
    groceries.push(RuleInput::Adjustment(rub(2000)));
    let mut cafe = rule("Газпром КР", Some("еда"), 1000);
    cafe.push(RuleInput::CashbackCap(Some(rub(300))));
    add_rules(
        &mut v,
        july(),
        vec![
            groceries,
            rule("Райф А", None, 100),
            cafe,
            rule("ПСБ 3170", None, 300),
        ],
    );
    let d = v.dictionaries().unwrap();
    let mts = d
        .products
        .iter()
        .find(|p| p.short_name == "ПСБ 3170")
        .unwrap()
        .id;
    v.edit_dictionary(
        DictKind::Product,
        vec![DictOp::Edit {
            id: mts,
            edit: reestr_core::DictInput::Rounding(reestr_core::Rounding::FloorKop),
        }],
    )
    .unwrap();

    let rows = [
        // Groceries on Райф А: 2 000 (adjustment) + 6 400 + 5 120,50 = 13 520,50.
        ("продукты", "Райф А", Kopecks(640_000)), // 320 ₽
        ("продукты", "Райф А", Kopecks(512_050)), // 256,0250 → 256 ₽
        // A refund: −1 000 ₽ takes back 50 ₽ and frees the limit.
        ("продукты", "Райф А", Kopecks(-100_000)), // −50 ₽
        // 15 000 − 12 520,50 = 2 479,50 left: 2 479,50 × 5% = 123,975 → 123 ₽.
        ("продукты", "Райф А", Kopecks(400_000)), // 123 ₽, limit used up
        ("продукты", "Райф А", Kopecks(80_000)),  // 0 ₽
        // Other things on Райф А: «на всё» 1%.
        ("одежда", "Райф А", Kopecks(459_900)), // 45,99 → 45 ₽
        // Cafe on Газпром КР: 10% up to 300 ₽.
        ("еда", "Газпром КР", Kopecks(185_000)), // 185 ₽
        ("еда", "Газпром КР", Kopecks(140_000)), // 140 → 115 ₽ left
        ("еда", "Газпром КР", Kopecks(30_000)),  // 0 ₽
        // Газпром КР has no rule for transport.
        ("транспорт", "Газпром КР", Kopecks(250_000)), // none
        // ПСБ 3170: 3% of everything, to kopecks.
        ("автомобиль", "ПСБ 3170", Kopecks(351_275)), // 105,3825 → 105,38 ₽
        ("квартиры", "ПСБ 3170", Kopecks(784_000)),   // 235,20 ₽
    ];
    let ids: Vec<i64> = rows
        .iter()
        .map(|(category, card, amount)| {
            let mut edits = purchase(category, card, 0);
            edits.pop();
            edits.push(EditInput::Amount(*amount));
            append(&mut v, july(), edits)
        })
        .collect();
    let expected = [
        Some(Kopecks(32_000)),
        Some(Kopecks(25_600)),
        Some(Kopecks(-5_000)),
        Some(Kopecks(12_300)),
        Some(Kopecks::ZERO),
        Some(Kopecks(4_500)),
        Some(Kopecks(18_500)),
        Some(Kopecks(11_500)),
        Some(Kopecks::ZERO),
        None,
        Some(Kopecks(10_538)),
        Some(Kopecks(23_520)),
    ];
    let actual: Vec<_> = ids.iter().map(|id| cashback(&v, *id)).collect();
    assert_eq!(actual, expected);

    let rules = v.month_rules(july()).unwrap();
    let usage: Vec<_> = rules
        .usage
        .iter()
        .map(|u| (u.spent, u.cashback, u.remaining_spend, u.remaining_cashback))
        .collect();
    assert_eq!(
        usage,
        [
            // 2 000 + 6 400 + 5 120,50 − 1 000 + 4 000 + 800 = 17 320,50 ₽ spent;
            // cashback 100 (adjustment) + 320 + 256 − 50 + 123 + 0 = 749 ₽.
            (Kopecks(1_732_050), rub(749), Some(Kopecks::ZERO), None),
            (Kopecks(459_900), rub(45), None, None),
            (rub(3550), rub(300), None, Some(Kopecks::ZERO)),
            (Kopecks(1_135_275), Kopecks(34_058), None, None),
        ]
    );
    assert!(rules.usage[0].is_exhausted());
    assert!(!rules.usage[1].is_exhausted());
    assert!(rules.usage[2].is_exhausted());
}

#[test]
fn a_row_being_typed_on_the_phone_gets_card_suggestions() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    add_rules(
        &mut v,
        july(),
        vec![
            rule("Газпром КР", Some("продукты"), 500),
            rule("Халва", None, 200),
        ],
    );
    let cards = |v: &Vault, category: Option<&str>| -> Vec<(String, Option<Kopecks>)> {
        let d = v.dictionaries().unwrap();
        v.card_suggestions_draft(july(), category, Some(rub(1000)))
            .unwrap()
            .into_iter()
            .map(|s| {
                let card = d.products.iter().find(|p| p.id == s.product_id).unwrap();
                (card.short_name.clone(), s.expected)
            })
            .collect()
    };
    assert_eq!(
        cards(&v, Some("Продукты")),
        [
            ("Газпром КР".to_owned(), Some(rub(50))),
            ("Халва".to_owned(), Some(rub(20)))
        ]
    );
    // Without a category, or with one nobody has a rule for, only «на всё» counts.
    assert_eq!(cards(&v, None), [("Халва".to_owned(), Some(rub(20)))]);
    assert_eq!(
        cards(&v, Some("кино")),
        [("Халва".to_owned(), Some(rub(20)))]
    );
}
