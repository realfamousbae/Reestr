//! Cashback rules of a month (PLAN §6, D20, D21): which rule an entry falls under, how the
//! limits are spent in journal order, what is left, and which card to pay with (D6).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::cashback::{Cashback, CashbackSource, RateBp, Rounding, cashback_for};
use crate::dict::{CategoryId, ProductId};
use crate::journal::{Entry, EntryId};
use crate::money::Kopecks;
use crate::month::Month;

pub type RuleId = i64;

/// «Райф А: продукты 5%, до 30 000 ₽ трат, не больше 1 000 ₽ кэшбэка» for one month.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CashbackRule {
    pub id: RuleId,
    pub month: Month,
    pub product_id: ProductId,
    /// `None` is «на всё»: everything the card pays for that has no rule of its own.
    pub category_id: Option<CategoryId>,
    pub rate: RateBp,
    /// Spending that earns cashback; above it the rate no longer applies.
    pub spend_cap: Option<Kopecks>,
    /// The most cashback the rule pays in the month.
    pub cashback_cap: Option<Kopecks>,
    /// Spending that is not in the journal (D8); it earns cashback at the rule's rate.
    pub spent_adjustment: Kopecks,
    pub notes: Option<String>,
}

/// A change of one rule field as the UI sends it. The card and the category are names; missing
/// ones are created (D17). An empty category means «на всё».
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
pub enum RuleInput {
    Card(Option<String>),
    Category(Option<String>),
    Rate(RateBp),
    SpendCap(Option<Kopecks>),
    CashbackCap(Option<Kopecks>),
    Adjustment(Kopecks),
    Notes(Option<String>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum RuleOp {
    Edit {
        id: RuleId,
        edit: RuleInput,
    },
    /// A new rule of the month; `edits` must name the card.
    Append {
        edits: Vec<RuleInput>,
    },
    Delete {
        ids: Vec<RuleId>,
    },
    /// Copies the rules of `from` that the month does not have yet (PLAN §6.5). The
    /// adjustments are not copied: they belong to their month.
    CopyFrom {
        from: Month,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuleError {
    #[error("rate {0} bp is above 100%")]
    RateTooHigh(u32),
    #[error("a limit cannot be negative")]
    NegativeLimit,
}

/// Checks a field value before it is stored.
pub fn validate(input: &RuleInput) -> Result<(), RuleError> {
    match input {
        RuleInput::Rate(rate) if *rate > RateBp::MAX => Err(RuleError::RateTooHigh(rate.0)),
        RuleInput::SpendCap(Some(cap)) | RuleInput::CashbackCap(Some(cap))
            if *cap < Kopecks::ZERO =>
        {
            Err(RuleError::NegativeLimit)
        }
        _ => Ok(()),
    }
}

/// The rule an entry of this month, card and category falls under: the card's rule for the
/// category, else the card's «на всё» rule (D21).
pub fn rule_for(
    rules: &[CashbackRule],
    month: Month,
    product: ProductId,
    category: Option<CategoryId>,
) -> Option<&CashbackRule> {
    let mut of_card = rules
        .iter()
        .filter(|r| r.month == month && r.product_id == product);
    let exact = category.and_then(|c| of_card.clone().find(|r| r.category_id == Some(c)));
    exact.or_else(|| of_card.find(|r| r.category_id.is_none()))
}

/// The rule of an entry, if its card has one for its category.
pub fn rule_of_entry<'a>(rules: &'a [CashbackRule], entry: &Entry) -> Option<&'a CashbackRule> {
    entry
        .product_id
        .and_then(|p| rule_for(rules, entry.month, p, entry.category_id))
}

/// What a rule has used so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Tally {
    spent: Kopecks,
    cashback: Kopecks,
    entries: usize,
}

impl CashbackRule {
    /// Cashback for `amount` spent under the rule after `used`. A purchase earns only on the
    /// spending left under the spend cap and only up to the cashback left under the cashback
    /// cap; a refund takes back cashback at the full rate. When a cap cut the cashback, the
    /// rate is left out so the `%` column shows the effective rate.
    fn cashback_after(&self, used: Tally, amount: Kopecks, rounding: Rounding) -> Cashback {
        let full = cashback_for(amount, self.rate, rounding);
        let mut cashback = full;
        if amount > Kopecks::ZERO {
            if let Some(cap) = self.spend_cap {
                let eligible = amount.min((cap - used.spent).max(Kopecks::ZERO));
                cashback = cashback_for(eligible, self.rate, rounding);
            }
            if let Some(cap) = self.cashback_cap {
                cashback = cashback.min((cap - used.cashback).max(Kopecks::ZERO));
            }
        }
        Cashback {
            amount: cashback,
            rate: (cashback == full).then_some(self.rate),
            source: CashbackSource::Rule,
        }
    }

    /// The tally before the first journal entry: the adjustment, as if it were spent first.
    fn opening(&self, rounding: Rounding) -> Tally {
        let adjustment = self.spent_adjustment;
        Tally {
            spent: adjustment,
            cashback: self
                .cashback_after(Tally::default(), adjustment, rounding)
                .amount,
            entries: 0,
        }
    }

    fn usage(&self, tally: Tally) -> RuleUsage {
        RuleUsage {
            rule_id: self.id,
            spent: tally.spent,
            cashback: tally.cashback,
            entries: tally.entries,
            remaining_spend: self.spend_cap.map(|c| (c - tally.spent).max(Kopecks::ZERO)),
            remaining_cashback: self
                .cashback_cap
                .map(|c| (c - tally.cashback).max(Kopecks::ZERO)),
        }
    }
}

/// How much of a rule's limits is used: the «Лимиты по картам» block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleUsage {
    pub rule_id: RuleId,
    /// Journal entries under the rule plus the adjustment.
    pub spent: Kopecks,
    /// Their cashback plus the cashback of the adjustment.
    pub cashback: Kopecks,
    /// Journal entries under the rule.
    pub entries: usize,
    /// Never below zero.
    pub remaining_spend: Option<Kopecks>,
    /// Never below zero.
    pub remaining_cashback: Option<Kopecks>,
}

impl RuleUsage {
    /// Nothing more to earn: a cap is reached.
    pub fn is_exhausted(&self) -> bool {
        self.remaining_spend == Some(Kopecks::ZERO)
            || self.remaining_cashback == Some(Kopecks::ZERO)
    }
}

/// Rule-based cashback of a month and the usage of its rules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Allocation {
    /// The new cashback of every entry that follows the rules (source `Rule`); `None` when its
    /// card and category have no rule any more.
    pub cashback: Vec<(EntryId, Option<Cashback>)>,
    /// Usage of every rule, in the order of `rules`.
    pub usage: Vec<RuleUsage>,
}

/// Spends the limits of the month's rules in journal order (D21). `entries` are the month's
/// entries in row order; `rounding` gives each card's rounding (D13). Entries with a typed
/// cashback keep it but use up the limits; entries that follow the rules get their cashback
/// from what is left when their row comes.
pub fn allocate(
    entries: &[Entry],
    rules: &[CashbackRule],
    rounding: impl Fn(ProductId) -> Rounding,
) -> Allocation {
    let mut tallies: HashMap<RuleId, Tally> = rules
        .iter()
        .map(|r| (r.id, r.opening(rounding(r.product_id))))
        .collect();
    let mut cashback = Vec::new();
    for entry in entries {
        let rule = rule_of_entry(rules, entry);
        let follows_rules = entry
            .cashback
            .is_some_and(|c| c.source == CashbackSource::Rule);
        let earned = if follows_rules {
            let new = rule.map(|r| {
                let used = tallies.get(&r.id).copied().unwrap_or_default();
                r.cashback_after(used, entry.amount, rounding(r.product_id))
            });
            cashback.push((entry.id, new));
            new
        } else {
            entry.cashback
        };
        if let Some(rule) = rule {
            let tally = tallies.entry(rule.id).or_default();
            tally.spent += entry.amount;
            tally.cashback += earned.map_or(Kopecks::ZERO, |c| c.amount);
            tally.entries += 1;
        }
    }
    let usage = rules
        .iter()
        .map(|r| r.usage(tallies.get(&r.id).copied().unwrap_or_default()))
        .collect();
    Allocation { cashback, usage }
}

/// A card to pay with for a category, from the month's rules (D6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardSuggestion {
    pub product_id: ProductId,
    pub rule_id: RuleId,
    pub rate: RateBp,
    /// Cashback for the given amount with what is left of the limits.
    pub expected: Option<Kopecks>,
    pub exhausted: bool,
    pub remaining_spend: Option<Kopecks>,
    pub remaining_cashback: Option<Kopecks>,
}

/// Cards with a rule for `category` in `month`, best first: by the cashback the amount would
/// earn with what is left of the limits, or by rate when the amount is not known yet; cards
/// whose limit is used up go last. `usage` comes from [`allocate`] over the month without the
/// entry being typed.
pub fn suggest_cards(
    rules: &[CashbackRule],
    usage: &[RuleUsage],
    month: Month,
    category: Option<CategoryId>,
    amount: Option<Kopecks>,
    rounding: impl Fn(ProductId) -> Rounding,
) -> Vec<CardSuggestion> {
    let mut products: Vec<ProductId> = rules
        .iter()
        .filter(|r| r.month == month)
        .map(|r| r.product_id)
        .collect();
    products.sort_unstable();
    products.dedup();

    let mut suggestions: Vec<CardSuggestion> = products
        .into_iter()
        .filter_map(|product| {
            let rule = rule_for(rules, month, product, category)?;
            let used = usage.iter().find(|u| u.rule_id == rule.id);
            let tally = used.map_or_else(
                || rule.opening(rounding(product)),
                |u| Tally {
                    spent: u.spent,
                    cashback: u.cashback,
                    entries: u.entries,
                },
            );
            let usage = rule.usage(tally);
            Some(CardSuggestion {
                product_id: product,
                rule_id: rule.id,
                rate: rule.rate,
                expected: amount.map(|a| rule.cashback_after(tally, a, rounding(product)).amount),
                exhausted: usage.is_exhausted(),
                remaining_spend: usage.remaining_spend,
                remaining_cashback: usage.remaining_cashback,
            })
        })
        .collect();
    suggestions.sort_by(|a, b| {
        a.exhausted
            .cmp(&b.exhausted)
            .then_with(|| b.expected.cmp(&a.expected))
            .then_with(|| b.rate.cmp(&a.rate))
            .then_with(|| a.rule_id.cmp(&b.rule_id))
    });
    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: ProductId = 1;
    const OTHER_CARD: ProductId = 2;
    const FOOD: CategoryId = 10;
    const GROCERIES: CategoryId = 11;

    fn july() -> Month {
        "2026-07".parse().unwrap()
    }

    fn rub(r: i64) -> Kopecks {
        Kopecks::from_rubles(r)
    }

    fn rule(id: RuleId, product: ProductId, category: Option<CategoryId>, bp: u32) -> CashbackRule {
        CashbackRule {
            id,
            month: july(),
            product_id: product,
            category_id: category,
            rate: RateBp(bp),
            spend_cap: None,
            cashback_cap: None,
            spent_adjustment: Kopecks::ZERO,
            notes: None,
        }
    }

    /// An entry that follows the rules (cashback not typed).
    fn by_rule(
        id: EntryId,
        product: ProductId,
        category: Option<CategoryId>,
        amount: Kopecks,
    ) -> Entry {
        let mut e = Entry::new(id, july());
        e.product_id = Some(product);
        e.category_id = category;
        e.amount = amount;
        e.cashback = Some(Cashback::pending_rule());
        e
    }

    fn floor_rub(_: ProductId) -> Rounding {
        Rounding::FloorRub
    }

    fn cashback_of(a: &Allocation, id: EntryId) -> Option<Cashback> {
        a.cashback
            .iter()
            .find(|(e, _)| *e == id)
            .and_then(|(_, c)| *c)
    }

    fn amount_of(a: &Allocation, id: EntryId) -> Kopecks {
        cashback_of(a, id).map_or(Kopecks::ZERO, |c| c.amount)
    }

    #[test]
    fn category_rule_wins_over_everything_rule() {
        let rules = [
            rule(1, CARD, None, 100),
            rule(2, CARD, Some(GROCERIES), 500),
        ];
        assert_eq!(
            rule_for(&rules, july(), CARD, Some(GROCERIES)).map(|r| r.id),
            Some(2)
        );
        assert_eq!(
            rule_for(&rules, july(), CARD, Some(FOOD)).map(|r| r.id),
            Some(1)
        );
        assert_eq!(rule_for(&rules, july(), CARD, None).map(|r| r.id), Some(1));
        assert_eq!(rule_for(&rules, july(), OTHER_CARD, Some(FOOD)), None);
        let august = "2026-08".parse().unwrap();
        assert_eq!(rule_for(&rules, august, CARD, Some(FOOD)), None);

        let only_category = [rule(2, CARD, Some(GROCERIES), 500)];
        assert_eq!(rule_for(&only_category, july(), CARD, Some(FOOD)), None);
    }

    #[test]
    fn everything_rule_does_not_count_categories_with_their_own_rule() {
        let mut everything = rule(1, CARD, None, 100);
        everything.spend_cap = Some(rub(10_000));
        let rules = [everything, rule(2, CARD, Some(GROCERIES), 500)];
        let entries = [
            by_rule(1, CARD, Some(GROCERIES), rub(2000)),
            by_rule(2, CARD, Some(FOOD), rub(3000)),
            by_rule(3, CARD, None, rub(1000)),
        ];
        let a = allocate(&entries, &rules, floor_rub);
        assert_eq!(amount_of(&a, 1), rub(100));
        assert_eq!(amount_of(&a, 2), rub(30));
        assert_eq!(amount_of(&a, 3), rub(10));
        assert_eq!(a.usage[0].spent, rub(4000));
        assert_eq!(a.usage[0].entries, 2);
        assert_eq!(a.usage[0].remaining_spend, Some(rub(6000)));
        assert_eq!(a.usage[1].spent, rub(2000));
        assert_eq!(a.usage[1].cashback, rub(100));
    }

    #[test]
    fn spend_cap_partly_used_up() {
        let mut r = rule(1, CARD, Some(GROCERIES), 500);
        r.spend_cap = Some(rub(3000));
        let entries = [
            by_rule(1, CARD, Some(GROCERIES), rub(2000)),
            by_rule(2, CARD, Some(GROCERIES), rub(2000)),
            by_rule(3, CARD, Some(GROCERIES), rub(500)),
        ];
        let a = allocate(&entries, &[r], floor_rub);
        assert_eq!(amount_of(&a, 1), rub(100));
        // Only 1 000 ₽ of 2 000 ₽ is under the cap: 50 ₽, shown as 2,5%.
        let second = cashback_of(&a, 2).unwrap();
        assert_eq!(second.amount, rub(50));
        assert_eq!(second.rate, None);
        assert_eq!(second.source, CashbackSource::Rule);
        let mut e = entries[1].clone();
        e.cashback = Some(second);
        assert_eq!(e.display_rate_bp(), Some(250));
        // The cap is reached: nothing more.
        assert_eq!(amount_of(&a, 3), Kopecks::ZERO);
        assert_eq!(a.usage[0].spent, rub(4500));
        assert_eq!(a.usage[0].remaining_spend, Some(Kopecks::ZERO));
        assert!(a.usage[0].is_exhausted());
    }

    #[test]
    fn cashback_cap_partly_used_up() {
        let mut r = rule(1, CARD, Some(GROCERIES), 500);
        r.cashback_cap = Some(rub(120));
        let entries = [
            by_rule(1, CARD, Some(GROCERIES), rub(2000)),
            by_rule(2, CARD, Some(GROCERIES), rub(2000)),
        ];
        let a = allocate(&entries, &[r], floor_rub);
        assert_eq!(amount_of(&a, 1), rub(100));
        assert_eq!(amount_of(&a, 2), rub(20));
        assert_eq!(a.usage[0].cashback, rub(120));
        assert_eq!(a.usage[0].remaining_cashback, Some(Kopecks::ZERO));
        assert!(a.usage[0].is_exhausted());
    }

    #[test]
    fn limits_go_in_row_order() {
        let mut r = rule(1, CARD, Some(GROCERIES), 1000);
        r.cashback_cap = Some(rub(150));
        let entries = [
            by_rule(7, CARD, Some(GROCERIES), rub(1000)),
            by_rule(3, CARD, Some(GROCERIES), rub(1000)),
        ];
        let a = allocate(&entries, &[r], floor_rub);
        // The first row gets the full 100 ₽, whatever its id.
        assert_eq!(amount_of(&a, 7), rub(100));
        assert_eq!(amount_of(&a, 3), rub(50));
    }

    #[test]
    fn typed_cashback_stays_but_uses_the_limit() {
        let mut r = rule(1, CARD, Some(GROCERIES), 500);
        r.spend_cap = Some(rub(3000));
        r.cashback_cap = Some(rub(200));
        let mut typed = by_rule(1, CARD, Some(GROCERIES), rub(2500));
        typed.cashback = Some(Cashback::manual_amount(rub(180)));
        let entries = [typed, by_rule(2, CARD, Some(GROCERIES), rub(1000))];
        let a = allocate(&entries, &[r], floor_rub);
        // Typed rows are not in the result: they keep their cashback.
        assert_eq!(cashback_of(&a, 1), None);
        assert!(a.cashback.iter().all(|(id, _)| *id != 1));
        // 500 ₽ left under the spend cap → 25 ₽, and 20 ₽ left under the cashback cap → 20 ₽.
        assert_eq!(amount_of(&a, 2), rub(20));
        assert_eq!(a.usage[0].spent, rub(3500));
        assert_eq!(a.usage[0].cashback, rub(200));
    }

    #[test]
    fn adjustment_is_spending_outside_the_journal() {
        let mut r = rule(1, CARD, Some(GROCERIES), 500);
        r.spend_cap = Some(rub(3000));
        r.spent_adjustment = rub(2500);
        let entries = [by_rule(1, CARD, Some(GROCERIES), rub(1000))];
        let a = allocate(&entries, &[r.clone()], floor_rub);
        assert_eq!(amount_of(&a, 1), rub(25));
        assert_eq!(a.usage[0].spent, rub(3500));
        // 125 ₽ for the adjustment plus 25 ₽ for the entry.
        assert_eq!(a.usage[0].cashback, rub(150));
        assert_eq!(a.usage[0].entries, 1);

        // A negative adjustment: the bank counted less than the journal shows.
        r.spent_adjustment = -rub(1000);
        let a = allocate(&entries, &[r], floor_rub);
        assert_eq!(amount_of(&a, 1), rub(50));
        assert_eq!(a.usage[0].spent, Kopecks::ZERO);
        assert_eq!(a.usage[0].cashback, Kopecks::ZERO);
        assert_eq!(a.usage[0].remaining_spend, Some(rub(3000)));
    }

    #[test]
    fn refunds_take_back_cashback_and_free_the_limit() {
        let mut r = rule(1, CARD, Some(GROCERIES), 500);
        r.spend_cap = Some(rub(3000));
        let entries = [
            by_rule(1, CARD, Some(GROCERIES), rub(3000)),
            by_rule(2, CARD, Some(GROCERIES), -rub(1000)),
            by_rule(3, CARD, Some(GROCERIES), rub(1000)),
        ];
        let a = allocate(&entries, &[r], floor_rub);
        assert_eq!(amount_of(&a, 1), rub(150));
        assert_eq!(amount_of(&a, 2), -rub(50));
        assert_eq!(cashback_of(&a, 2).unwrap().rate, Some(RateBp(500)));
        assert_eq!(amount_of(&a, 3), rub(50));
        assert_eq!(a.usage[0].spent, rub(3000));
    }

    #[test]
    fn rounding_of_each_card() {
        let rules = [rule(1, CARD, None, 350), rule(2, OTHER_CARD, None, 350)];
        // 1 234,50 ₽ × 3,5% = 43,2075 ₽.
        let entries = [
            by_rule(1, CARD, None, Kopecks(123_450)),
            by_rule(2, OTHER_CARD, None, Kopecks(123_450)),
        ];
        for (mode, expected) in [
            (Rounding::FloorRub, Kopecks(4300)),
            (Rounding::FloorKop, Kopecks(4320)),
            (Rounding::HalfUpRub, Kopecks(4300)),
            (Rounding::HalfUpKop, Kopecks(4321)),
        ] {
            let a = allocate(&entries, &rules, |p| {
                if p == OTHER_CARD {
                    mode
                } else {
                    Rounding::FloorRub
                }
            });
            assert_eq!(amount_of(&a, 1), Kopecks(4300));
            assert_eq!(amount_of(&a, 2), expected, "{mode:?}");
        }
    }

    #[test]
    fn kopecks_under_a_cap() {
        let mut r = rule(1, CARD, None, 500);
        r.spend_cap = Some(Kopecks(100_050)); // 1 000,50 ₽
        r.cashback_cap = Some(Kopecks(4_999)); // 49,99 ₽
        let entries = [by_rule(1, CARD, None, rub(2000))];
        let a = allocate(&entries, &[r.clone()], |_| Rounding::FloorKop);
        // 1 000,50 × 5% = 50,025 → 50,02, then the cashback cap → 49,99.
        assert_eq!(amount_of(&a, 1), Kopecks(4_999));
        r.cashback_cap = None;
        let a = allocate(&entries, &[r], |_| Rounding::FloorKop);
        assert_eq!(amount_of(&a, 1), Kopecks(5_002));
    }

    #[test]
    fn zero_rate_and_empty_amount() {
        let rules = [rule(1, CARD, None, 0)];
        let entries = [
            by_rule(1, CARD, None, rub(1000)),
            by_rule(2, CARD, None, Kopecks::ZERO),
        ];
        let a = allocate(&entries, &rules, floor_rub);
        assert_eq!(cashback_of(&a, 1).unwrap().amount, Kopecks::ZERO);
        assert_eq!(cashback_of(&a, 1).unwrap().rate, Some(RateBp(0)));
        assert_eq!(cashback_of(&a, 2).unwrap().amount, Kopecks::ZERO);
        assert!(!a.usage[0].is_exhausted());
    }

    #[test]
    fn rule_cashback_goes_away_with_the_rule() {
        let entries = [
            by_rule(1, CARD, Some(FOOD), rub(1000)),
            by_rule(2, OTHER_CARD, Some(FOOD), rub(1000)),
        ];
        let a = allocate(&entries, &[rule(1, CARD, None, 100)], floor_rub);
        assert_eq!(amount_of(&a, 1), rub(10));
        assert_eq!(a.cashback.iter().find(|(id, _)| *id == 2), Some(&(2, None)));
        // Blank cashback that never asked for a rule stays blank.
        let mut blank = by_rule(3, CARD, Some(FOOD), rub(1000));
        blank.cashback = None;
        let a = allocate(&[blank], &[rule(1, CARD, None, 100)], floor_rub);
        assert!(a.cashback.is_empty());
        assert_eq!(a.usage[0].spent, rub(1000));
        assert_eq!(a.usage[0].cashback, Kopecks::ZERO);
    }

    #[test]
    fn suggestions_best_first_used_up_last() {
        let mut high = rule(1, CARD, Some(GROCERIES), 1000);
        high.cashback_cap = Some(rub(100));
        let low = rule(2, OTHER_CARD, None, 100);
        let mut mid = rule(3, 3, Some(GROCERIES), 500);
        mid.spend_cap = Some(rub(1000));
        let unrelated = rule(4, 4, Some(FOOD), 700);
        let rules = [high, low, mid, unrelated];
        let entries = [by_rule(1, CARD, Some(GROCERIES), rub(500))];
        let usage = allocate(&entries, &rules, floor_rub).usage;

        // No amount yet: by rate.
        let s = suggest_cards(&rules, &usage, july(), Some(GROCERIES), None, floor_rub);
        let order: Vec<_> = s.iter().map(|c| c.product_id).collect();
        assert_eq!(order, vec![CARD, 3, OTHER_CARD]);
        assert_eq!(s[0].remaining_cashback, Some(rub(50)));
        assert_eq!(s[1].remaining_spend, Some(rub(1000)));

        // 2 000 ₽: card 1 has 50 ₽ left, card 3 pays 5% of 1 000 ₽, card 2 pays 1% of 2 000 ₽.
        let s = suggest_cards(
            &rules,
            &usage,
            july(),
            Some(GROCERIES),
            Some(rub(2000)),
            floor_rub,
        );
        let order: Vec<_> = s.iter().map(|c| (c.product_id, c.expected)).collect();
        assert_eq!(
            order,
            vec![
                (CARD, Some(rub(50))),
                (3, Some(rub(50))),
                (OTHER_CARD, Some(rub(20)))
            ]
        );

        // Card 1 used up: last, whatever its rate.
        let entries = [by_rule(1, CARD, Some(GROCERIES), rub(1000))];
        let usage = allocate(&entries, &rules, floor_rub).usage;
        let s = suggest_cards(&rules, &usage, july(), Some(GROCERIES), None, floor_rub);
        let order: Vec<_> = s.iter().map(|c| (c.product_id, c.exhausted)).collect();
        assert_eq!(order, vec![(3, false), (OTHER_CARD, false), (CARD, true)]);

        // No rules in another month: no suggestions.
        let august = "2026-08".parse().unwrap();
        assert!(suggest_cards(&rules, &usage, august, Some(GROCERIES), None, floor_rub).is_empty());
    }

    #[test]
    fn validation_and_wire_format() {
        assert_eq!(
            validate(&RuleInput::Rate(RateBp(10_001))),
            Err(RuleError::RateTooHigh(10_001))
        );
        assert_eq!(
            validate(&RuleInput::SpendCap(Some(-rub(1)))),
            Err(RuleError::NegativeLimit)
        );
        assert_eq!(validate(&RuleInput::CashbackCap(None)), Ok(()));
        let op: RuleOp = serde_json::from_str(
            r#"{"op":"edit","id":4,"edit":{"field":"spendCap","value":300000}}"#,
        )
        .unwrap();
        assert_eq!(
            op,
            RuleOp::Edit {
                id: 4,
                edit: RuleInput::SpendCap(Some(rub(3000)))
            }
        );
        let op: RuleOp = serde_json::from_str(r#"{"op":"copyFrom","from":"2026-06"}"#).unwrap();
        assert_eq!(
            op,
            RuleOp::CopyFrom {
                from: "2026-06".parse().unwrap()
            }
        );
    }
}
