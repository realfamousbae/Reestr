use reestr_core::{
    CashbackSource, DictInput, DictKind, DictOp, EditInput, Entry, JournalOp, Kopecks, Month,
    RateBp, Rounding,
};
use reestr_storage::{Created, StorageError, Vault};
use tempfile::TempDir;

const PW: &str = "test-password";

fn new_vault(dir: &TempDir) -> Vault {
    Vault::create(&dir.path().join("j.reestr"), PW).unwrap()
}

fn july() -> Month {
    "2026-07".parse().unwrap()
}

fn rub(r: i64) -> Kopecks {
    Kopecks::from_rubles(r)
}

fn append(vault: &mut Vault, edits: Vec<EditInput>) -> i64 {
    vault
        .apply(july(), vec![JournalOp::Append { edits }])
        .unwrap()
        .appended[0]
}

fn edit(vault: &mut Vault, id: i64, edit: EditInput) {
    vault
        .apply(july(), vec![JournalOp::Edit { id, edit }])
        .unwrap();
}

fn entry(vault: &Vault, id: i64) -> Entry {
    vault
        .entries(july())
        .unwrap()
        .into_iter()
        .find(|e| e.id == id)
        .unwrap()
}

fn category_id(vault: &Vault, name: &str) -> i64 {
    vault
        .dictionaries()
        .unwrap()
        .categories
        .iter()
        .find(|c| c.name == name)
        .unwrap()
        .id
}

fn purchase(title: &str, category: &str, who: &str, card: &str, amount: i64) -> Vec<EditInput> {
    vec![
        EditInput::Title(title.into()),
        EditInput::Category(Some(category.into())),
        EditInput::Participant(Some(who.into())),
        EditInput::Card(Some(card.into())),
        EditInput::Amount(rub(amount)),
    ]
}

#[test]
fn new_files_start_with_the_workbook_categories() {
    let dir = TempDir::new().unwrap();
    let v = new_vault(&dir);
    let d = v.dictionaries().unwrap();
    let names: Vec<_> = d.categories.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names.len(), 16);
    assert_eq!(&names[..4], ["еда", "продукты", "обучение", "лечение"]);
    let groups: Vec<_> = d.groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(
        groups,
        ["Лечение/лекарства", "Красота/косметика", "Одежда/обувь"]
    );
    assert!(d.participants.is_empty() && d.products.is_empty());
}

#[test]
fn typed_names_create_dictionary_items_once() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    let before = v.dictionaries().unwrap().categories.len();
    let outcome = v
        .apply(
            july(),
            vec![
                JournalOp::Append {
                    edits: purchase("Цветы", "подарки", "Марина", "Газпром КР", 1000),
                },
                JournalOp::Append {
                    edits: purchase("Торт", "Подарки", "марина", "газпром кр", 500),
                },
                JournalOp::Append {
                    edits: purchase("Лента", "ПРОДУКТЫ", "Марина", "Газпром КР", 700),
                },
            ],
        )
        .unwrap();
    assert_eq!(outcome.appended.len(), 3);
    assert_eq!(
        outcome.created,
        [
            Created {
                kind: DictKind::Category,
                name: "подарки".into()
            },
            Created {
                kind: DictKind::Participant,
                name: "Марина".into()
            },
            Created {
                kind: DictKind::Product,
                name: "Газпром КР".into()
            },
        ]
    );
    let d = v.dictionaries().unwrap();
    assert_eq!(d.categories.len(), before + 1);
    assert_eq!(d.participants.len(), 1);
    assert_eq!(d.products.len(), 1);
    let entries = v.entries(july()).unwrap();
    assert_eq!(entries[0].category_id, entries[1].category_id);
    assert_eq!(entries[2].category_id, Some(category_id(&v, "продукты")));
    assert_eq!(v.months().unwrap(), [july()]);
}

#[test]
fn title_autofills_blanks_from_the_last_same_entry() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    let mut first = purchase("Пятёрочка", "продукты", "Марина", "Газпром КР", 1000);
    first.push(EditInput::IsRegular(true));
    append(&mut v, first);
    // Different case and «е» instead of «ё».
    let id = append(&mut v, vec![EditInput::Title("ПЯТЕРОЧКА".into())]);
    let e = entry(&v, id);
    let d = v.dictionaries().unwrap();
    let products = category_id(&v, "продукты");
    assert_eq!(e.category_id, Some(products));
    assert_eq!(e.participant_id, Some(d.participants[0].id));
    assert_eq!(e.product_id, Some(d.products[0].id));
    assert!(e.is_regular);

    // A filled field is not overwritten.
    let other = append(
        &mut v,
        vec![
            EditInput::Participant(Some("Андрей".into())),
            EditInput::Title("Пятёрочка".into()),
        ],
    );
    let other = entry(&v, other);
    assert_ne!(other.participant_id, Some(d.participants[0].id));
    assert_eq!(other.category_id, Some(products));
}

#[test]
fn category_regular_default_marks_the_entry() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    let flats = category_id(&v, "квартиры");
    v.edit_dictionary(
        DictKind::Category,
        vec![DictOp::Edit {
            id: flats,
            edit: DictInput::IsRegularDefault(true),
        }],
    )
    .unwrap();
    let id = append(&mut v, vec![EditInput::Category(Some("Квартиры".into()))]);
    assert!(entry(&v, id).is_regular);
}

#[test]
fn cashback_uses_the_rounding_of_the_card() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    let id = append(
        &mut v,
        vec![
            EditInput::Card(Some("Райф А".into())),
            EditInput::Amount(Kopecks(123_450)),
            EditInput::Rate(Some(RateBp(350))),
        ],
    );
    // Default: down to whole rubles, like the workbook.
    assert_eq!(entry(&v, id).cashback_amount(), Kopecks(4300));

    let card = v.dictionaries().unwrap().products[0].id;
    v.edit_dictionary(
        DictKind::Product,
        vec![DictOp::Edit {
            id: card,
            edit: DictInput::Rounding(Rounding::HalfUpKop),
        }],
    )
    .unwrap();
    // An unrelated edit keeps the stored cashback.
    edit(&mut v, id, EditInput::Title("Кафе".into()));
    assert_eq!(entry(&v, id).cashback_amount(), Kopecks(4300));
    // Touching the amount recomputes it with the card's current rounding.
    edit(&mut v, id, EditInput::Amount(Kopecks(123_460)));
    assert_eq!(entry(&v, id).cashback_amount(), Kopecks(4321));

    edit(&mut v, id, EditInput::Cashback(Some(rub(200))));
    let e = entry(&v, id);
    assert_eq!(e.cashback.unwrap().source, CashbackSource::ManualAmount);
    assert_eq!(e.cashback_amount(), rub(200));
}

#[test]
fn undo_and_redo_a_cell_edit() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    let id = append(&mut v, vec![EditInput::Amount(rub(100))]);
    edit(&mut v, id, EditInput::Amount(rub(250)));
    assert_eq!(entry(&v, id).amount, rub(250));

    let undone = v.undo().unwrap().unwrap();
    assert_eq!(undone.label, "edit");
    assert_eq!(undone.months, ["2026-07"]);
    assert_eq!(entry(&v, id).amount, rub(100));
    assert!(v.can_redo());
    assert_eq!(v.redo().unwrap().unwrap().label, "edit");
    assert_eq!(entry(&v, id).amount, rub(250));

    // Undo everything: the appended row disappears too.
    v.undo().unwrap();
    v.undo().unwrap();
    assert!(v.entries(july()).unwrap().is_empty());
    assert_eq!(v.undo().unwrap(), None);
}

#[test]
fn undo_removes_auto_created_items_with_the_edit() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    let before = v.dictionaries().unwrap().categories;
    let id = append(&mut v, vec![EditInput::Title("Кино".into())]);
    edit(&mut v, id, EditInput::Category(Some("кино".into())));
    assert_eq!(v.dictionaries().unwrap().categories.len(), before.len() + 1);
    v.undo().unwrap();
    assert_eq!(entry(&v, id).category_id, None);
    assert_eq!(v.dictionaries().unwrap().categories, before);
    v.redo().unwrap();
    assert_eq!(entry(&v, id).category_id, Some(category_id(&v, "кино")));
}

#[test]
fn undo_restores_deleted_rows_in_place() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    let a = append(&mut v, vec![EditInput::Title("a".into())]);
    let b = append(&mut v, vec![EditInput::Title("b".into())]);
    let c = append(&mut v, vec![EditInput::Title("c".into())]);
    assert_eq!(v.delete_entries(&[b, 999]).unwrap(), 1);
    assert_eq!(v.entries(july()).unwrap().len(), 2);
    v.undo().unwrap();
    let ids: Vec<_> = v.entries(july()).unwrap().iter().map(|e| e.id).collect();
    assert_eq!(ids, [a, b, c]);
}

#[test]
fn a_new_action_clears_redo_and_no_op_edits_leave_no_trace() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    let id = append(&mut v, vec![EditInput::Amount(rub(1))]);
    edit(&mut v, id, EditInput::Amount(rub(2)));
    v.undo().unwrap();
    edit(&mut v, id, EditInput::Amount(rub(3)));
    assert!(!v.can_redo());

    // Same value again: nothing to undo for it.
    edit(&mut v, id, EditInput::Amount(rub(3)));
    v.undo().unwrap();
    assert_eq!(entry(&v, id).amount, rub(1));
}

#[test]
fn dictionary_rules() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    v.edit_dictionary(
        DictKind::Category,
        vec![
            DictOp::Append {
                edits: vec![
                    DictInput::Name("стоматология".into()),
                    DictInput::Group(Some("Здоровье".into())),
                ],
            },
            DictOp::Append {
                edits: vec![
                    DictInput::Name("анализы".into()),
                    DictInput::Group(Some("здоровье".into())),
                ],
            },
        ],
    )
    .unwrap();
    let d = v.dictionaries().unwrap();
    let health = d.groups.iter().find(|g| g.name == "Здоровье").unwrap().id;
    let mine: Vec<_> = d
        .categories
        .iter()
        .filter(|c| c.group_id == Some(health))
        .map(|c| c.id)
        .collect();
    assert_eq!(mine.len(), 2);

    // Duplicate names are refused, also by case.
    let err = v
        .edit_dictionary(
            DictKind::Category,
            vec![DictOp::Edit {
                id: mine[1],
                edit: DictInput::Name("СТОМАТОЛОГИЯ".into()),
            }],
        )
        .unwrap_err();
    assert!(matches!(err, StorageError::DuplicateName(_)), "{err:?}");

    // A field of another dictionary is refused.
    let err = v
        .edit_dictionary(
            DictKind::Category,
            vec![DictOp::Edit {
                id: mine[0],
                edit: DictInput::Rounding(Rounding::FloorKop),
            }],
        )
        .unwrap_err();
    assert!(matches!(err, StorageError::InvalidField { .. }), "{err:?}");

    // A category in use cannot be deleted.
    append(
        &mut v,
        vec![EditInput::Category(Some("стоматология".into()))],
    );
    let err = v
        .edit_dictionary(
            DictKind::Category,
            vec![DictOp::Delete { ids: vec![mine[0]] }],
        )
        .unwrap_err();
    assert!(matches!(err, StorageError::InUse { count: 1 }), "{err:?}");

    // Deleting a group ungroups its categories; undo regroups them.
    v.edit_dictionary(DictKind::Group, vec![DictOp::Delete { ids: vec![health] }])
        .unwrap();
    let after = v.dictionaries().unwrap();
    assert!(after.groups.iter().all(|g| g.id != health));
    assert!(after.categories.iter().all(|c| c.group_id != Some(health)));
    v.undo().unwrap();
    let restored = v.dictionaries().unwrap();
    assert_eq!(restored.groups, d.groups);
    assert_eq!(restored.categories, d.categories);
}

#[test]
fn templates_add_regular_entries() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    v.edit_dictionary(
        DictKind::Template,
        vec![DictOp::Append {
            edits: vec![
                DictInput::Name("Интернет".into()),
                DictInput::Category(Some("квартиры".into())),
                DictInput::Card(Some("Райф А".into())),
                DictInput::DefaultAmount(Some(rub(650))),
            ],
        }],
    )
    .unwrap();
    // A new file already has «Квартплата» (D37); this one is added after it.
    let t = v
        .dictionaries()
        .unwrap()
        .templates
        .into_iter()
        .find(|t| t.title == "Интернет")
        .unwrap();
    let ids = v.add_from_templates(july(), &[t.id]).unwrap();
    let e = entry(&v, ids[0]);
    assert_eq!(e.title, "Интернет");
    assert!(e.is_regular);
    assert_eq!(e.amount, rub(650));
    assert_eq!(e.category_id, t.category_id);
    assert_eq!(e.product_id, t.product_id);
}

#[test]
fn title_history_is_distinct_and_recent_first() {
    let dir = TempDir::new().unwrap();
    let mut v = new_vault(&dir);
    for t in ["Лента", "Пятёрочка", "лента", "Аптека"] {
        append(&mut v, vec![EditInput::Title(t.into())]);
    }
    assert_eq!(
        v.title_history(10).unwrap(),
        ["Аптека", "лента", "Пятёрочка"]
    );
    assert_eq!(v.title_history(1).unwrap(), ["Аптека"]);
}

#[test]
fn data_survives_reopening_but_undo_does_not() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("j.reestr");
    let mut v = Vault::create(&path, PW).unwrap();
    append(
        &mut v,
        purchase("Пятёрочка", "продукты", "Марина", "Газпром КР", 1000),
    );
    v.close().unwrap();

    let v = Vault::open(&path, PW).unwrap();
    assert_eq!(v.entries(july()).unwrap().len(), 1);
    assert_eq!(
        v.dictionaries().unwrap().products[0].short_name,
        "Газпром КР"
    );
    assert!(!v.can_undo());
}
