# CI budget

The repository has been public since 2026-11-02, so standard GitHub-hosted runners cost nothing
(PLAN D42). While it was private, the minutes were limited and Windows minutes counted ×2; that
is why CI runs were kept few (D36). The Windows build is still slow and publishes releases, so it
stays manual. The tables below record how long the runs take.

| Workflow            | Runner         | Trigger                                      | Expected time                                                         |
| ------------------- | -------------- | -------------------------------------------- | --------------------------------------------------------------------- |
| `ci.yml`            | ubuntu-latest  | push to `dev`, ready PR to `master`, by hand | 3–6 min (cached)                                                      |
| `windows-build.yml` | windows-latest | manual, `v*` tags                            | 5–15 min (cached), 25–35 min first run (OpenSSL is built from source) |

## Measured

Add a row after notable runs (time of the run, cache hit or miss).

| Date       | Workflow            | Cache            | Duration                |
| ---------- | ------------------- | ---------------- | ----------------------- |
| 2026-10-27 | `ci.yml`            | miss (first run) | 3 min 37 s              |
| 2026-10-28 | `windows-build.yml` | miss (first run) | 24 min (release v0.1.0) |
| 2026-10-28 | `ci.yml`            | hit              | 6 min (PR #2)           |
| 2026-10-28 | `windows-build.yml` | hit              | 4 min 38 s (v0.2.0)     |
| 2026-10-28 | `android-build.yml` | miss (first run) | 14 min (v0.2.0)         |
