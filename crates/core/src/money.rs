//! Money as integer kopecks.

use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// An amount of money in kopecks (1/100 of a ruble).
///
/// Serialized as a plain JSON integer. `i64` kopecks cover ±92 quadrillion rubles, and every
/// realistic value stays below 2^53, so it is exact as a JavaScript number too.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Kopecks(pub i64);

impl Kopecks {
    pub const ZERO: Self = Self(0);

    /// Builds an amount from whole rubles.
    pub const fn from_rubles(rubles: i64) -> Self {
        Self(rubles * 100)
    }

    pub const fn get(self) -> i64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl Add for Kopecks {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for Kopecks {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for Kopecks {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for Kopecks {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Neg for Kopecks {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Sum for Kopecks {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, Add::add)
    }
}

impl<'a> Sum<&'a Kopecks> for Kopecks {
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        iter.copied().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_and_sum() {
        let a = Kopecks(150);
        let b = Kopecks::from_rubles(2);
        assert_eq!(a + b, Kopecks(350));
        assert_eq!(b - a, Kopecks(50));
        assert_eq!(-a, Kopecks(-150));
        assert_eq!([a, b, Kopecks(-1)].iter().sum::<Kopecks>(), Kopecks(349));
    }

    #[test]
    fn serializes_as_plain_integer() {
        assert_eq!(serde_json::to_string(&Kopecks(123_456)).unwrap(), "123456");
        assert_eq!(serde_json::from_str::<Kopecks>("-5").unwrap(), Kopecks(-5));
    }
}
