//! Copies of the file when it opens (D32) and changing its password (D33).

use std::path::Path;

use reestr_core::date::parse_iso;
use reestr_core::{EditInput, JournalOp, Kopecks, Month};
use reestr_storage::{COPIES_KEPT, CopyOutcome, StorageError, Vault, copies_dir};
use tempfile::TempDir;

const PW: &str = "старый 'пароль'";

fn day(d: u8) -> time::Date {
    parse_iso(&format!("2026-11-{d:02}")).unwrap()
}

fn july() -> Month {
    "2026-07".parse().unwrap()
}

fn add_row(vault: &mut Vault, title: &str) {
    vault
        .apply(
            july(),
            vec![JournalOp::Append {
                edits: vec![
                    EditInput::Title(title.into()),
                    EditInput::Amount(Kopecks::from_rubles(100)),
                ],
            }],
        )
        .unwrap();
}

fn titles(path: &Path, password: &str) -> Vec<String> {
    let vault = Vault::open(path, password).unwrap();
    let titles = vault
        .entries(july())
        .unwrap()
        .into_iter()
        .map(|e| e.title)
        .collect();
    vault.close().unwrap();
    titles
}

#[test]
fn a_copy_a_day_and_the_last_ten_kept() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Семья.reestr");
    let mut v = Vault::create(&path, PW).unwrap();
    add_row(&mut v, "Пятёрочка");
    v.close().unwrap();

    // The first opening of the day copies the file as it was, the second one does not.
    let (mut v, outcome) = Vault::open_with_copy(&path, PW, day(1)).unwrap();
    let CopyOutcome::Made(first) = outcome else {
        panic!("{outcome:?}")
    };
    assert_eq!(
        first,
        dir.path()
            .join("Семья — копии")
            .join("Семья 2026-11-01.reestr")
    );
    add_row(&mut v, "Лента");
    v.close().unwrap();
    let (v, outcome) = Vault::open_with_copy(&path, PW, day(1)).unwrap();
    assert!(matches!(outcome, CopyOutcome::Skipped), "{outcome:?}");
    v.close().unwrap();

    // The copy is the encrypted file of that moment, with the same password.
    assert_eq!(titles(&first, PW), ["Пятёрочка"]);
    assert!(matches!(
        Vault::open(&first, "чужой"),
        Err(StorageError::WrongPassword)
    ));

    for d in 2..=12 {
        let (v, outcome) = Vault::open_with_copy(&path, PW, day(d)).unwrap();
        assert!(matches!(outcome, CopyOutcome::Made(_)), "{outcome:?}");
        v.close().unwrap();
    }
    let v = Vault::open(&path, PW).unwrap();
    let copies = v.copies().unwrap();
    v.close().unwrap();
    assert_eq!(copies.len(), COPIES_KEPT);
    assert_eq!(copies[0].date, day(3));
    assert_eq!(copies[COPIES_KEPT - 1].date, day(12));
    assert!(copies.iter().all(|c| c.size > 0));
    // Nothing half-written is left behind.
    let names: Vec<String> = std::fs::read_dir(copies_dir(&path).unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), COPIES_KEPT, "{names:?}");

    // A copy opened to look at it makes no copies of its own.
    let (v, outcome) = Vault::open_with_copy(&copies[0].path, PW, day(13)).unwrap();
    assert!(matches!(outcome, CopyOutcome::Skipped), "{outcome:?}");
    assert!(v.copies().unwrap().is_empty());
    v.close().unwrap();
    assert_eq!(copies_dir(&copies[0].path), None);
}

#[test]
fn a_failed_copy_does_not_stop_the_file_from_opening() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Семья.reestr");
    Vault::create(&path, PW).unwrap().close().unwrap();
    // A plain file where the copies folder should be.
    std::fs::write(dir.path().join("Семья — копии"), b"").unwrap();

    let (v, outcome) = Vault::open_with_copy(&path, PW, day(1)).unwrap();
    assert!(matches!(outcome, CopyOutcome::Failed(_)), "{outcome:?}");
    v.close().unwrap();
}

#[test]
fn the_password_changes_after_the_current_one_is_checked() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Семья.reestr");
    let mut v = Vault::create(&path, PW).unwrap();
    add_row(&mut v, "Пятёрочка");
    v.close().unwrap();
    let (mut v, _) = Vault::open_with_copy(&path, PW, day(1)).unwrap();

    assert!(matches!(
        v.change_password("не тот", "новый"),
        Err(StorageError::WrongPassword)
    ));
    assert!(matches!(
        v.change_password(PW, ""),
        Err(StorageError::EmptyPassword)
    ));
    // The file stays usable in this session after a refused change and after the change.
    add_row(&mut v, "Лента");
    v.change_password(PW, "новый пароль").unwrap();
    add_row(&mut v, "Рынок");
    v.close().unwrap();

    assert!(matches!(
        Vault::open(&path, PW),
        Err(StorageError::WrongPassword)
    ));
    assert_eq!(
        titles(&path, "новый пароль"),
        ["Пятёрочка", "Лента", "Рынок"]
    );
    // The copy made before keeps the old password.
    let copy = copies_dir(&path).unwrap().join("Семья 2026-11-01.reestr");
    assert_eq!(titles(&copy, PW), ["Пятёрочка"]);
}
