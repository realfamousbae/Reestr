//! Schema migrations, tracked by `PRAGMA user_version`.
//!
//! Each entry upgrades the schema by one version. Never edit a released migration — append
//! a new one.

use rusqlite::{Connection, params};

use crate::error::StorageError;

pub(crate) const MIGRATIONS: &[&str] = &[
    // 1: file metadata (PLAN §4.3).
    "CREATE TABLE meta (
        key   TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL
    ) STRICT;",
    // 2: dictionaries, the journal and the change log (stage 1, D18). Money is INTEGER
    // kopecks, dates are ISO TEXT. `name_key` columns hold `reestr_core::name_key` of the name:
    // case-insensitive uniqueness that works for Cyrillic (SQLite NOCASE is ASCII-only).
    "CREATE TABLE participant (
        id         INTEGER PRIMARY KEY,
        name       TEXT    NOT NULL,
        name_key   TEXT    NOT NULL UNIQUE,
        sort_order INTEGER NOT NULL DEFAULT 0,
        is_active  INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1))
    ) STRICT;

    CREATE TABLE bank (
        id         INTEGER PRIMARY KEY,
        name       TEXT NOT NULL,
        name_key   TEXT NOT NULL UNIQUE,
        short_name TEXT
    ) STRICT;

    CREATE TABLE product (
        id                INTEGER PRIMARY KEY,
        bank_id           INTEGER REFERENCES bank (id),
        participant_id    INTEGER REFERENCES participant (id),
        kind              TEXT CHECK (kind IN ('debit', 'credit', 'savings', 'deposit', 'cash')),
        short_name        TEXT    NOT NULL,
        name_key          TEXT    NOT NULL UNIQUE,
        full_name         TEXT,
        cashback_rounding TEXT    NOT NULL DEFAULT 'floor_rub'
            CHECK (cashback_rounding IN ('floor_rub', 'floor_kop', 'half_up_rub', 'half_up_kop')),
        is_active         INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
        opened_at         TEXT,
        closed_at         TEXT,
        notes             TEXT
    ) STRICT;

    CREATE TABLE category_group (
        id         INTEGER PRIMARY KEY,
        name       TEXT    NOT NULL,
        name_key   TEXT    NOT NULL UNIQUE,
        sort_order INTEGER NOT NULL DEFAULT 0
    ) STRICT;

    CREATE TABLE category (
        id                 INTEGER PRIMARY KEY,
        name               TEXT    NOT NULL,
        name_key           TEXT    NOT NULL UNIQUE,
        group_id           INTEGER REFERENCES category_group (id),
        is_regular_default INTEGER NOT NULL DEFAULT 0 CHECK (is_regular_default IN (0, 1)),
        sort_order         INTEGER NOT NULL DEFAULT 0,
        is_active          INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1))
    ) STRICT;

    -- PLAN §5 `transaction`; renamed because TRANSACTION is an SQL keyword (D18).
    CREATE TABLE entry (
        id               INTEGER PRIMARY KEY,
        month            TEXT    NOT NULL,
        position         INTEGER NOT NULL,
        title            TEXT    NOT NULL DEFAULT '',
        title_key        TEXT    NOT NULL DEFAULT '',
        category_id      INTEGER REFERENCES category (id),
        is_regular       INTEGER NOT NULL DEFAULT 0 CHECK (is_regular IN (0, 1)),
        participant_id   INTEGER REFERENCES participant (id),
        date             TEXT,
        amount           INTEGER NOT NULL DEFAULT 0,
        cashback_amount  INTEGER,
        cashback_rate_bp INTEGER,
        cashback_source  TEXT
            CHECK (cashback_source IN ('rule', 'manual_percent', 'manual_amount')),
        product_id       INTEGER REFERENCES product (id),
        notes            TEXT,
        import_key       TEXT UNIQUE,
        created_at       TEXT    NOT NULL,
        updated_at       TEXT    NOT NULL
    ) STRICT;
    CREATE INDEX entry_month ON entry (month, position);
    CREATE INDEX entry_title ON entry (title_key);

    CREATE TABLE regular_template (
        id             INTEGER PRIMARY KEY,
        title          TEXT    NOT NULL,
        category_id    INTEGER REFERENCES category (id),
        participant_id INTEGER REFERENCES participant (id),
        product_id     INTEGER REFERENCES product (id),
        default_amount INTEGER,
        sort_order     INTEGER NOT NULL DEFAULT 0,
        is_active      INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1))
    ) STRICT;

    -- One row per user action; `change_log` rows of the action point to it (undo groups).
    CREATE TABLE change_op (
        id     INTEGER PRIMARY KEY,
        ts     TEXT    NOT NULL,
        label  TEXT    NOT NULL,
        undone INTEGER NOT NULL DEFAULT 0 CHECK (undone IN (0, 1))
    ) STRICT;

    -- `field` is a column name, or '*' for a whole inserted/deleted row. Values are JSON.
    CREATE TABLE change_log (
        id        INTEGER PRIMARY KEY,
        op_id     INTEGER NOT NULL REFERENCES change_op (id),
        ts        TEXT    NOT NULL,
        entity    TEXT    NOT NULL,
        entity_id INTEGER NOT NULL,
        field     TEXT    NOT NULL,
        old_value TEXT,
        new_value TEXT
    ) STRICT;
    CREATE INDEX change_log_op ON change_log (op_id);",
    // 3: cashback rules of a month (stage 2, D20, D21) and the «Ср. %» flag of a category (D19).
    "ALTER TABLE category ADD COLUMN exclude_from_rate INTEGER NOT NULL DEFAULT 0
        CHECK (exclude_from_rate IN (0, 1));
    UPDATE category SET exclude_from_rate = 1 WHERE name_key = 'квартиры';

    -- `category_id` NULL is «на всё». One rule per month, card and category.
    CREATE TABLE cashback_rule (
        id               INTEGER PRIMARY KEY,
        month            TEXT    NOT NULL,
        product_id       INTEGER NOT NULL REFERENCES product (id),
        category_id      INTEGER REFERENCES category (id),
        rate_bp          INTEGER NOT NULL DEFAULT 0 CHECK (rate_bp BETWEEN 0 AND 10000),
        spend_cap        INTEGER CHECK (spend_cap >= 0),
        cashback_cap     INTEGER CHECK (cashback_cap >= 0),
        spent_adjustment INTEGER NOT NULL DEFAULT 0,
        notes            TEXT
    ) STRICT;
    CREATE UNIQUE INDEX cashback_rule_key
        ON cashback_rule (month, product_id, IFNULL(category_id, 0));",
    // 4: credit card tranches, repayments, savings terms, reminders (stage 3a, D27, D28).
    // Tranche status and the deadlines of tranches and savings accounts are computed, not
    // stored; `entry.grace_period_id` holds only a binding chosen by hand.
    "CREATE TABLE credit_terms (
        id                 INTEGER PRIMARY KEY,
        product_id         INTEGER NOT NULL UNIQUE REFERENCES product (id),
        credit_limit       INTEGER CHECK (credit_limit >= 0),
        default_grace_days INTEGER CHECK (default_grace_days > 0)
    ) STRICT;

    CREATE TABLE grace_period (
        id              INTEGER PRIMARY KEY,
        product_id      INTEGER NOT NULL REFERENCES product (id),
        start_date      TEXT    NOT NULL,
        end_date        TEXT    NOT NULL,
        debt_adjustment INTEGER NOT NULL DEFAULT 0,
        notes           TEXT,
        CHECK (end_date >= start_date)
    ) STRICT;
    CREATE INDEX grace_period_product ON grace_period (product_id, start_date);

    CREATE TABLE repayment (
        id              INTEGER PRIMARY KEY,
        grace_period_id INTEGER NOT NULL REFERENCES grace_period (id),
        date            TEXT    NOT NULL,
        amount          INTEGER NOT NULL CHECK (amount > 0),
        notes           TEXT
    ) STRICT;

    CREATE TABLE savings_terms (
        id                INTEGER PRIMARY KEY,
        product_id        INTEGER NOT NULL UNIQUE REFERENCES product (id),
        promo_rate_bp     INTEGER CHECK (promo_rate_bp BETWEEN 0 AND 10000),
        promo_closed_at   TEXT,
        reopen_after_days INTEGER CHECK (reopen_after_days >= 0)
    ) STRICT;

    CREATE TABLE deadline (
        id         INTEGER PRIMARY KEY,
        product_id INTEGER REFERENCES product (id),
        date       TEXT    NOT NULL,
        title      TEXT    NOT NULL,
        is_done    INTEGER NOT NULL DEFAULT 0 CHECK (is_done IN (0, 1))
    ) STRICT;

    ALTER TABLE entry ADD COLUMN grace_period_id INTEGER REFERENCES grace_period (id);",
    // 5: inflows and the income plan («Оклад» of «Свод»; stage 3b). A plan carries over to the
    // following months until changed, so only months where it was set have a row.
    // `participant_id` NULL is the family plan, the only one the app edits for now.
    "CREATE TABLE inflow (
        id             INTEGER PRIMARY KEY,
        month          TEXT    NOT NULL,
        date           TEXT,
        participant_id INTEGER REFERENCES participant (id),
        product_id     INTEGER REFERENCES product (id),
        source         TEXT    NOT NULL DEFAULT '',
        amount         INTEGER NOT NULL,
        notes          TEXT
    ) STRICT;
    CREATE INDEX inflow_month ON inflow (month);

    CREATE TABLE monthly_plan (
        id             INTEGER PRIMARY KEY,
        month          TEXT    NOT NULL,
        participant_id INTEGER REFERENCES participant (id),
        planned_income INTEGER NOT NULL CHECK (planned_income >= 0)
    ) STRICT;
    CREATE UNIQUE INDEX monthly_plan_key ON monthly_plan (month, IFNULL(participant_id, 0));",
    // 6: «Не в Ср. %» moves from the category to the regular payment (D37): rent stays out of
    // the base of «Ср. %», the internet in the same category does not. A file with entries
    // gets «Квартплата» marked, made in the rent category if it has none. SQLite's lower()
    // knows ASCII only, hence both spellings.
    "ALTER TABLE regular_template ADD COLUMN exclude_from_rate INTEGER NOT NULL DEFAULT 0
        CHECK (exclude_from_rate IN (0, 1));
    UPDATE regular_template SET exclude_from_rate = 1
        WHERE trim(title) IN ('Квартплата', 'квартплата', 'КВАРТПЛАТА');
    INSERT INTO regular_template (title, category_id, sort_order, is_active, exclude_from_rate)
        SELECT 'Квартплата',
               (SELECT id FROM category WHERE exclude_from_rate = 1 ORDER BY id LIMIT 1),
               (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM regular_template),
               1, 1
        WHERE EXISTS (SELECT 1 FROM entry)
          AND NOT EXISTS (SELECT 1 FROM regular_template WHERE exclude_from_rate = 1);
    ALTER TABLE category DROP COLUMN exclude_from_rate;",
    // 7: exchange with the phone (D39). `file_id` ties the copies for the phone and the
    // phone's file to their main file. On the phone, `phone_change` lists its rows the computer
    // may not have taken yet (`key` is `entry.import_key`); on the computer, `phone_taken`
    // remembers what it took of each key, also after the row is deleted there.
    "INSERT INTO meta (key, value)
        SELECT 'file_id', lower(hex(randomblob(16)))
        WHERE NOT EXISTS (SELECT 1 FROM meta WHERE key = 'file_id');

    CREATE TABLE phone_change (
        id       INTEGER PRIMARY KEY,
        key      TEXT    NOT NULL UNIQUE,
        entry_id INTEGER NOT NULL,
        rev      INTEGER NOT NULL CHECK (rev > 0),
        deleted  INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1))
    ) STRICT;

    CREATE TABLE phone_taken (
        id          INTEGER PRIMARY KEY,
        key         TEXT    NOT NULL UNIQUE,
        rev         INTEGER NOT NULL CHECK (rev > 0),
        fingerprint TEXT    NOT NULL
    ) STRICT;",
    // 8: the exchange with the phone is gone (D41). Copies for the phone and the phone's files
    // are refused before the migrations, so only main files get here. The log keeps its old
    // `phone` actions and `phone_taken` rows: undo reaches this session's actions only, and
    // none of those can exist yet.
    "DROP TABLE phone_change;
    DROP TABLE phone_taken;
    DELETE FROM meta WHERE key IN ('file_id', 'snapshot_seq', 'role');",
    // 9: the available balance the bank showed for a credit card, with the date it was valid
    // for (PLAN §7). The core keeps them set together; the columns are written one at a time.
    "ALTER TABLE credit_terms ADD COLUMN manual_available INTEGER;
    ALTER TABLE credit_terms ADD COLUMN manual_available_on TEXT;",
    // 10: MCC of an entry and of a rule (D20), four digits as text: leading zeros matter. A rule
    // has an MCC, a category or neither (the core checks both), and existing rules stay as they
    // are: no code. The key keeps one rule per month, card and scope.
    "ALTER TABLE entry ADD COLUMN mcc TEXT;
    ALTER TABLE cashback_rule ADD COLUMN mcc TEXT;
    DROP INDEX cashback_rule_key;
    CREATE UNIQUE INDEX cashback_rule_key
        ON cashback_rule (month, product_id, IFNULL(category_id, 0), IFNULL(mcc, ''));",
];

/// The schema version this build writes and reads.
pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

pub(crate) fn user_version(conn: &Connection) -> Result<u32, StorageError> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

/// Brings the schema up to [`SCHEMA_VERSION`] in one transaction.
pub(crate) fn migrate(conn: &mut Connection) -> Result<(), StorageError> {
    let current = user_version(conn)?;
    if current > SCHEMA_VERSION {
        return Err(StorageError::TooNew {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    if current == SCHEMA_VERSION {
        return Ok(());
    }
    let tx = conn.transaction()?;
    for sql in &MIGRATIONS[current as usize..] {
        tx.execute_batch(sql)?;
    }
    tx.execute(
        "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![SCHEMA_VERSION.to_string()],
    )?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rent stays out of «Ср. %» in older files: by the category up to version 5, by the regular
    /// payment «Квартплата» from version 6 on (D37).
    #[test]
    fn rent_stays_out_of_the_rate_in_older_files() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..2] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 2).unwrap();
        conn.execute_batch(
            "INSERT INTO category (name, name_key) VALUES ('Квартиры', 'квартиры'), ('еда', 'еда');
             INSERT INTO regular_template (title, category_id) VALUES ('Интернет', 1);
             INSERT INTO entry (month, position, title, title_key, amount, created_at, updated_at)
                 VALUES ('2026-07', 1, 'Квартплата', 'квартплата', 784000, '', '');",
        )
        .unwrap();
        migrate(&mut conn).unwrap();
        let templates: Vec<(String, Option<i64>, bool)> = conn
            .prepare(
                "SELECT title, category_id, exclude_from_rate FROM regular_template ORDER BY id",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            templates,
            [
                ("Интернет".to_owned(), Some(1), false),
                ("Квартплата".to_owned(), Some(1), true)
            ]
        );
        // The category no longer has the column.
        assert!(
            conn.prepare("SELECT exclude_from_rate FROM category")
                .is_err()
        );
        assert_eq!(user_version(&conn).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn version_6_marks_an_existing_rent_payment_and_makes_none_in_an_empty_file() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..5] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 5).unwrap();
        conn.execute(
            "INSERT INTO regular_template (title) VALUES (' квартплата '), ('Интернет')",
            [],
        )
        .unwrap();
        migrate(&mut conn).unwrap();
        let marked: Vec<String> = conn
            .prepare("SELECT title FROM regular_template WHERE exclude_from_rate = 1")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(marked, [" квартплата "]);

        // A file without entries gets no payment made up.
        let mut empty = Connection::open_in_memory().unwrap();
        migrate(&mut empty).unwrap();
        let count: i64 = empty
            .query_row("SELECT count(*) FROM regular_template", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    /// Version 8 removes the exchange and nothing else, also from a file that took rows from
    /// the phone (D41).
    #[test]
    fn version_8_removes_the_phone_exchange_and_keeps_the_journal() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..7] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 7).unwrap();
        conn.execute_batch(
            "INSERT INTO meta (key, value) VALUES ('snapshot_seq', '2'), ('card_suggestions', 'on');
             INSERT INTO entry (month, position, title, title_key, amount, import_key,
                                created_at, updated_at)
                 VALUES ('2026-07', 1, 'Обед', 'обед', 45000, 'phone-1', '', '');
             INSERT INTO phone_taken (key, rev, fingerprint) VALUES ('phone-1', 1, 'abc');
             INSERT INTO phone_change (key, entry_id, rev) VALUES ('phone-1', 1, 1);
             INSERT INTO change_op (ts, label) VALUES ('', 'phone');
             INSERT INTO change_log (op_id, ts, entity, entity_id, field, new_value)
                 VALUES (1, '', 'phone_taken', 1, '*', '{}'), (1, '', 'entry', 1, '*', '{}');",
        )
        .unwrap();
        migrate(&mut conn).unwrap();

        assert_eq!(user_version(&conn).unwrap(), SCHEMA_VERSION);
        let row: (String, i64, String) = conn
            .query_row("SELECT title, amount, import_key FROM entry", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(row, ("Обед".to_owned(), 45000, "phone-1".to_owned()));
        for table in ["phone_change", "phone_taken"] {
            assert!(conn.prepare(&format!("SELECT 1 FROM {table}")).is_err());
        }
        let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(
            count(
                "SELECT count(*) FROM meta
                 WHERE key IN ('file_id', 'snapshot_seq', 'role')"
            ),
            0
        );
        assert_eq!(
            count("SELECT count(*) FROM meta WHERE key = 'card_suggestions'"),
            1
        );
        // The old log stays as it was.
        assert_eq!(count("SELECT count(*) FROM change_log"), 2);
    }

    /// Version 9 adds the entered balance to the cards as they are: nothing entered yet.
    #[test]
    fn version_9_keeps_the_card_terms_and_leaves_the_balance_empty() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..8] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 8).unwrap();
        conn.execute_batch(
            "INSERT INTO product (short_name, name_key) VALUES ('Газпром КР', 'газпром кр');
             INSERT INTO credit_terms (product_id, credit_limit, default_grace_days)
                 VALUES (1, 15000000, 100);",
        )
        .unwrap();
        migrate(&mut conn).unwrap();

        assert_eq!(user_version(&conn).unwrap(), SCHEMA_VERSION);
        let row: (i64, i64, Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT credit_limit, default_grace_days, manual_available, manual_available_on
                 FROM credit_terms WHERE product_id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(row, (15_000_000, 100, None, None));
    }

    /// Version 10 gives rules and entries an empty MCC: every rule stays unique under the new
    /// key and an entry falls under the same rule as before.
    #[test]
    fn version_10_keeps_the_rules_unique_and_matched_the_same_way() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..9] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 9).unwrap();
        conn.execute_batch(
            "INSERT INTO product (short_name, name_key) VALUES ('Райф А', 'райф а');
             INSERT INTO category (name, name_key) VALUES ('продукты', 'продукты');
             INSERT INTO cashback_rule (month, product_id, category_id, rate_bp)
                 VALUES ('2026-07', 1, 1, 500), ('2026-07', 1, NULL, 100);
             INSERT INTO entry (month, position, title, title_key, category_id, product_id, amount,
                                created_at, updated_at)
                 VALUES ('2026-07', 1, 'Лента', 'лента', 1, 1, 100000, '', ''),
                        ('2026-07', 2, 'Кино', 'кино', NULL, 1, 50000, '', '');",
        )
        .unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(user_version(&conn).unwrap(), SCHEMA_VERSION);

        let july = "2026-07".parse().unwrap();
        let rules = crate::rules::rules_of(&conn, july).unwrap();
        let entries = crate::journal::entries_of(&conn, july).unwrap();
        assert!(rules.iter().all(|r| r.mcc.is_none()));
        assert!(entries.iter().all(|e| e.mcc.is_none()));
        let rates: Vec<_> = entries
            .iter()
            .map(|e| reestr_core::rule_of_entry(&rules, e).map(|r| r.rate.0))
            .collect();
        assert_eq!(rates, [Some(500), Some(100)]);

        // The key still refuses a second rule of a scope, with or without a category.
        let add = |category: Option<i64>, mcc: Option<&str>| {
            conn.execute(
                "INSERT INTO cashback_rule (month, product_id, category_id, mcc)
                 VALUES ('2026-07', 1, ?1, ?2)",
                params![category, mcc],
            )
        };
        assert!(add(Some(1), None).is_err());
        assert!(add(None, None).is_err());
        add(None, Some("0742")).unwrap();
        add(None, Some("5411")).unwrap();
        assert!(add(None, Some("0742")).is_err());
    }
}
