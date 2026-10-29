//! Exchange with the phone (D39, D40): the computer writes a copy for the phone, the phone adds
//! rows and writes its file, «Забрать с телефона» takes them, a new copy keeps what the
//! computer has not taken yet.

use std::path::PathBuf;

use reestr_core::date::parse_iso;
use reestr_core::{
    CashbackSource, DictKind, DictOp, EditInput, Entry, JournalOp, Kopecks, Month, RateBp,
    RuleInput, RuleOp,
};
use reestr_storage::{Role, SnapshotStatus, StorageError, TakeReport, Vault};
use tempfile::TempDir;

const PW: &str = "семейный 'пароль'";

struct Setup {
    _dir: TempDir,
    main: Vault,
    phone: Option<Vault>,
    for_phone: PathBuf,
    from_phone: PathBuf,
    working: PathBuf,
}

fn november() -> Month {
    "2026-11".parse().unwrap()
}

fn rub(r: i64) -> Kopecks {
    Kopecks::from_rubles(r)
}

/// A main file with a 5% rule on «Газпром КР» for groceries, its copy for the phone, and the
/// phone working on that copy.
fn setup() -> Setup {
    let dir = TempDir::new().unwrap();
    let mut main = Vault::create(&dir.path().join("Семья.reestr"), PW).unwrap();
    main.apply_rules(
        november(),
        vec![RuleOp::Append {
            edits: vec![
                RuleInput::Card(Some("Газпром КР".into())),
                RuleInput::Category(Some("продукты".into())),
                RuleInput::Rate(RateBp(500)),
            ],
        }],
        true,
    )
    .unwrap();
    let disk = dir.path().join("Яндекс Диск");
    std::fs::create_dir(&disk).unwrap();
    let paths = reestr_storage::exchange_paths(main.path(), &disk).unwrap();
    main.write_for_phone(&paths.for_phone).unwrap();
    let working = dir.path().join("телефон.reestr");
    let (phone, start) = Vault::start_on_phone(&paths.for_phone, &working, PW, None).unwrap();
    assert_eq!(start.carried, 0);
    assert_eq!(phone.role(), Role::Phone);
    Setup {
        _dir: dir,
        main,
        phone: Some(phone),
        for_phone: paths.for_phone,
        from_phone: paths.from_phone,
        working,
    }
}

fn purchase(title: &str, amount: i64) -> Vec<EditInput> {
    vec![
        EditInput::Title(title.into()),
        EditInput::Category(Some("продукты".into())),
        EditInput::Card(Some("Газпром КР".into())),
        EditInput::Date(parse_iso("2026-11-03")),
        EditInput::Amount(rub(amount)),
    ]
}

fn append(vault: &mut Vault, edits: Vec<EditInput>) -> i64 {
    vault
        .apply(november(), vec![JournalOp::Append { edits }])
        .unwrap()
        .appended[0]
}

fn edit(vault: &mut Vault, id: i64, edit: EditInput) -> Result<(), StorageError> {
    vault
        .apply(november(), vec![JournalOp::Edit { id, edit }])
        .map(|_| ())
}

fn rows(vault: &Vault) -> Vec<Entry> {
    vault.entries(november()).unwrap()
}

fn find(vault: &Vault, title: &str) -> Entry {
    rows(vault)
        .into_iter()
        .find(|e| e.title == title)
        .unwrap_or_else(|| panic!("{title}"))
}

fn titles(vault: &Vault) -> Vec<String> {
    rows(vault).into_iter().map(|e| e.title).collect()
}

fn counts(r: &TakeReport) -> (usize, usize, usize, usize) {
    (r.added, r.changed, r.deleted, r.kept)
}

impl Setup {
    fn phone(&mut self) -> &mut Vault {
        self.phone.as_mut().unwrap()
    }

    fn send(&mut self) {
        let target = self.from_phone.clone();
        self.phone().write_for_computer(&target).unwrap();
    }

    fn close_phone(&mut self) {
        self.phone.take().unwrap().close().unwrap();
    }

    fn take(&mut self) -> TakeReport {
        self.send();
        self.main.take_from_phone(&self.from_phone, None).unwrap()
    }

    /// The computer writes a new copy and the phone starts on it.
    fn refresh(&mut self) -> reestr_storage::PhoneStart {
        self.main.write_for_phone(&self.for_phone).unwrap();
        self.close_phone();
        let (phone, start) =
            Vault::start_on_phone(&self.for_phone, &self.working, PW, None).unwrap();
        self.phone = Some(phone);
        start
    }
}

#[test]
fn a_phone_row_reaches_the_computer_once_with_its_cashback() {
    let mut s = setup();
    append(s.phone(), purchase("Пятёрочка", 1200));
    s.send();
    let preview = s.main.phone_preview(&s.from_phone, None).unwrap();
    assert_eq!(counts(&preview), (1, 0, 0, 0));

    let report = s.take();
    assert_eq!(counts(&report), (1, 0, 0, 0));
    assert_eq!(report.months, [november()]);
    let row = find(&s.main, "Пятёрочка");
    assert_eq!(row.amount, rub(1200));
    assert_eq!(row.date, parse_iso("2026-11-03"));
    // The computer's rule: 5% of 1 200.
    let cashback = row.cashback.unwrap();
    assert_eq!(
        (cashback.amount, cashback.source),
        (rub(60), CashbackSource::Rule)
    );

    // Taking the same file again adds nothing.
    assert!(s.take().is_empty());
    assert_eq!(titles(&s.main), ["Пятёрочка"]);

    // «Забрать» is undone as one action and can be repeated.
    s.main.undo().unwrap();
    assert!(titles(&s.main).is_empty());
    assert_eq!(counts(&s.take()), (1, 0, 0, 0));
}

#[test]
fn a_row_deleted_on_the_computer_does_not_come_back() {
    let mut s = setup();
    let id = append(s.phone(), purchase("Пятёрочка", 1200));
    s.take();
    let on_main = find(&s.main, "Пятёрочка").id;
    s.main.delete_entries(&[on_main]).unwrap();
    assert!(s.take().is_empty());

    // The phone does not know yet and changes its row: the computer's deletion stays.
    edit(s.phone(), id, EditInput::Amount(rub(1300))).unwrap();
    assert_eq!(counts(&s.take()), (0, 0, 0, 1));
    assert!(titles(&s.main).is_empty());
}

#[test]
fn phone_edits_and_deletions_reach_untouched_rows() {
    let mut s = setup();
    let id = append(s.phone(), purchase("Пятёрочка", 1200));
    s.take();

    edit(s.phone(), id, EditInput::Amount(rub(1500))).unwrap();
    assert_eq!(counts(&s.take()), (0, 1, 0, 0));
    let row = find(&s.main, "Пятёрочка");
    assert_eq!(row.amount, rub(1500));
    assert_eq!(row.cashback.unwrap().amount, rub(75));

    s.phone().delete_entries(&[id]).unwrap();
    assert_eq!(counts(&s.take()), (0, 0, 1, 0));
    assert!(titles(&s.main).is_empty());
}

#[test]
fn the_computer_wins_when_both_changed() {
    let mut s = setup();
    let id = append(s.phone(), purchase("Пятёрочка", 1200));
    s.take();
    let on_main = find(&s.main, "Пятёрочка").id;
    edit(
        &mut s.main,
        on_main,
        EditInput::Category(Some("еда".into())),
    )
    .unwrap();

    edit(s.phone(), id, EditInput::Amount(rub(1500))).unwrap();
    assert_eq!(counts(&s.take()), (0, 0, 0, 1));
    assert_eq!(find(&s.main, "Пятёрочка").amount, rub(1200));
    // Later phone changes of that row lose as well.
    edit(s.phone(), id, EditInput::Amount(rub(1600))).unwrap();
    assert_eq!(counts(&s.take()), (0, 0, 0, 1));
}

#[test]
fn an_undo_on_the_phone_is_a_change_too() {
    let mut s = setup();
    let id = append(s.phone(), purchase("Пятёрочка", 1200));
    s.take();
    edit(s.phone(), id, EditInput::Amount(rub(1500))).unwrap();
    s.take();
    s.phone().undo().unwrap();
    assert_eq!(counts(&s.take()), (0, 1, 0, 0));
    assert_eq!(find(&s.main, "Пятёрочка").amount, rub(1200));

    // Undoing a new row deletes it on the computer too.
    append(s.phone(), purchase("Лента", 800));
    s.take();
    s.phone().undo().unwrap();
    assert_eq!(counts(&s.take()), (0, 0, 1, 0));
    assert_eq!(titles(&s.main), ["Пятёрочка"]);
}

#[test]
fn a_new_copy_keeps_what_the_computer_has_not_taken() {
    let mut s = setup();
    append(s.phone(), purchase("Пятёрочка", 1200));
    s.take();
    append(s.phone(), purchase("Лента", 800));
    let gone = append(s.phone(), purchase("Рынок", 300));
    s.phone().delete_entries(&[gone]).unwrap();
    // The computer changes something the phone gets with the copy.
    s.main
        .edit_dictionary(
            DictKind::Participant,
            vec![DictOp::Append {
                edits: vec![reestr_core::DictInput::Name("Марина".into())],
            }],
        )
        .unwrap();

    let start = s.refresh();
    // «Лента» and the deletion of «Рынок» wait for the computer.
    assert_eq!((start.carried, start.kept), (2, 0));
    assert_eq!(titles(s.phone()), ["Пятёрочка", "Лента"]);
    assert!(
        s.phone()
            .dictionaries()
            .unwrap()
            .participants
            .iter()
            .any(|p| p.name == "Марина")
    );
    // A row the computer has taken is the computer's now.
    let on_phone = find(s.phone(), "Пятёрочка").id;
    assert_eq!(rows(s.phone()).len(), 2);
    assert!(matches!(
        edit(s.phone(), on_phone, EditInput::Amount(rub(1))),
        Err(StorageError::NotPhoneRow)
    ));
    // The row still waiting can be changed.
    let lenta = find(s.phone(), "Лента").id;
    edit(s.phone(), lenta, EditInput::Amount(rub(900))).unwrap();

    assert_eq!(counts(&s.take()), (1, 0, 0, 0));
    assert_eq!(titles(&s.main), ["Пятёрочка", "Лента"]);
    assert_eq!(find(&s.main, "Лента").amount, rub(900));
    // Everything is taken: the next copy carries nothing.
    assert_eq!(s.refresh().carried, 0);
}

#[test]
fn a_new_copy_drops_rows_the_computer_changed_meanwhile() {
    let mut s = setup();
    let id = append(s.phone(), purchase("Пятёрочка", 1200));
    s.take();
    let on_main = find(&s.main, "Пятёрочка").id;
    s.main.delete_entries(&[on_main]).unwrap();
    edit(s.phone(), id, EditInput::Amount(rub(1500))).unwrap();

    let start = s.refresh();
    assert_eq!((start.carried, start.kept), (0, 1));
    assert!(titles(s.phone()).is_empty());
}

#[test]
fn the_phone_changes_its_own_rows_only() {
    let mut s = setup();
    let on_main = append(&mut s.main, purchase("Квартплата", 7840));
    assert_eq!(s.refresh().carried, 0);
    let row = find(s.phone(), "Квартплата").id;
    assert_eq!(row, on_main);
    assert!(matches!(
        edit(s.phone(), row, EditInput::Amount(rub(1))),
        Err(StorageError::NotPhoneRow)
    ));
    assert!(matches!(
        s.phone().delete_entries(&[row]),
        Err(StorageError::NotPhoneRow)
    ));
    assert!(matches!(
        s.phone().edit_dictionary(
            DictKind::Participant,
            vec![DictOp::Append {
                edits: vec![reestr_core::DictInput::Name("Марина".into())],
            }],
        ),
        Err(StorageError::ComputerOnly)
    ));
    // A new name typed on the phone travels as a name and is created on the computer (D17).
    let mut edits = purchase("Аптека", 450);
    edits[1] = EditInput::Category(Some("аптека новая".into()));
    append(s.phone(), edits);
    let report = s.take();
    assert_eq!(report.created.len(), 1);
    let category = find(&s.main, "Аптека").category_id.unwrap();
    assert!(
        s.main
            .dictionaries()
            .unwrap()
            .categories
            .iter()
            .any(|c| c.id == category && c.name == "аптека новая")
    );
}

#[test]
fn files_are_checked() {
    let mut s = setup();
    append(s.phone(), purchase("Пятёрочка", 1200));
    s.send();
    let dir = s.working.parent().unwrap().to_owned();
    let today = parse_iso("2026-11-04").unwrap();

    // The computer opens neither the phone's file nor the copy for the phone.
    let copy = dir.join("копия для открытия.reestr");
    std::fs::copy(&s.from_phone, &copy).unwrap();
    assert!(matches!(
        Vault::open_with_copy(&copy, PW, today),
        Err(StorageError::PhoneFile)
    ));
    assert!(matches!(
        Vault::open_with_copy(&s.for_phone, PW, today),
        Err(StorageError::SnapshotFile)
    ));
    assert!(matches!(
        s.main.take_from_phone(&s.for_phone, None),
        Err(StorageError::NotFromPhone)
    ));
    assert!(matches!(
        s.main.take_from_phone(&dir.join("нет.reestr"), None),
        Err(StorageError::NotFound)
    ));

    // Another family file with the same password: files of one are refused by the other.
    let mut other = Vault::create(&dir.join("Другой.reestr"), PW).unwrap();
    assert!(matches!(
        other.take_from_phone(&s.from_phone, Some(PW)),
        Err(StorageError::OtherFile)
    ));
    let other_copy = dir.join("Другой — для телефона.reestr");
    other.write_for_phone(&other_copy).unwrap();
    other.close().unwrap();

    // The phone does not start on its own file or on an older copy.
    assert!(matches!(
        Vault::start_on_phone(&s.from_phone, &dir.join("ещё.reestr"), PW, None),
        Err(StorageError::NotForPhone)
    ));
    let old_copy = dir.join("старая копия.reestr");
    std::fs::copy(&s.for_phone, &old_copy).unwrap();
    assert_eq!(
        s.phone
            .as_ref()
            .unwrap()
            .snapshot_status(&s.for_phone, None)
            .unwrap(),
        SnapshotStatus::Same
    );
    s.refresh();
    s.main.write_for_phone(&s.for_phone).unwrap();
    assert_eq!(
        s.phone
            .as_ref()
            .unwrap()
            .snapshot_status(&s.for_phone, None)
            .unwrap(),
        SnapshotStatus::Newer
    );
    assert_eq!(
        s.phone
            .as_ref()
            .unwrap()
            .snapshot_status(&old_copy, None)
            .unwrap(),
        SnapshotStatus::Older
    );
    s.close_phone();
    assert!(matches!(
        Vault::start_on_phone(&old_copy, &s.working, PW, None),
        Err(StorageError::OlderCopy)
    ));
    assert!(matches!(
        Vault::start_on_phone(&other_copy, &s.working, PW, Some(PW)),
        Err(StorageError::OtherFile)
    ));
    // A refused copy leaves the phone's file as it was.
    let phone = Vault::open(&s.working, PW).unwrap();
    assert_eq!(titles(&phone), ["Пятёрочка"]);
}

#[test]
fn a_changed_password_is_asked_for() {
    let mut s = setup();
    append(s.phone(), purchase("Пятёрочка", 1200));
    s.send();
    s.main.change_password(PW, "новый пароль").unwrap();
    assert!(matches!(
        s.main.take_from_phone(&s.from_phone, None),
        Err(StorageError::WrongPassword)
    ));
    assert!(matches!(
        s.main.take_from_phone(&s.from_phone, Some("не тот")),
        Err(StorageError::WrongPassword)
    ));
    let report = s.main.take_from_phone(&s.from_phone, Some(PW)).unwrap();
    assert_eq!(report.added, 1);

    // The phone gets the copy with the new password and moves its rows over with the old one.
    append(s.phone(), purchase("Лента", 800));
    s.main.write_for_phone(&s.for_phone).unwrap();
    s.close_phone();
    assert!(matches!(
        Vault::start_on_phone(&s.for_phone, &s.working, "новый пароль", None),
        Err(StorageError::WrongPassword)
    ));
    let (phone, start) =
        Vault::start_on_phone(&s.for_phone, &s.working, "новый пароль", Some(PW)).unwrap();
    assert_eq!(start.carried, 1);
    assert_eq!(titles(&phone), ["Пятёрочка", "Лента"]);
    phone.close().unwrap();
    assert!(Vault::open(&s.working, "новый пароль").is_ok());
}

#[test]
fn an_empty_phone_file_waits_for_the_phone() {
    let mut s = setup();
    std::fs::remove_file(&s.from_phone).ok();
    s.main.write_empty_phone_file(&s.from_phone).unwrap();
    assert!(
        s.main
            .take_from_phone(&s.from_phone, None)
            .unwrap()
            .is_empty()
    );

    // The phone's rows: its own new row only, until the computer takes it.
    let mine = append(s.phone(), purchase("Пятёрочка", 1200));
    assert_eq!(s.phone().phone_rows().unwrap(), [mine]);
    s.take();
    s.refresh();
    assert!(s.phone().phone_rows().unwrap().is_empty());
}
