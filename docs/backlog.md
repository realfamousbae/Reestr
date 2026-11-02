# Backlog

Ideas and findings that are not part of the current stage (PLAN §11: anything outside the data
model goes here, not into code). Newest first.

## Found during stage 0

- **Glide 6.0.3 patch** (`patches/@glideapps__glide-data-grid@6.0.3.patch`), three changes:
  typing a Cyrillic letter on a selected cell did not start editing (6.0.3 checks `/[ -~]/`,
  ASCII only; the patch backports the Unicode check of 6.0.4-alpha); Russian labels of the search
  box; the cell editor is imported up front instead of `React.lazy`, because while it loaded the
  keys of the first edit after launch went to the grid and only the last one was kept. When
  upgrading Glide, drop what the new version fixes and check that the first edit still keeps
  every key.

- **xlsx «Служебный», column C.** It holds the words «да» / «нет» used by the
  `SUMIF(D…; Служебный!C2; G…)` formula for «Регулярные» — a vocabulary, not a per-category
  «regular» flag. Stage 1 import should not read it as `category.is_regular_default`.

## Found during stage 2

- **Rules for imported months** (stage 3, right blocks): imported rows without cashback stay
  blank; a rule added later fills them only after the recompute question (PLAN §6.3). Keep it
  that way when the import learns to read the rules from an xlsx.

## Found before stage 3

- **Import of the right blocks** (PLAN §9 item 3, D25): their xlsx layout is not defined yet,
  so there is nothing to recognize. Needs a sample of filled-in blocks.
