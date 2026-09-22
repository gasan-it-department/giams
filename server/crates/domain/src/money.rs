//! Peso amounts.
//!
//! The project design (§4.1) requires deduction and net amounts to be computed
//! "on the server using exact decimal arithmetic to guarantee centavo
//! accuracy". This module is how that promise is kept.
//!
//! [`Money`] is a newtype over [`Decimal`] rather than a type alias, so that:
//!
//!   * there is no way to accidentally pass a bare `f64` where an amount is
//!     expected -- floating point never enters the system at all;
//!   * amounts serialise as JSON **strings**, so the TypeScript client
//!     receives `"184320.00"` and not a number it might do arithmetic on;
//!   * rounding happens in exactly one place, documented, half-up to the
//!     centavo, matching Philippine accounting practice.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Decimal places a peso amount carries. Two — pesos and centavos.
///
/// The Accounting and Treasury offices work to the centavo, and every figure
/// they check by hand is checked at that precision. A `Money` value is
/// therefore *always* exactly two decimal places: there is no way to construct
/// one carrying a hidden third digit that would make a column of figures sum
/// to something other than the total shown beside it.
///
/// Intermediate precision still exists where it is needed — [`Money::apply_rate`]
/// multiplies in full [`Decimal`] precision and rounds once at the end — but it
/// never survives into a stored or displayed value.
pub const SCALE: u32 = 2;

/// The one rounding rule for money in this system.
///
/// Two decimal places, **half away from zero**: `.005` and above goes up.
/// That is what the Accounting Office does, and what a clerk checking a figure
/// on a calculator does.
///
/// Banker's rounding — the default in many libraries, including
/// `Decimal::round_dp` — is deliberately *not* used. It would round 0.005 to
/// 0.00 and 0.015 to 0.02, which is defensible statistically and indefensible
/// when someone is holding a voucher that says otherwise.
///
/// Because the input is an exact base-10 `Decimal`, a value like `2.675` is
/// genuinely 2.675 and rounds to `2.68`. The same computation in `f64` holds
/// 2.67499999999999982236431605997495353221893310546875 and rounds to `2.67`.
fn round_half_up(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(SCALE, RoundingStrategy::MidpointAwayFromZero)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct Money(Decimal);

impl Money {
    pub const ZERO: Money = Money(Decimal::ZERO);

    /// Builds an amount, rounding to the centavo.
    ///
    /// Rounding here rather than only on display is deliberate: two `Money`
    /// values that look identical on screen must also compare and sum
    /// identically.
    pub fn new(value: Decimal) -> Self {
        Money(round_half_up(value))
    }

    /// Builds an amount from whole centavos. The safest constructor, and the
    /// one to prefer when reading from an external system.
    pub fn from_centavos(centavos: i64) -> Self {
        Money(Decimal::new(centavos, 2))
    }

    pub fn inner(self) -> Decimal {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.is_zero()
    }

    pub fn is_negative(self) -> bool {
        self.0.is_sign_negative() && !self.0.is_zero()
    }

    pub fn abs(self) -> Self {
        Money(self.0.abs())
    }

    /// Already at centavo precision by construction. Retained so call sites
    /// that want to state the intent explicitly can.
    pub fn round_to_centavo(self) -> Self {
        Money(round_half_up(self.0))
    }

    /// Applies a rate (e.g. 0.02 for 2% expanded withholding tax).
    ///
    /// The multiplication happens in full [`Decimal`] precision and is rounded
    /// once, at the end. Rounding the operands first would compound the error
    /// across a voucher with several deductions.
    pub fn apply_rate(self, rate: Decimal) -> Self {
        Money(round_half_up(self.0 * rate))
    }

    /// Splits an amount into `parts` shares that sum back to exactly the
    /// original.
    ///
    /// Naive division leaves a remainder -- ₱100.00 across 3 lines gives
    /// ₱33.33 each and loses a centavo. The remainder is distributed one
    /// centavo at a time across the leading shares, so the total is always
    /// preserved. Losing that centavo is what makes a journal entry refuse to
    /// balance at the end of a long voucher.
    pub fn allocate(self, parts: usize) -> Vec<Money> {
        if parts == 0 {
            return Vec::new();
        }

        let rounded = self.round_to_centavo();
        let divisor = Decimal::from(parts as i64);
        let base = Money(rounded.0 / divisor).round_to_centavo();

        let mut shares = vec![base; parts];
        let distributed = Money(base.0 * divisor);
        let mut remainder = rounded.0 - distributed.0;

        // One centavo at a time, in whichever direction the remainder points.
        let centavo = Decimal::new(1, 2);
        let step = if remainder.is_sign_negative() { -centavo } else { centavo };

        let mut i = 0;
        while !remainder.is_zero() && i < parts {
            shares[i] = Money(shares[i].0 + step);
            remainder -= step;
            i += 1;
        }

        shares
    }

    /// Renders for display: thousands separated, always two decimals,
    /// negatives in parentheses per accounting convention.
    pub fn to_display(self) -> String {
        let rounded = self.round_to_centavo();
        let negative = rounded.is_negative();
        let digits = format!("{:.2}", rounded.0.abs());

        let (whole, frac) = digits.split_once('.').unwrap_or((digits.as_str(), "00"));
        let mut grouped = String::new();
        for (i, c) in whole.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(c);
        }
        let whole: String = grouped.chars().rev().collect();

        if negative {
            format!("({whole}.{frac})")
        } else {
            format!("{whole}.{frac}")
        }
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.round_to_centavo().0)
    }
}

impl Add for Money {
    type Output = Money;
    fn add(self, rhs: Money) -> Money {
        Money(self.0 + rhs.0)
    }
}

impl Sub for Money {
    type Output = Money;
    fn sub(self, rhs: Money) -> Money {
        Money(self.0 - rhs.0)
    }
}

impl Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        Money(-self.0)
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Money) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Money {
    fn sub_assign(&mut self, rhs: Money) {
        self.0 -= rhs.0;
    }
}

impl Sum for Money {
    fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
        iter.fold(Money::ZERO, |a, b| a + b)
    }
}

// ── Serialisation ────────────────────────────────────────────────────────────
// Always a string on the wire. A JSON number would be parsed into a JavaScript
// double by the client, and 0.1 + 0.2 is not 0.3 there either.

impl Serialize for Money {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // Always exactly two decimal places. `Decimal::to_string` preserves
        // whatever scale the value happens to carry, so a zero read from the
        // database would go out as "0" while a computed amount went out as
        // "0.00". Two representations of the same amount is the kind of
        // inconsistency that eventually gets "fixed" by parsing on the client.
        s.serialize_str(&format!("{:.2}", self.round_to_centavo().0))
    }
}

impl<'de> Deserialize<'de> for Money {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Money, D::Error> {
        use serde::de::Error;
        let raw = String::deserialize(d)?;
        Decimal::from_str_exact(raw.trim())
            .map(Money::new)
            .map_err(|_| D::Error::custom(format!("`{raw}` is not a valid amount")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn allocation_never_loses_a_centavo() {
        // The classic case: 100.00 over 3 ways.
        let shares = Money::from_centavos(10_000).allocate(3);
        assert_eq!(shares.len(), 3);
        assert_eq!(
            shares.iter().copied().sum::<Money>(),
            Money::from_centavos(10_000)
        );
        assert_eq!(shares[0].to_display(), "33.34");
        assert_eq!(shares[1].to_display(), "33.33");
        assert_eq!(shares[2].to_display(), "33.33");
    }

    #[test]
    fn allocation_preserves_total_for_many_awkward_splits() {
        for total in [1i64, 7, 99, 100, 12_345, 1_000_001] {
            for parts in 1..=13usize {
                let amount = Money::from_centavos(total);
                let sum: Money = amount.allocate(parts).into_iter().sum();
                assert_eq!(sum, amount, "total {total} over {parts} parts");
            }
        }
    }

    #[test]
    fn rounding_is_half_up_not_bankers() {
        // Banker's rounding would give 0.02 here; an auditor's calculator gives 0.03.
        assert_eq!(Money::new(dec!(0.025)).to_display(), "0.03");
        assert_eq!(Money::new(dec!(0.035)).to_display(), "0.04");
    }

    #[test]
    fn a_third_decimal_of_five_or_more_always_rounds_up() {
        // The rule as the Accounting Office states it: .005 and above goes up.
        for (input, expected) in [
            (dec!(1.004), "1.00"),
            (dec!(1.005), "1.01"),
            (dec!(1.006), "1.01"),
            (dec!(2.675), "2.68"), // f64 gets this wrong — it holds 2.67499999…
            (dec!(0.125), "0.13"),
            (dec!(0.135), "0.14"), // banker's would give 0.14 and 0.12 respectively
            (dec!(1234.565), "1,234.57"),
        ] {
            assert_eq!(Money::new(input).to_display(), expected, "rounding {input}");
        }
    }

    #[test]
    fn negative_amounts_round_by_magnitude() {
        // Away from zero, so a refund of 1.005 is 1.01 whichever side it is on.
        assert_eq!(Money::new(dec!(-1.005)).to_display(), "(1.01)");
        assert_eq!(Money::new(dec!(-1.004)).to_display(), "(1.00)");
    }

    #[test]
    fn money_can_never_hold_a_third_decimal() {
        // The property the storage change depends on: what is summed is what
        // is shown. A hidden third digit is what makes a column of figures add
        // up to something other than its own total.
        for raw in [dec!(3686.4050), dec!(0.001), dec!(99.999), dec!(1.4999)] {
            let m = Money::new(raw);
            assert!(
                m.inner().scale() <= 2,
                "{raw} produced a Money with scale {}",
                m.inner().scale()
            );
            // And it survives a round trip through the wire format unchanged.
            let json = serde_json::to_string(&m).unwrap();
            let back: Money = serde_json::from_str(&json).unwrap();
            assert_eq!(back, m);
        }
    }

    #[test]
    fn a_rate_is_applied_at_full_precision_then_rounded_once() {
        // 1234.56 × 2% = 24.6912, which must become 24.69 and not 24.70 via a
        // rounded intermediate.
        let gross = Money::from_centavos(123_456);
        assert_eq!(gross.apply_rate(dec!(0.02)).to_display(), "24.69");

        // And the boundary: 1000.25 × 1% = 10.0025 → 10.00, while
        // 1000.50 × 1% = 10.005 → 10.01.
        assert_eq!(Money::from_centavos(100_025).apply_rate(dec!(0.01)).to_display(), "10.00");
        assert_eq!(Money::from_centavos(100_050).apply_rate(dec!(0.01)).to_display(), "10.01");
    }

    #[test]
    fn withholding_tax_matches_manual_computation() {
        // 2% EWT on 184,320.00
        let gross = Money::from_centavos(18_432_000);
        let ewt = gross.apply_rate(dec!(0.02));
        assert_eq!(ewt.to_display(), "3,686.40");
        assert_eq!((gross - ewt).to_display(), "180,633.60");
    }

    #[test]
    fn negatives_display_in_parentheses() {
        assert_eq!(Money::from_centavos(-125_000).to_display(), "(1,250.00)");
    }

    #[test]
    fn serialises_as_a_string() {
        let json = serde_json::to_string(&Money::from_centavos(18_432_000)).unwrap();
        assert_eq!(json, "\"184320.00\"");
    }

    #[test]
    fn serialisation_always_carries_two_decimals() {
        // Whatever scale the value arrived with, the wire format is uniform.
        for (input, expected) in [
            (Decimal::ZERO, "\"0.00\""),
            (dec!(0.0000), "\"0.00\""),
            (dec!(5), "\"5.00\""),
            (dec!(5.1), "\"5.10\""),
            (dec!(-1250), "\"-1250.00\""),
            (dec!(1234.5678), "\"1234.57\""),
        ] {
            assert_eq!(
                serde_json::to_string(&Money::new(input)).unwrap(),
                expected,
                "serialising {input}"
            );
        }
    }

    #[test]
    fn a_serialised_amount_round_trips() {
        let original = Money::from_centavos(87_641_225);
        let json = serde_json::to_string(&original).unwrap();
        let back: Money = serde_json::from_str(&json).unwrap();
        assert_eq!(back, original);
    }
}
