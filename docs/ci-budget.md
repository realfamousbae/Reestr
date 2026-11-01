# CI budget

GitHub Actions minutes for a private repository are limited (PLAN §16). Windows runners cost
more: GitHub's billing pages disagree on whether Windows minutes still count ×2 against the
included quota, so plan as if they do.

| Workflow            | Runner         | Trigger                       | Expected time                                                          | Counted minutes |
| ------------------- | -------------- | ----------------------------- | ---------------------------------------------------------------------- | --------------- |
| `ci.yml`            | ubuntu-latest  | ready PR to `master`, by hand | 4–6 min (cached)                                                       | ×1              |
| `windows-build.yml` | windows-latest | manual, `v*` tags             | 10–15 min (cached), 25–35 min first run (OpenSSL is built from source) | ×2              |

## Measured

Add a row after notable runs (time of the run, cache hit or miss).

| Date       | Workflow            | Cache            | Duration                |
| ---------- | ------------------- | ---------------- | ----------------------- |
| 2026-10-27 | `ci.yml`            | miss (first run) | 3 min 37 s              |
| 2026-10-28 | `windows-build.yml` | miss (first run) | 24 min (release v0.1.0) |
| 2026-10-28 | `ci.yml`            | hit              | 6 min (PR #2)           |
| 2026-10-28 | `windows-build.yml` | hit              | 4 min 38 s (v0.2.0)     |
| 2026-10-28 | `android-build.yml` | miss (first run) | 14 min (v0.2.0)         |
