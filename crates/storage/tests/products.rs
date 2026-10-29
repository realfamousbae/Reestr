//! Credit cards, savings accounts and reminders in the file (PLAN §7, D27, D28).

use reestr_core::date::parse_iso;
use reestr_core::{
    CreditInput, DeadlineInput, DeadlineSource, DictInput, DictKind, DictOp, EditInput, JournalOp,
    Kopecks, Month, ProductError, ProductKind, ProductOp, RateBp, RepaymentInput, SavingsInput,
    SavingsState, TrancheInput, TrancheStatus, Urgency,
};
use reestr_storage::{CreditCard, StorageError, Vault};
use tempfile::TempDir;
use time::Date;

fn d(s: &str) -> Date {
    parse_iso(s).unwrap()
}

fn rub(r: i64) -> Kopecks {
    Kopecks::from_rubles(r)
}

/// A file with a credit card «Газпром КР» and a savings account «Альфа НС».
fn setup(dir: &TempDir) -> (Vault, i64, i64) {
    let mut v = Vault::create(&dir.path().join("p.reestr"), "pw").unwrap();
    v.edit_dictionary(
        DictKind::Product,
        vec![
            DictOp::Append {
                edits: vec![
                    DictInput::Name("Газпром КР".into()),
                    DictInput::Kind(Some(ProductKind::Credit)),
                ],
            },
            DictOp::Append {
                edits: vec![
                    DictInput::Name("Альфа НС".into()),
                    DictInput::Kind(Some(ProductKind::Savings)),
                ],
            },
        ],
    )
    .unwrap();
    let products = v.dictionaries().unwrap().products;
    let id = |name: &str| products.iter().find(|p| p.short_name == name).unwrap().id;
    let (card, savings) = (id("Газпром КР"), id("Альфа НС"));
    (v, card, savings)
}

fn purchase(v: &mut Vault, month: &str, date: Option<&str>, card: &str, amount: i64) -> i64 {
    let month: Month = month.parse().unwrap();
    let mut edits = vec![
        EditInput::Title("покупка".into()),
        EditInput::Card(Some(card.into())),
        EditInput::Amount(rub(amount)),
    ];
    if let Some(date) = date {
        edits.push(EditInput::Date(parse_iso(date)));
    }
    v.apply(month, vec![JournalOp::Append { edits }])
        .unwrap()
        .appended[0]
}

fn tranche(start: &str, end: &str) -> Vec<TrancheInput> {
    vec![TrancheInput::Start(d(start)), TrancheInput::End(d(end))]
}

fn card_of(v: &Vault, card: i64, today: &str) -> CreditCard {
    v.products(d(today))
        .unwrap()
        .credit
        .into_iter()
        .find(|c| c.product_id == card)
        .unwrap()
}

#[test]
fn purchases_find_their_tranches_and_repayments_reduce_the_debt() {
    let dir = TempDir::new().unwrap();
    let (mut v, card, _) = setup(&dir);
    v.apply_products(
        vec![
            ProductOp::AddTranche {
                product: card,
                edits: tranche("2026-06-01", "2026-09-28"),
            },
            ProductOp::AddTranche {
                product: card,
                edits: tranche("2026-07-01", "2026-10-28"),
            },
        ],
        d("2026-07-01"),
    )
    .unwrap();
    let early = purchase(&mut v, "2026-04", Some("2026-04-20"), "Газпром КР", 700);
    purchase(&mut v, "2026-06", Some("2026-06-10"), "Газпром КР", 3000);
    purchase(&mut v, "2026-07", Some("2026-07-02"), "Газпром КР", 1500);
    let undated = purchase(&mut v, "2026-07", None, "Газпром КР", 200);

    let c = card_of(&v, card, "2026-08-01");
    let june = c.tranches[0].id;
    let july = c.tranches[1].id;
    assert_eq!(c.states[0].debt, rub(3000));
    assert_eq!(c.states[1].debt, rub(1500));
    assert_eq!(c.debt, rub(4500));
    // Before the first tranche and without a date: not in any tranche.
    assert_eq!(c.unbound, 2);
    assert!(
        c.purchases
            .iter()
            .any(|p| p.entry.id == undated && p.tranche.is_none())
    );
    // The April purchase is older than 90 days: not listed.
    assert!(c.purchases.iter().all(|p| p.entry.id != early));
    assert_eq!(c.repayment_tranche, Some(june));

    // A repayment without a tranche goes to the one ending first (D27); partial.
    v.apply_products(
        vec![ProductOp::AddRepayment {
            product: card,
            edits: vec![RepaymentInput::Amount(rub(1000))],
        }],
        d("2026-08-05"),
    )
    .unwrap();
    let c = card_of(&v, card, "2026-08-05");
    assert_eq!(c.repayments[0].grace_period_id, june);
    assert_eq!(c.repayments[0].date, d("2026-08-05"));
    assert_eq!(c.states[0].remaining, rub(2000));
    assert_eq!(c.debt, rub(3500));

    // The rest of June is paid: the tranche is closed; the next repayment goes to July.
    v.apply_products(
        vec![ProductOp::AddRepayment {
            product: card,
            edits: vec![RepaymentInput::Amount(rub(2000))],
        }],
        d("2026-08-06"),
    )
    .unwrap();
    let c = card_of(&v, card, "2026-10-29");
    assert_eq!(c.states[0].status, TrancheStatus::Repaid);
    // July ended 28.10 with 1 500 ₽ unpaid.
    assert_eq!(c.states[1].status, TrancheStatus::Overdue);
    assert_eq!(c.repayment_tranche, Some(july));

    // Undo takes the last repayment back.
    v.undo().unwrap();
    assert_eq!(
        card_of(&v, card, "2026-08-06").states[0].remaining,
        rub(2000)
    );
}

#[test]
fn a_new_tranche_starts_today_and_dates_are_checked() {
    let dir = TempDir::new().unwrap();
    let (mut v, card, _) = setup(&dir);
    v.apply_products(
        vec![
            ProductOp::CreditTerms {
                product: card,
                edit: CreditInput::GraceDays(Some(55)),
            },
            ProductOp::CreditTerms {
                product: card,
                edit: CreditInput::CreditLimit(Some(rub(150_000))),
            },
            // «Начался новый грейс».
            ProductOp::AddTranche {
                product: card,
                edits: vec![],
            },
        ],
        d("2026-10-27"),
    )
    .unwrap();
    let c = card_of(&v, card, "2026-10-27");
    assert_eq!(c.terms.credit_limit, Some(rub(150_000)));
    assert_eq!(
        (c.tranches[0].start, c.tranches[0].end),
        (d("2026-10-27"), d("2026-12-21"))
    );
    let id = c.tranches[0].id;

    let set = |edit| vec![ProductOp::EditTranche { id, edit }];
    assert!(matches!(
        v.apply_products(set(TrancheInput::End(d("2026-10-01"))), d("2026-10-27")),
        Err(StorageError::Product(ProductError::EndBeforeStart))
    ));
    assert!(matches!(
        v.apply_products(
            vec![ProductOp::AddTranche {
                product: card,
                edits: tranche("2026-11-01", "2026-10-01"),
            }],
            d("2026-10-27"),
        ),
        Err(StorageError::Product(ProductError::EndBeforeStart))
    ));
    // Typing only the start keeps the suggested length.
    v.apply_products(
        vec![ProductOp::AddTranche {
            product: card,
            edits: vec![TrancheInput::Start(d("2026-11-27"))],
        }],
        d("2026-10-27"),
    )
    .unwrap();
    let c = card_of(&v, card, "2026-10-27");
    assert_eq!(c.tranches[1].end, d("2027-01-21"));

    v.apply_products(set(TrancheInput::Adjustment(rub(99))), d("2026-10-27"))
        .unwrap();
    assert_eq!(card_of(&v, card, "2026-10-27").states[0].debt, rub(99));

    // A repayment must be above zero; a card without tranches cannot take one.
    assert!(matches!(
        v.apply_products(
            vec![ProductOp::AddRepayment {
                product: card,
                edits: vec![RepaymentInput::Amount(Kopecks::ZERO)],
            }],
            d("2026-10-27"),
        ),
        Err(StorageError::Product(ProductError::AmountNotPositive))
    ));
}

#[test]
fn binding_by_hand_and_deleting_tranches() {
    let dir = TempDir::new().unwrap();
    let (mut v, card, _) = setup(&dir);
    v.apply_products(
        vec![
            ProductOp::AddTranche {
                product: card,
                edits: tranche("2026-06-01", "2026-07-25"),
            },
            ProductOp::AddTranche {
                product: card,
                edits: tranche("2026-07-01", "2026-08-25"),
            },
        ],
        d("2026-07-01"),
    )
    .unwrap();
    // Bought 30.06 but the bank put it into the July statement.
    let late = purchase(&mut v, "2026-06", Some("2026-06-30"), "Газпром КР", 1000);
    let c = card_of(&v, card, "2026-07-10");
    let (june, july) = (c.tranches[0].id, c.tranches[1].id);
    assert_eq!(c.states[0].debt, rub(1000));

    v.apply_products(
        vec![ProductOp::Bind {
            entry: late,
            tranche: Some(july),
        }],
        d("2026-07-10"),
    )
    .unwrap();
    let c = card_of(&v, card, "2026-07-10");
    assert_eq!(
        (c.states[0].debt, c.states[1].debt),
        (Kopecks::ZERO, rub(1000))
    );
    let p = c.purchases.iter().find(|p| p.entry.id == late).unwrap();
    assert!(p.manual);
    assert_eq!(p.tranche, Some(july));

    // A tranche with a purchase bound by hand, or with repayments, is not deleted.
    assert!(matches!(
        v.apply_products(
            vec![ProductOp::DeleteTranches { ids: vec![july] }],
            d("2026-07-10")
        ),
        Err(StorageError::InUse { count: 1 })
    ));
    // Back to the binding by date.
    v.apply_products(
        vec![ProductOp::Bind {
            entry: late,
            tranche: None,
        }],
        d("2026-07-10"),
    )
    .unwrap();
    assert_eq!(card_of(&v, card, "2026-07-10").states[0].debt, rub(1000));

    // Bound by hand again, then moved to another card: the binding goes with the old card.
    v.apply_products(
        vec![ProductOp::Bind {
            entry: late,
            tranche: Some(july),
        }],
        d("2026-07-10"),
    )
    .unwrap();
    v.apply(
        "2026-06".parse().unwrap(),
        vec![JournalOp::Edit {
            id: late,
            edit: EditInput::Card(Some("Райф А".into())),
        }],
    )
    .unwrap();
    let entry = v
        .entries("2026-06".parse().unwrap())
        .unwrap()
        .into_iter()
        .find(|e| e.id == late)
        .unwrap();
    assert_eq!(entry.grace_period_id, None);

    // Nothing refers to June now: it can go, and undo brings it back.
    v.apply_products(
        vec![ProductOp::DeleteTranches { ids: vec![june] }],
        d("2026-07-10"),
    )
    .unwrap();
    assert_eq!(card_of(&v, card, "2026-07-10").tranches.len(), 1);
    v.undo().unwrap();
    assert_eq!(card_of(&v, card, "2026-07-10").tranches.len(), 2);

    // A card with tranches stays in the dictionary.
    assert!(matches!(
        v.edit_dictionary(DictKind::Product, vec![DictOp::Delete { ids: vec![card] }]),
        Err(StorageError::InUse { .. })
    ));
}

#[test]
fn savings_and_the_deadlines_list() {
    let dir = TempDir::new().unwrap();
    let (mut v, card, savings) = setup(&dir);
    let today = d("2026-10-27");
    v.apply_products(
        vec![
            ProductOp::SavingsTerms {
                product: savings,
                edit: SavingsInput::PromoRate(Some(RateBp(1600))),
            },
            ProductOp::SavingsTerms {
                product: savings,
                edit: SavingsInput::ClosedAt(Some(d("2026-09-01"))),
            },
            ProductOp::SavingsTerms {
                product: savings,
                edit: SavingsInput::ReopenAfterDays(Some(60)),
            },
            ProductOp::AddTranche {
                product: card,
                edits: tranche("2026-09-01", "2026-10-30"),
            },
            ProductOp::AddDeadline {
                edits: vec![
                    DeadlineInput::Date(d("2026-12-15")),
                    DeadlineInput::Title("Перевыпустить карту".into()),
                    DeadlineInput::Card(Some("Райф А".into())),
                ],
            },
            ProductOp::AddDeadline {
                edits: vec![
                    DeadlineInput::Date(d("2026-10-01")),
                    DeadlineInput::Title("Сделано".into()),
                    DeadlineInput::Done(true),
                ],
            },
        ],
        today,
    )
    .unwrap();
    purchase(&mut v, "2026-09", Some("2026-09-05"), "Газпром КР", 2500);

    let data = v.products(today).unwrap();
    assert_eq!(data.savings.len(), 1);
    assert_eq!(
        data.savings[0].state,
        SavingsState::Waiting {
            from: d("2026-10-31"),
            days_left: 4
        }
    );
    assert_eq!(data.reminders.len(), 2);
    let list: Vec<_> = data
        .deadlines
        .iter()
        .map(|x| (x.date, x.urgency, x.days_left))
        .collect();
    assert_eq!(
        list,
        [
            (d("2026-10-30"), Urgency::Soon, 3),
            (d("2026-10-31"), Urgency::Soon, 4),
            (d("2026-12-15"), Urgency::Later, 49),
        ]
    );
    assert!(matches!(
        data.deadlines[0].source,
        DeadlineSource::GraceEnd { remaining, .. } if remaining == rub(2500)
    ));
    // The reminder's card was created from its name (D17).
    let raif = v
        .dictionaries()
        .unwrap()
        .products
        .into_iter()
        .find(|p| p.short_name == "Райф А")
        .unwrap();
    assert_eq!(data.deadlines[2].product_id, Some(raif.id));

    // Done: off the list.
    let id = data
        .reminders
        .iter()
        .find(|r| r.title == "Перевыпустить карту")
        .unwrap()
        .id;
    v.apply_products(
        vec![ProductOp::EditDeadline {
            id,
            edit: DeadlineInput::Done(true),
        }],
        today,
    )
    .unwrap();
    assert_eq!(v.products(today).unwrap().deadlines.len(), 2);
    assert!(matches!(
        v.apply_products(
            vec![ProductOp::AddDeadline {
                edits: vec![DeadlineInput::Title(" ".into())]
            }],
            today
        ),
        Err(StorageError::Product(ProductError::EmptyTitle))
    ));
}
