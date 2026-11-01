# CLAUDE.md — rules for working on Reestr

Reestr is a desktop family-budget app (Tauri 2 + Rust core + React UI) that replaces a household
Excel workbook (its layout is reproduced by `tools/fixtures/make_workbook.py`). `PLAN.md` is the
source of truth; this file condenses the rules from its §8, §10, §15 and §16.

- Test data, fixtures, examples and screenshots are fictional only (participants Андрей and
  Марина, made-up cards). Never commit real names, card digits, amounts or files of real people.

## Language

- Talk to the author in **Russian**.
- Code, comments, commit messages, file names, docs in `docs/` — **English**.
- User-facing UI strings are Russian.

## Process

- Before each stage (§12): a short plan, then implement only after the author confirms.
- Commits: English, Conventional Commits (`feat:`, `fix:`, `chore:`, `ci:`, `docs:`, `test:`…).
- Anything not in the data model (§5) goes to `docs/backlog.md`, not into code.
- The user manual is the site in `site/` (Astro Starlight, Russian). Every change a user can see
  updates its pages in the same commit: the task guide (`guide/`, `start/`), the reference of
  the screen (`reference/`, labels exactly as in the UI) and, for new error texts,
  `help/messages.md`. A release adds an entry to `help/changelog.md`.
- Verify external library APIs (`tauri`, `@glideapps/glide-data-grid`, `rusqlite`/SQLCipher,
  `calamine`, `rust_xlsxwriter`) against current docs, not memory.

## Architecture rules

- `crates/core`: domain + business logic as pure functions with tests. Knows nothing about
  SQLite or Tauri.
- `crates/storage`: SQLite/SQLCipher, schema, migrations, snapshots, `change_log`. No Tauri.
- `crates/app`: Tauri commands only — thin glue and serialization.
- `ui/`: React 18.3 + TypeScript (strict) + Glide Data Grid 6.0.3 (PLAN D12: no React 19 until
  Glide ships a stable release for it). **The UI never computes money**: totals, cashback,
  limits, debts come from the core. The UI may only parse input and format output.

## Money and dates

- Money is integer kopecks, `i64` in Rust, a plain integer in TS. Single currency (RUB).
- Any `f64`/float used for money is a bug. Rates are basis points (`rate_bp`).
- Dates are ISO `YYYY-MM-DD`, months `YYYY-MM`. UI shows `DD.MM.YYYY` and `MM.YY` tabs.

## Dependencies

- Add a dependency only when needed; every new one gets a one-line "why" in the PR.
- Forbidden: anything with network activity (analytics, telemetry, auto-update, downloaders).

## UI (§8)

- The month screen mirrors the source workbook's sheet: same columns in the same order
  (`№ | Наименование | Назначение | Регулярная | Участник | Дата | Сумма | % | Кэшбэк | Карта`),
  category totals above the journal, blocks on the right, month tabs `MM.YY`, a «Свод» sheet.
  Changing column/block order requires an explicit decision by the author.
- Excel feel: Enter/Tab/arrows, `Ctrl+Z`, paste ranges from clipboard, new row always at the
  bottom, delete with confirmation, autocomplete of «Наименование» from history.
- Shortcuts: `Ctrl+N` new entry, `Ctrl+F` search, `Ctrl+Tab` next month.
- Design via the `frontend-design` skill, but restrained: the grid is the main element,
  minimal accents, color only for statuses (deadline close, limit exhausted).
- Contrast ≥ 4.5:1; grid font size is a user setting.

## Security and privacy (§10)

- Zero network requests. Tauri CSP forbids external sources.
- The data file (`.reestr`) is a SQLCipher database encrypted as a whole. The password is not
  recoverable — say so prominently on the create-file screen.
- Keep the password in memory only as long as needed; wipe it (`zeroize`).
- xlsx export is not encrypted — warn on export.
- Clipboard copy is plain text, no warnings (expected Excel behaviour).
- App logs contain technical events only — never amounts, titles or names.

## Environment and CI (§16)

- Code must not depend on where it is written: no absolute paths, no OS assumptions in tests
  (`std::path`, `tempfile`).
- Dev machine is a MacBook Air M2 / 8 GB: Windows builds only in CI, no heavy emulators.
- In a cloud session there is no Tauri window: verify via `cargo test`, `cargo clippy`,
  `cargo fmt --check`, `tsc`, `eslint`, `vitest`.
- GitHub Actions minutes are limited (Windows counts ×2):
  - `ci.yml` (ubuntu): tests, clippy (including the Tauri crate), tsc, vitest — only on a
    ready (not draft) `pull_request` to `master` and by hand; not on pushes, not on docs-only
    changes. The default branch is `master`.
  - `windows-build.yml` (windows): full Tauri `.exe`/`.msi` — `workflow_dispatch` and `v*` tags
    only, never on every PR.
  - **D36: CI and builds only for large changes.** Small changes gather on the working branch
    without a pull request and are checked locally before every push. A pull request (and so
    CI) is opened when a large change is ready. Windows builds and releases run only
    when the author says so.
  - All use caches (`Swatinem/rust-cache`, pnpm) and `concurrency` with `cancel-in-progress`.
- `cargo test` and `vitest` must be green before opening a PR.
- Tests are meaningful: money, cashback, grace periods, import. UI tests only for pure
  functions (formatting, input parsing). No e2e in MVP.
