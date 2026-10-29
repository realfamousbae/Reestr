# Backlog

Ideas and findings that are not part of the current stage (PLAN §11: anything outside the data
model goes here, not into code). Newest first.

## Found during stage 0

- **Glide 6.0.3 patch** (`patches/@glideapps__glide-data-grid@6.0.3.patch`): typing a
  Cyrillic letter on a selected cell did not start editing (6.0.3 checks `/[ -~]/`, ASCII only).
  The patch backports the Unicode check of 6.0.4-alpha. Drop it when upgrading Glide.

- **Workbook «Служебный», column C.** It holds the words «да» / «нет» used by the
  `SUMIF(D…; Служебный!C2; G…)` formula for «Регулярные» — a vocabulary, not a per-category
  «regular» flag. Stage 1 import should not read it as `category.is_regular_default`.
- **Row-number column header.** Glide's row markers cannot show a «№» title; the numbers are
  there, the header is blank. Revisit if the author wants the label.
- **Very fast synthetic input** (0 ms between click and keys) can lose the first character
  when typing opens the cell editor. Human typing is not affected; keep an eye on it.
- **Amount display**: kopecks are shown only when non-zero («1 200» vs «1 234,50»). Make
  «always show kopecks» a setting if the mixed look bothers the author.

## Found during stage 2

- **Rules for imported months** (stage 3, right blocks): imported rows without cashback stay
  blank; a rule added later fills them only after the recompute question (PLAN §6.3). Keep it
  that way when the import learns to read the rules of the source workbook.

## Found before stage 3

- **Import of the right blocks** (PLAN §9 item 3, D25): the template's right blocks are empty,
  so there is nothing to recognize yet. Needs a real workbook or a sample of those blocks.
- **Manual credit card balances with a date** (PLAN §7, last bullet): not in the data model
  (§5). Add a field to `credit_terms` if the author wants it.

## Later

- **Phone: unlock with the fingerprint** (stage 5): the password is asked at every start for
  now, as agreed; a biometric unlock keeps the password in the Android keystore.

- MCC codes: rules by MCC and an MCC column in the journal (D20).

- `PRAGMA cipher_memory_security` — wipe SQLCipher memory allocations (performance cost).
- Calculator «кэшбэк vs проценты по накопительному» (PLAN §3).
