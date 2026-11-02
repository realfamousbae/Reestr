//! «Калькулятор»: is it worth paying by a cashback card now, or keeping the money on a savings
//! account for the term? (PLAN §3, after the MVP.)
//!
//! Pure arithmetic on integer kopecks and basis points; nothing is stored. Simple interest only:
//! no compounding and no tax.
//!
//! Rounding, both ends fixed so the screen is reproducible:
//! - cashback: [`Rounding::default()`] (whole rubles, down), what the cashback rules of a new
//!   card use and what the source workbook does, via [`cashback_for`];
//! - interest: half away from zero to the kopeck, like a bank statement.
//!
//! Overflow: the arithmetic runs in `i128` and the input is bounded ([`MAX_AMOUNT`],
//! [`RateBp::MAX`], [`MAX_DAYS`]), so the largest intermediate product is about 10^14 × 10^4 ×
//! 3650 ≈ 4·10^21, far below `i128::MAX` (≈ 1.7·10^38), and every result fits `i64` — in fact
//! stays below 2^53, so it is exact as a JavaScript number too.

use serde::{Deserialize, Serialize};

use crate::cashback::{RateBp, Rounding, cashback_for, div_half_away, saturate_i64};
use crate::money::Kopecks;

/// The longest term accepted: ten years of 365 days.
pub const MAX_DAYS: u32 = 3650;

/// The largest amount accepted: 10^14 kopecks (a trillion rubles). Keeps every result below 2^53.
pub const MAX_AMOUNT: Kopecks = Kopecks(100_000_000_000_000);

/// Days in a year for the interest: the actual/365 convention of savings accounts.
const DAYS_IN_YEAR: i128 = 365;

/// What the person types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalcInput {
    /// The purchase, equally the sum that would stay on the savings account.
    pub amount: Kopecks,
    /// The cashback of the card.
    pub cashback_rate: RateBp,
    /// The yearly rate of the savings account.
    pub savings_rate: RateBp,
    /// For how many days the money would stay on the account.
    pub days: u32,
}

/// Which option brings more money.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Better {
    /// Pay by the card: the cashback is larger.
    Cashback,
    /// Keep the money on the account: the interest is larger.
    Savings,
    /// Both give the same.
    Equal,
}

/// The comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalcResult {
    pub cashback: Kopecks,
    pub interest: Kopecks,
    /// `cashback − interest`: positive when the card wins.
    pub difference: Kopecks,
    /// The yearly savings rate (basis points, rounded half away from zero) at which the
    /// interest for the term equals the cashback. It may exceed 100% for a short term.
    /// `None` when it is undefined: zero days or a zero amount.
    pub break_even_bp: Option<i64>,
    pub better: Better,
}

/// An input the calculator does not accept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CalcError {
    #[error("the amount is negative")]
    NegativeAmount,
    #[error("the amount is above the limit of {} kopecks", MAX_AMOUNT.get())]
    AmountTooLarge,
    #[error("a rate is above 100% ({0} bp)")]
    RateTooHigh(u32),
    #[error("the term is above {MAX_DAYS} days ({0})")]
    TooManyDays(u32),
}

/// Simple interest for `days` at a yearly `rate`: `amount × rate_bp × days / (10 000 × 365)`,
/// rounded half away from zero to the kopeck.
pub fn interest_for(amount: Kopecks, rate: RateBp, days: u32) -> Kopecks {
    let numerator = i128::from(amount.get()) * i128::from(rate.0) * i128::from(days);
    Kopecks(saturate_i64(div_half_away(
        numerator,
        10_000 * DAYS_IN_YEAR,
    )))
}

/// The yearly rate in basis points at which the interest of `amount` over `days` equals
/// `cashback`, rounded half away from zero. `None` for zero days or a zero amount.
fn break_even(amount: Kopecks, cashback: Kopecks, days: u32) -> Option<i64> {
    if amount.is_zero() || days == 0 {
        return None;
    }
    let numerator = i128::from(cashback.get()) * 10_000 * DAYS_IN_YEAR;
    let denominator = i128::from(amount.get()) * i128::from(days);
    // `amount` is not negative here (validated) and not zero, so the denominator is positive.
    Some(saturate_i64(div_half_away(numerator, denominator)))
}

/// Compares the cashback with the interest for the term.
pub fn compare(input: CalcInput) -> Result<CalcResult, CalcError> {
    if input.amount.get() < 0 {
        return Err(CalcError::NegativeAmount);
    }
    if input.amount > MAX_AMOUNT {
        return Err(CalcError::AmountTooLarge);
    }
    for rate in [input.cashback_rate, input.savings_rate] {
        if rate > RateBp::MAX {
            return Err(CalcError::RateTooHigh(rate.0));
        }
    }
    if input.days > MAX_DAYS {
        return Err(CalcError::TooManyDays(input.days));
    }
    let cashback = cashback_for(input.amount, input.cashback_rate, Rounding::default());
    let interest = interest_for(input.amount, input.savings_rate, input.days);
    let better = match cashback.cmp(&interest) {
        std::cmp::Ordering::Greater => Better::Cashback,
        std::cmp::Ordering::Less => Better::Savings,
        std::cmp::Ordering::Equal => Better::Equal,
    };
    Ok(CalcResult {
        cashback,
        interest,
        difference: cashback - interest,
        break_even_bp: break_even(input.amount, cashback, input.days),
        better,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rub(r: i64) -> Kopecks {
        Kopecks(r * 100)
    }

    fn input(amount: Kopecks, cashback_bp: u32, savings_bp: u32, days: u32) -> CalcInput {
        CalcInput {
            amount,
            cashback_rate: RateBp(cashback_bp),
            savings_rate: RateBp(savings_bp),
            days,
        }
    }

    #[test]
    fn card_wins_on_a_short_term() {
        // 100 000 ₽, 5% cashback = 5 000 ₽; 16% yearly for 30 days = 1 315,07 ₽ (1 315,0684…).
        let r = compare(input(rub(100_000), 500, 1600, 30)).unwrap();
        assert_eq!(r.cashback, rub(5000));
        assert_eq!(r.interest, Kopecks(131_507));
        assert_eq!(r.difference, Kopecks(368_493));
        assert_eq!(r.better, Better::Cashback);
        // 500 000 kop × 10 000 × 365 / (10 000 000 × 30) = 6083,33 bp.
        assert_eq!(r.break_even_bp, Some(6083));
    }

    #[test]
    fn savings_win_on_a_long_term() {
        // 100 000 ₽, 1% cashback = 1 000 ₽; 16% for 365 days = 16 000 ₽.
        let r = compare(input(rub(100_000), 100, 1600, 365)).unwrap();
        assert_eq!(r.cashback, rub(1000));
        assert_eq!(r.interest, rub(16_000));
        assert_eq!(r.difference, -rub(15_000));
        assert_eq!(r.better, Better::Savings);
        assert_eq!(r.break_even_bp, Some(100));
    }

    #[test]
    fn equal_results() {
        // 10% for 365 days is 10% of the amount: same as a 10% cashback.
        let r = compare(input(rub(1000), 1000, 1000, 365)).unwrap();
        assert_eq!(r.cashback, rub(100));
        assert_eq!(r.interest, rub(100));
        assert_eq!(r.difference, Kopecks::ZERO);
        assert_eq!(r.better, Better::Equal);
        assert_eq!(r.break_even_bp, Some(1000));
    }

    #[test]
    fn cashback_rounds_down_to_rubles_and_interest_half_up_to_kopecks() {
        // 2 234 ₽ × 6% = 134,04 ₽ → 134 ₽ (the default rounding of the cashback rules).
        let r = compare(input(rub(2234), 600, 0, 10)).unwrap();
        assert_eq!(r.cashback, rub(134));
        // 1 000 ₽ at 12,5% for 1 day: 100 000 × 1250 / 3 650 000 = 34,246… kop → 34.
        assert_eq!(interest_for(rub(1000), RateBp(1250), 1), Kopecks(34));
        // 100 kop at 1% for 73 days is 0,2 kop → 0; at 10% it is exactly 2 kop.
        assert_eq!(interest_for(Kopecks(100), RateBp(100), 73), Kopecks(0));
        assert_eq!(interest_for(Kopecks(100), RateBp(1000), 73), Kopecks(2));
        // Exactly half a kopeck rounds up: 1 000 kop at 18,25% for 1 day is 0,5 kop → 1 kop.
        assert_eq!(interest_for(Kopecks(1000), RateBp(1825), 1), Kopecks(1));
    }

    #[test]
    fn zero_days() {
        let r = compare(input(rub(50_000), 300, 1800, 0)).unwrap();
        assert_eq!(r.cashback, rub(1500));
        assert_eq!(r.interest, Kopecks::ZERO);
        assert_eq!(r.better, Better::Cashback);
        // No term: the break-even rate is undefined, not infinite.
        assert_eq!(r.break_even_bp, None);
    }

    #[test]
    fn zero_days_and_zero_cashback_is_a_tie() {
        let r = compare(input(rub(50_000), 0, 1800, 0)).unwrap();
        assert_eq!(r.better, Better::Equal);
        assert_eq!(r.break_even_bp, None);
    }

    #[test]
    fn zero_rates() {
        // No cashback: any positive savings rate wins, and the break-even rate is 0.
        let r = compare(input(rub(1000), 0, 500, 30)).unwrap();
        assert_eq!(r.cashback, Kopecks::ZERO);
        assert!(r.interest > Kopecks::ZERO);
        assert_eq!(r.better, Better::Savings);
        assert_eq!(r.break_even_bp, Some(0));
        // No interest: the card wins.
        let r = compare(input(rub(1000), 500, 0, 30)).unwrap();
        assert_eq!(r.interest, Kopecks::ZERO);
        assert_eq!(r.better, Better::Cashback);
        // Both zero: a tie.
        let r = compare(input(rub(1000), 0, 0, 30)).unwrap();
        assert_eq!(r.better, Better::Equal);
        assert_eq!(r.break_even_bp, Some(0));
    }

    #[test]
    fn zero_amount() {
        let r = compare(input(Kopecks::ZERO, 500, 1600, 30)).unwrap();
        assert_eq!(r.cashback, Kopecks::ZERO);
        assert_eq!(r.interest, Kopecks::ZERO);
        assert_eq!(r.better, Better::Equal);
        assert_eq!(r.break_even_bp, None);
    }

    #[test]
    fn break_even_makes_the_interest_match_the_cashback() {
        for (amount, cashback_bp, days) in [
            (rub(100_000), 500, 30),
            (rub(2234), 600, 10),
            (rub(75_000), 150, 45),
            (rub(1_000_000), 300, 365),
            (rub(5), 1000, 1),
        ] {
            let r = compare(input(amount, cashback_bp, 0, days)).unwrap();
            let bp = r.break_even_bp.unwrap();
            // Re-run with the break-even rate (it may exceed 100% for tiny terms: use the
            // helper directly then): the interest is within one basis point's worth of money.
            let interest = interest_for_bp(amount, bp, days);
            let one_bp = interest_for_bp(amount, 1, days).get().max(1);
            assert!(
                (interest - r.cashback).get().abs() <= one_bp,
                "{amount:?} {cashback_bp} {days}: interest {interest:?} vs cashback {:?}",
                r.cashback
            );
        }
    }

    /// `interest_for` without the 100% bound, for rates above it.
    fn interest_for_bp(amount: Kopecks, bp: i64, days: u32) -> Kopecks {
        let n = i128::from(amount.get()) * i128::from(bp) * i128::from(days);
        Kopecks(saturate_i64(div_half_away(n, 10_000 * DAYS_IN_YEAR)))
    }

    #[test]
    fn break_even_above_100_percent_for_one_day() {
        // 1% cashback for one day equals 365% yearly.
        let r = compare(input(rub(10_000), 100, 0, 1)).unwrap();
        assert_eq!(r.break_even_bp, Some(36_500));
    }

    #[test]
    fn extreme_values_do_not_overflow() {
        let r = compare(input(MAX_AMOUNT, 10_000, 10_000, MAX_DAYS)).unwrap();
        assert_eq!(r.cashback, MAX_AMOUNT);
        // 100% for ten years of simple interest = ten times the amount.
        assert_eq!(r.interest, Kopecks(MAX_AMOUNT.get() * 10));
        assert!(r.interest.get() < (1_i64 << 53));
        assert_eq!(r.better, Better::Savings);
        assert_eq!(r.break_even_bp, Some(1000));
        // Smallest values.
        let r = compare(input(Kopecks(1), 1, 1, 1)).unwrap();
        assert_eq!(r.cashback, Kopecks::ZERO);
        assert_eq!(r.interest, Kopecks::ZERO);
    }

    #[test]
    fn rejects_input_out_of_range() {
        assert_eq!(
            compare(input(Kopecks(-1), 100, 100, 1)),
            Err(CalcError::NegativeAmount)
        );
        assert_eq!(
            compare(input(Kopecks(MAX_AMOUNT.get() + 1), 100, 100, 1)),
            Err(CalcError::AmountTooLarge)
        );
        assert_eq!(
            compare(input(rub(1), 10_001, 100, 1)),
            Err(CalcError::RateTooHigh(10_001))
        );
        assert_eq!(
            compare(input(rub(1), 100, 10_001, 1)),
            Err(CalcError::RateTooHigh(10_001))
        );
        assert_eq!(
            compare(input(rub(1), 100, 100, MAX_DAYS + 1)),
            Err(CalcError::TooManyDays(MAX_DAYS + 1))
        );
        assert!(compare(input(rub(1), 10_000, 10_000, MAX_DAYS)).is_ok());
    }

    #[test]
    fn serializes_for_the_ui() {
        assert_eq!(
            serde_json::to_string(&Better::Savings).unwrap(),
            "\"savings\""
        );
    }
}
