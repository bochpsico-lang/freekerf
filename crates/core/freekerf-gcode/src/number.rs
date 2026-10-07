//! Numeric helpers that reproduce the conversions of the original C# core.
//!
//! LaserGRBL keeps coordinates in `System.Decimal` and does geometry in `double`.
//! FreeKerf uses [`Decimal`] (ADR 0002) and converts between the two exactly where
//! LaserGRBL does, so the analysis (time, ranges) is identical to the last digit.

use std::str::FromStr;

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

/// Parses a G-code number the way `decimal.Parse(s, NumberStyles.Number, Invariant)`
/// does for the characters the tokenizer accumulates (digits, `.` and `-`).
///
/// Accepted: an optional leading **or** trailing `-`, ASCII digits with at most one
/// decimal point and at least one digit (`5`, `-5`, `5-`, `.5`, `5.`).
/// Everything else (`""`, `-`, `1..5`, `-4-2`, non-ASCII digits) is rejected.
#[must_use]
pub fn parse_number(s: &str) -> Option<Decimal> {
    let (negative, body) = if let Some(rest) = s.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = s.strip_suffix('-') {
        (true, rest)
    } else {
        (false, s)
    };
    let mut digits = 0usize;
    let mut points = 0usize;
    for c in body.chars() {
        match c {
            '0'..='9' => digits += 1,
            '.' => points += 1,
            _ => return None,
        }
    }
    if digits == 0 || points > 1 {
        return None;
    }
    let mut text = String::with_capacity(body.len() + 2);
    if body.starts_with('.') {
        text.push('0');
    }
    text.push_str(body.strip_suffix('.').unwrap_or(body));
    let value = Decimal::from_str(&text).ok()?;
    Some(if negative && !value.is_zero() { -value } else { value })
}

/// `(decimal)double` of .NET Framework: at most 15 significant digits, `None` for
/// NaN, infinities and values out of the decimal range (an `OverflowException` there).
#[must_use]
pub fn decimal_from_f64(value: f64) -> Option<Decimal> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some(Decimal::ZERO);
    }
    // shortest decimal with 15 significant digits, then trailing zeros dropped
    let text = format!("{value:.14e}");
    let (mantissa, exponent) = text.split_once('e')?;
    let exponent: i32 = exponent.parse().ok()?;
    let negative = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    // value = 0.d1d2..d15 * 10^(exponent + 1)
    let scale = 14 - exponent;
    let mut number = Decimal::from_str(&digits).ok()?;
    if scale >= 0 {
        if scale > 28 {
            // smaller than the smallest decimal step: rounds to zero (or to fewer digits)
            let shift = u32::try_from(scale - 28).ok()?;
            number = number.checked_div(Decimal::from(10u64.checked_pow(shift.min(19))?))?.round_dp(0);
            number.set_scale(28).ok()?;
        } else {
            number.set_scale(u32::try_from(scale).ok()?).ok()?;
        }
    } else {
        let shift = u32::try_from(-scale).ok()?;
        for _ in 0..shift {
            number = number.checked_mul(Decimal::TEN)?;
        }
    }
    let number = number.normalize();
    Some(if negative { -number } else { number })
}

/// `(double)decimal` (lossy, as in .NET).
#[must_use]
pub fn f64_from_decimal(value: Decimal) -> f64 {
    value.to_f64().unwrap_or(0.0)
}

/// `Math.Round(value, digits)` of .NET Framework (round half to even on the scaled value).
#[must_use]
pub fn round_half_even(value: f64, digits: i32) -> f64 {
    const LIMIT: f64 = 1e16;
    if value.abs() < LIMIT {
        let power = 10f64.powi(digits);
        (value * power).round_ties_even() / power
    } else {
        value
    }
}

/// `TimeSpan.FromMinutes` / `FromSeconds` of .NET Framework: the result is rounded to
/// whole milliseconds (half away from zero). `None` for NaN or overflow.
#[must_use]
pub fn millis_from(value: f64, scale_ms: f64) -> Option<i64> {
    if value.is_nan() {
        return None;
    }
    let tmp = value * scale_ms;
    let millis = tmp + if value >= 0.0 { 0.5 } else { -0.5 };
    // i64::MAX / 10_000 ticks per millisecond
    const LIMIT: f64 = 922_337_203_685_477.0;
    if !(-LIMIT..=LIMIT).contains(&millis) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)] // checked against LIMIT above
    Some(millis as i64)
}

/// Formats a decimal like `Decimal.ToString(InvariantCulture)` (keeps the scale,
/// never prints a negative zero).
#[must_use]
pub fn format_decimal(value: Decimal) -> String {
    if value.is_zero() {
        let mut z = value;
        z.set_sign_positive(true);
        return z.to_string();
    }
    value.to_string()
}

/// Formats with exactly `decimals` decimals, rounding half away from zero
/// (`decimal.ToString("0.000", Invariant)`); never prints a negative zero.
#[must_use]
pub fn format_fixed(value: Decimal, decimals: u32) -> String {
    let mut r = value.round_dp_with_strategy(decimals, rust_decimal::RoundingStrategy::MidpointAwayFromZero);
    if r.is_zero() {
        r.set_sign_positive(true);
    }
    format!("{r:.prec$}", prec = decimals as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn parse_number_accepts_dotnet_number_style() {
        assert_eq!(parse_number("5"), Some(d("5")));
        assert_eq!(parse_number("-5"), Some(d("-5")));
        assert_eq!(parse_number("5-"), Some(d("-5")));
        assert_eq!(parse_number(".5"), Some(d("0.5")));
        assert_eq!(parse_number("-.5"), Some(d("-0.5")));
        assert_eq!(parse_number("5."), Some(d("5")));
        assert_eq!(parse_number("1.50").map(|v| v.to_string()), Some("1.50".into()));
        assert_eq!(parse_number("-0").map(format_decimal), Some("0".into()));
    }

    #[test]
    fn parse_number_rejects_what_dotnet_rejects() {
        for s in ["", "-", ".", "-.", "1..5", "-4-2", "--5", "5--", "1.2.3", "٣", "1-2"] {
            assert_eq!(parse_number(s), None, "{s:?}");
        }
    }

    #[test]
    fn decimal_from_f64_keeps_15_significant_digits() {
        assert_eq!(decimal_from_f64(0.1).unwrap().to_string(), "0.1");
        assert_eq!(decimal_from_f64(1.0 / 3.0).unwrap().to_string(), "0.333333333333333");
        assert_eq!(decimal_from_f64(-2.5).unwrap().to_string(), "-2.5");
        assert_eq!(decimal_from_f64(1234.5678).unwrap().to_string(), "1234.5678");
        assert_eq!(decimal_from_f64(1e20).unwrap().to_string(), "100000000000000000000");
        assert_eq!(decimal_from_f64(0.0).unwrap(), Decimal::ZERO);
        assert_eq!(decimal_from_f64(1e-30).unwrap(), Decimal::ZERO);
        assert_eq!(decimal_from_f64(f64::NAN), None);
        assert_eq!(decimal_from_f64(f64::INFINITY), None);
        assert_eq!(decimal_from_f64(1e30), None);
    }

    #[test]
    fn millis_round_like_timespan() {
        assert_eq!(millis_from(1.0 / 60_000.0 * 1.4, 60_000.0), Some(1));
        assert_eq!(millis_from(0.5, 1000.0), Some(500));
        assert_eq!(millis_from(0.0005, 1000.0), Some(1));
        assert_eq!(millis_from(-0.0005, 1000.0), Some(-1));
        assert_eq!(millis_from(f64::NAN, 1000.0), None);
        assert_eq!(millis_from(1e300, 1000.0), None);
    }

    #[test]
    fn round_half_even_matches_math_round() {
        assert_eq!(round_half_even(2.5, 0), 2.0);
        assert_eq!(round_half_even(3.5, 0), 4.0);
        assert_eq!(round_half_even(1e17, 2), 1e17);
        assert_eq!(round_half_even(0.123_456_789_012_345_67, 14), 0.123_456_789_012_35);
    }

    #[test]
    fn format_fixed_rounds_away_from_zero() {
        assert_eq!(format_fixed(d("0.25"), 1), "0.3");
        assert_eq!(format_fixed(d("-0.25"), 1), "-0.3");
        assert_eq!(format_fixed(d("0.04"), 1), "0.0");
        assert_eq!(format_fixed(d("-0.04"), 1), "0.0");
        assert_eq!(format_fixed(d("10"), 3), "10.000");
    }

    #[test]
    fn format_decimal_never_prints_negative_zero() {
        assert_eq!(format_decimal(d("-0.0")), "0.0");
        assert_eq!(format_decimal(d("-1.20")), "-1.20");
        assert_eq!(f64_from_decimal(d("1.5")), 1.5);
    }
}
