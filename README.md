# Reestr (Реестр)

A desktop family-budget app that replaces a hand-made Excel workbook: a monthly spending
journal, bank cards and cashback, all in one encrypted file. Tauri 2 + Rust core + React UI.
Windows builds for the computer and an Android app for entering purchases on the go, which
exchanges rows with the computer through a Yandex Disk folder (PLAN D38–D40); development works
on macOS and Linux too.

`PLAN.md` is the project plan (in Russian), `CLAUDE.md` the short rules.

## Layout

```
crates/core     domain and business logic: money, cashback and its rules, dictionaries,
                journal, totals, reading the old xlsx workbook (no SQL, no Tauri)
crates/storage  the encrypted .reestr file: SQLCipher, migrations, change log and undo, import
crates/files    Tauri plugin: files picked on Android stay reachable after a restart
crates/app      Tauri app: thin commands between the UI and core/storage; gen/android is the
                Android project
ui/             React + TypeScript UI (Vite), the month grid on Glide Data Grid
tools/fixtures  generator of the synthetic workbook used by the import tests
docs/           reference workbook, backlog, CI budget
site/           the user manual (Astro Starlight, Russian)
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
Screenshots in `site/src/assets/screens/` are taken from a demo file with fictional data:
`cargo run -p reestr-storage --example demo_file -- <path>/Семья.reestr` (password `demo-reestr`).

## Android build

The APK is built by GitHub Actions (`.github/workflows/android-build.yml`, by hand and on `v*`
tags) or locally with the Android SDK and NDK (`ANDROID_HOME`, `NDK_HOME`, and the NDK's
`toolchains/llvm/prebuilt/<host>/bin` on `PATH` for the OpenSSL build):

```sh
rustup target add aarch64-linux-android
pnpm tauri android build --apk --target aarch64
```

It is signed with the release key when `crates/app/gen/android/keystore.properties` exists
(`password`, `keyAlias`, `storeFile`; never committed). In GitHub the key comes from the secrets
`ANDROID_KEYSTORE` (the `.jks` file in base64) and `ANDROID_KEY_PASSWORD`. Keep the key: updates
install over the app only when signed with the same one.

The phone's screens can be tried in the desktop app: set `localStorage["reestr.phone"] = "1"`
in its web inspector and reload.

## Releasing

The version lives in the workspace `Cargo.toml` (`[workspace.package] version`) and in
`crates/app/tauri.conf.json` (Android reads only the latter); a test keeps the two equal. To
release, merge to `master`, then either run **Windows build** on `master` with **release** set to
the tag (e.g. `v0.1.0`), or push that tag:

```sh
git tag v0.1.0 && git push origin v0.1.0
```

The release is published with the `.exe` and `.msi` once the build succeeds; a failed build
publishes nothing. Run **Android build** with the same tag (a pushed tag runs it too) to attach
the APK.

## License

MIT, see `LICENSE`.
