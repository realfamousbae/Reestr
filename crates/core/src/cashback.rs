//! Cashback arithmetic (PLAN §6, D7, D13).
//!
//! Cashback of an entry is stored as an amount in kopecks; a rate is either the input it was
//! computed from or a value derived for display.

use serde::{Deserialize, Serialize};

use crate::money::Kopecks;

/// A rate in basis points: 1% = 100 bp, 100% = 10 000 bp.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct RateBp(pub u32);

impl RateBp {
    /// The largest rate accepted as input (100%).
    pub const MAX: Self = Self(10_000);
}

/// How a computed cashback amount is rounded. Configured per product (D13).
///
/// "Floor" rounds toward zero, like Excel `ROUNDDOWN`; for purchases (positive amounts) this
/// is the usual floor. "Half up" rounds half away from zero, like Excel `ROUND`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rounding {
    /// Down to whole rubles — what the source workbook does: `ROUNDDOWN(G*H;0)`.
    #[default]
    FloorRub,
    /// Down to kopecks.
    FloorKop,
    /// Half up to whole rubles.
    HalfUpRub,
    /// Half up to kopecks.
    HalfUpKop,
}

impl Rounding {
    pub const ALL: [Self; 4] = [
        Self::FloorRub,
        Self::FloorKop,
        Self::HalfUpRub,
        Self::HalfUpKop,
    ];

    /// The stored and serialized code, as in PLAN §5.
    pub const fn code(self) -> &'static str {
        match self {
            Self::FloorRub => "floor_rub",
            Self::FloorKop => "floor_kop",
            Self::HalfUpRub => "half_up_rub",
            Self::HalfUpKop => "half_up_kop",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.code() == code)
    }
}

impl CashbackSource {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::ManualPercent => "manual_percent",
            Self::ManualAmount => "manual_amount",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        [Self::Rule, Self::ManualPercent, Self::ManualAmount]
            .into_iter()
            .find(|s| s.code() == code)
    }
}

/// Where the cashback number of an entry came from (PLAN §5 `cashback_source`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CashbackSource {
    /// Computed from a cashback rule of the month (stage 2).
    Rule,
    /// The user typed a percentage; the amount is computed from it.
    ManualPercent,
    /// The user typed the amount directly.
    ManualAmount,
}

/// Cashback attached to an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cashback {
    pub amount: Kopecks,
    /// The rate the amount was computed from; `None` for [`CashbackSource::ManualAmount`].
    pub rate: Option<RateBp>,
    pub source: CashbackSource,
}

impl Cashback {
    /// Cashback computed from a typed percentage.
    pub fn from_percent(amount: Kopecks, rate: RateBp, rounding: Rounding) -> Self {
        Self {
            amount: cashback_for(amount, rate, rounding),
            rate: Some(rate),
            source: CashbackSource::ManualPercent,
        }
    }

    /// Cashback typed as an amount.
    pub fn manual_amount(cashback: Kopecks) -> Self {
        Self {
            amount: cashback,
            rate: None,
            source: CashbackSource::ManualAmount,
        }
    }

    /// Cashback that follows the month's rules; the amount comes from
    /// [`crate::rules::allocate`], which also drops it when no rule applies.
    pub fn pending_rule() -> Self {
        Self {
            amount: Kopecks::ZERO,
            rate: None,
            source: CashbackSource::Rule,
        }
    }

    /// The same cashback as a typed one, so a rule change no longer moves it (PLAN §6.3):
    /// a percentage when the full rate applied, an amount when a limit cut it.
    pub fn frozen(self) -> Self {
        match (self.source, self.rate) {
            (CashbackSource::Rule, Some(_)) => Self {
                source: CashbackSource::ManualPercent,
                ..self
            },
            (CashbackSource::Rule, None) => Self::manual_amount(self.amount),
            _ => self,
        }
    }

    /// Recomputes the amount for a new entry amount. Only a typed percentage changes here;
    /// a typed amount stays as typed (PLAN §6.3), and rule cashback depends on the whole
    /// month, see [`crate::rules::allocate`].
    pub fn recomputed(self, amount: Kopecks, rounding: Rounding) -> Self {
        match (self.source, self.rate) {
            (CashbackSource::ManualAmount | CashbackSource::Rule, _) | (_, None) => self,
            (_, Some(rate)) => Self {
                amount: cashback_for(amount, rate, rounding),
                ..self
            },
        }
    }

    /// The rate to show in the `%` column: the input rate, or one derived from the amounts.
    pub fn display_rate_bp(&self, amount: Kopecks) -> Option<i64> {
        match self.rate {
            Some(rate) if self.source != CashbackSource::ManualAmount => Some(i64::from(rate.0)),
            _ => effective_rate_bp(amount, self.amount),
        }
    }
}

/// Cashback for `amount` at `rate`, rounded as configured.
pub fn cashback_for(amount: Kopecks, rate: RateBp, rounding: Rounding) -> Kopecks {
    // Exact cashback in kopecks is amount * bp / 10_000; i128 cannot overflow here.
    let numerator = i128::from(amount.get()) * i128::from(rate.0);
    let kopecks = match rounding {
        Rounding::FloorKop => numerator / 10_000,
        Rounding::HalfUpKop => div_half_away(numerator, 10_000),
        Rounding::FloorRub => numerator / 1_000_000 * 100,
        Rounding::HalfUpRub => div_half_away(numerator, 1_000_000) * 100,
    };
    Kopecks(saturate_i64(kopecks))
}

/// The rate `cashback / amount` in basis points, rounded half away from zero.
/// `None` when the amount is zero.
pub fn effective_rate_bp(amount: Kopecks, cashback: Kopecks) -> Option<i64> {
    if amount.is_zero() {
        return None;
    }
    let numerator = i128::from(cashback.get()) * 10_000;
    let denominator = i128::from(amount.get());
    let sign = numerator.signum() * denominator.signum();
    let bp = div_half_away(numerator.abs(), denominator.abs()) * sign;
    Some(saturate_i64(bp))
}

/// Integer division rounding half away from zero. `denominator` must be positive.
pub(crate) fn div_half_away(numerator: i128, denominator: i128) -> i128 {
    debug_assert!(denominator > 0);
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    if remainder.abs() * 2 >= denominator {
        quotient + numerator.signum()
    } else {
        quotient
    }
}

pub(crate) fn saturate_i64(value: i128) -> i64 {
    i64::try_from(value).unwrap_or(if value < 0 { i64::MIN } else { i64::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rub(r: i64, k: i64) -> Kopecks {
        Kopecks(r * 100 + k)
    }

    #[test]
    fn floor_rub_matches_excel_rounddown() {
        // 2 234,00 ₽ × 6% = 134,04 ₽ → 134 ₽ (the "6% → 134 ₽" case of the source workbook).
        assert_eq!(
            cashback_for(rub(2234, 0), RateBp(600), Rounding::FloorRub),
            rub(134, 0)
        );
        // 999,99 ₽ × 5% = 49,9995 ₽ → 49 ₽.
        assert_eq!(
            cashback_for(rub(999, 99), RateBp(500), Rounding::FloorRub),
            rub(49, 0)
        );
    }

    #[test]
    fn each_rounding_mode() {
        // 1 234,50 ₽ × 3,5% = 43,2075 ₽.
        let amount = rub(1234, 50);
        let rate = RateBp(350);
        assert_eq!(cashback_for(amount, rate, Rounding::FloorRub), rub(43, 0));
        assert_eq!(cashback_for(amount, rate, Rounding::FloorKop), rub(43, 20));
        assert_eq!(cashback_for(amount, rate, Rounding::HalfUpRub), rub(43, 0));
        assert_eq!(cashback_for(amount, rate, Rounding::HalfUpKop), rub(43, 21));
    }

    #[test]
    fn half_up_on_exact_half() {
        // 1 350,00 ₽ × 1% = 13,50 ₽.
        assert_eq!(
            cashback_for(rub(1350, 0), RateBp(100), Rounding::HalfUpRub),
            rub(14, 0)
        );
        // 0,50 ₽ × 1% = 0,005 ₽ → 0,01 ₽.
        assert_eq!(
            cashback_for(rub(0, 50), RateBp(100), Rounding::HalfUpKop),
            rub(0, 1)
        );
        assert_eq!(
            cashback_for(rub(0, 50), RateBp(100), Rounding::FloorKop),
            rub(0, 0)
        );
    }

    #[test]
    fn zero_rate_zero_amount_and_full_rate() {
        assert_eq!(
            cashback_for(rub(500, 0), RateBp(0), Rounding::HalfUpKop),
            Kopecks::ZERO
        );
        assert_eq!(
            cashback_for(Kopecks::ZERO, RateBp(500), Rounding::FloorRub),
            Kopecks::ZERO
        );
        assert_eq!(
            cashback_for(rub(12, 34), RateBp::MAX, Rounding::FloorKop),
            rub(12, 34)
        );
    }

    #[test]
    fn refunds_round_toward_zero() {
        // A refund of 2 234,00 ₽ at 6% takes back 134 ₽, not 135 ₽.
        assert_eq!(
            cashback_for(-rub(2234, 0), RateBp(600), Rounding::FloorRub),
            -rub(134, 0)
        );
        assert_eq!(
            cashback_for(-rub(1350, 0), RateBp(100), Rounding::HalfUpRub),
            -rub(14, 0)
        );
    }

    #[test]
    fn effective_rate() {
        // Certificate 666 ₽ → 200 ₽ of cashback = 30,03%.
        assert_eq!(effective_rate_bp(rub(666, 0), rub(200, 0)), Some(3003));
        assert_eq!(effective_rate_bp(rub(2234, 0), rub(134, 0)), Some(600));
        assert_eq!(effective_rate_bp(Kopecks::ZERO, rub(1, 0)), None);
        assert_eq!(effective_rate_bp(-rub(100, 0), -rub(5, 0)), Some(500));
    }

    #[test]
    fn recompute_keeps_manual_amount() {
        let typed = Cashback::manual_amount(rub(200, 0));
        assert_eq!(typed.recomputed(rub(1000, 0), Rounding::FloorRub), typed);
        assert_eq!(typed.display_rate_bp(rub(666, 0)), Some(3003));

        let by_rate = Cashback::from_percent(rub(1000, 0), RateBp(500), Rounding::FloorRub);
        assert_eq!(by_rate.amount, rub(50, 0));
        let changed = by_rate.recomputed(rub(2000, 0), Rounding::FloorRub);
        assert_eq!(changed.amount, rub(100, 0));
        assert_eq!(changed.display_rate_bp(rub(2000, 0)), Some(500));
    }

    #[test]
    fn frozen_rule_cashback_becomes_typed() {
        let full = Cashback {
            amount: rub(50, 0),
            rate: Some(RateBp(500)),
            source: CashbackSource::Rule,
        };
        assert_eq!(
            full.frozen(),
            Cashback {
                source: CashbackSource::ManualPercent,
                ..full
            }
        );
        let capped = Cashback { rate: None, ..full };
        assert_eq!(capped.frozen(), Cashback::manual_amount(rub(50, 0)));
        let typed = Cashback::manual_amount(rub(7, 0));
        assert_eq!(typed.frozen(), typed);
        // Rule cashback is not recomputed one entry at a time.
        assert_eq!(full.recomputed(rub(5000, 0), Rounding::FloorRub), full);
    }

    #[test]
    fn rounding_serializes_as_plan_names() {
        let json = serde_json::to_string(&Rounding::HalfUpKop).unwrap();
        assert_eq!(json, "\"half_up_kop\"");
        assert_eq!(Rounding::default(), Rounding::FloorRub);
        for r in Rounding::ALL {
            assert_eq!(
                serde_json::to_string(&r).unwrap(),
                format!("\"{}\"", r.code())
            );
            assert_eq!(Rounding::from_code(r.code()), Some(r));
        }
        for s in [
            CashbackSource::Rule,
            CashbackSource::ManualPercent,
            CashbackSource::ManualAmount,
        ] {
            assert_eq!(
                serde_json::to_string(&s).unwrap(),
                format!("\"{}\"", s.code())
            );
            assert_eq!(CashbackSource::from_code(s.code()), Some(s));
        }
    }
}
