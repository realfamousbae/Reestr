# Reestr (Реестр)

A desktop family-budget app laid out like a spreadsheet: a monthly spending journal, bank
cards and cashback, all in one encrypted file. Tauri 2 + Rust core + React UI.
Windows builds; development works on macOS and Linux too.

`PLAN.md` is the project plan (in Russian), `CLAUDE.md` the short rules.

## Layout

```
crates/core     domain and business logic: money, cashback and its rules, dictionaries,
                journal, totals, xlsx import and export (no SQL, no Tauri)
crates/storage  the encrypted .reestr file: SQLCipher, migrations, change log and undo, import
crates/app      Tauri app: thin commands between the UI and core/storage
ui/             React + TypeScript UI (Vite), the month grid on Glide Data Grid
tools/fixtures  generator of the synthetic workbook used by the import tests
docs/           backlog, CI budget
site/           the user manual (Astro Starlight, Russian)
landing/        the landing page with the download buttons (static HTML, Caddy on Railway)
```

## Development

Requirements: Rust (stable), Node 22+, pnpm 10 (`corepack enable`), and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```sh
pnpm install
pnpm dev                 # the desktop app with hot reload
cargo test --workspace   # Rust tests
pnpm check               # format, typecheck, lint and UI tests
```

## Windows build

Installers are built by GitHub Actions only (`.github/workflows/windows-build.yml`):
Actions → **Windows build** → **Run workflow**. The `.exe` and `.msi` appear as artifacts of the
run.

The installers are not code-signed, so Windows SmartScreen warns on first launch:
click **Подробнее** → **Выполнить в любом случае**.

**For users** (in Russian): the user manual is a static site in `site/` (Astro Starlight), published
on Railway at https://reestr-docs.up.railway.app from the `dev` branch (`site/Dockerfile`):
`pnpm site:dev` serves it at `http://localhost:4321`, `pnpm site:build` writes `site/dist/`.
`USER_GUIDE.md` points to its pages.

The landing page (https://reestr.up.railway.app) is plain HTML in `landing/`, served by Caddy
on Railway from the `dev` branch (`landing/Dockerfile`); no build step and no requests to other
hosts. Its download links name the release files, so **every release updates the version in
`landing/index.html`** (search for the previous one, e.g. `0.3.1`).
Screenshots in `site/src/assets/screens/` are taken from a demo file with fictional data:
`cargo run -p reestr-storage --example demo_file -- <path>/Семья.reestr` (password `demo-reestr`).

## Releasing

The version lives in the workspace `Cargo.toml` (`[workspace.package] version`) and in
`crates/app/tauri.conf.json`; a test keeps the two equal. To
release, merge to `master`, then either run **Windows build** on `master` with **release** set to
the tag (e.g. `v0.1.0`), or push that tag:

```sh
git tag v0.1.0 && git push origin v0.1.0
```

The release is published with the `.exe` and `.msi` once the build succeeds; a failed build
publishes nothing.

## License

MIT, see `LICENSE`.
