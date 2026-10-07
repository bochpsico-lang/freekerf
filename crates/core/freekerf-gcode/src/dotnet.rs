//! Number formatting of .NET as LaserGRBL runs it (Mono 6.8, whose `Number`
//! formatting comes from corefx): `double.ToString()`, `float.ToString()` and custom
//! formats such as `"0.###"` and `"0.00"`.
//!
//! The digits come from `DoubleToNumber`: the value printed with `%.40e` (exact),
//! truncated to the precision (15 significant digits for `double`, 7 for `float`)
//! and rounded half up on the next digit. Formats then round again on that digit
//! string (`RoundNumber`), which is why `0.0004999999999999999` formats as `0.001`
//! with `"0.###"`. G-code written by LaserGRBL's importers and generators goes
//! through these functions, and so does FreeKerf's when it must match them.

/// Significant digits of `double` formatting.
pub const DOUBLE_PRECISION: usize = 15;
/// Significant digits of `float` formatting.
pub const FLOAT_PRECISION: usize = 7;

/// The decimal digits of a number (`NumberBuffer`): `0.d1d2d3… × 10^scale`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Digits {
    digits: Vec<u8>,
    scale: i32,
    negative: bool,
}

impl Digits {
    fn get(&self, i: usize) -> u8 {
        self.digits.get(i).copied().unwrap_or(0)
    }
}

/// `Number.DoubleToNumber` of Mono 6.8 (`Number.Unix.cs`); `None` for NaN/∞.
fn double_to_number(value: f64, precision: usize) -> Option<Digits> {
    if !value.is_finite() {
        return None;
    }
    let negative = value.is_sign_negative();
    if value.to_bits() == 0 {
        return Some(Digits {
            digits: vec![b'0'; precision],
            scale: 0,
            negative,
        });
    }
    let text = format!("{value:.40e}");
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let scale = if exponent == 0 { 1 } else { exponent + 1 };
    let all: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    let mut digits: Vec<u8> = all.iter().copied().take(precision).collect();
    while digits.len() < precision {
        digits.push(b'0');
    }
    let mut number = Digits {
        digits,
        scale,
        negative,
    };
    if all.get(precision).is_some_and(|&d| d >= b'5') {
        let mut i = precision - 1;
        while number.digits[i] == b'9' && i > 0 {
            number.digits[i] = b'0';
            i -= 1;
        }
        if i == 0 && number.digits[0] == b'9' {
            number.digits[0] = b'1';
            number.scale += 1;
        } else {
            number.digits[i] += 1;
        }
    }
    Some(number)
}

/// `Number.RoundNumber`: keeps `pos` digits (rounding half up on the next one) and
/// drops trailing zeros.
fn round_number(n: &mut Digits, pos: i32) {
    let mut i = 0usize;
    while (i as i64) < i64::from(pos) && n.get(i) != 0 {
        i += 1;
    }
    if (i as i64) == i64::from(pos) && n.get(i) >= b'5' {
        while i > 0 && n.get(i - 1) == b'9' {
            i -= 1;
        }
        if i > 0 {
            n.digits[i - 1] += 1;
        } else {
            n.scale += 1;
            if n.digits.is_empty() {
                n.digits.push(b'1');
            } else {
                n.digits[0] = b'1';
            }
            i = 1;
        }
    } else {
        while i > 0 && n.get(i - 1) == b'0' {
            i -= 1;
        }
    }
    if i == 0 {
        n.scale = 0;
        n.negative = false;
    }
    n.digits.truncate(i);
}

/// `ToString()` / `"G"`: up to `precision` significant digits, scientific notation
/// (`1E+15`, `1E-05`) outside `1e-5 ≤ |v| < 10^precision`.
fn general(value: f64, precision: usize) -> String {
    let Some(mut n) = double_to_number(value, precision) else {
        return non_finite(value);
    };
    let max = i32::try_from(precision).unwrap_or(15);
    round_number(&mut n, max);
    let mut out = String::new();
    if n.negative {
        out.push('-');
    }
    let mut dig_pos = n.scale;
    let scientific = dig_pos > max || dig_pos < -3;
    if scientific {
        dig_pos = 1;
    }
    let mut cur = 0usize;
    if dig_pos > 0 {
        while dig_pos > 0 {
            out.push(char::from(if n.get(cur) == 0 { b'0' } else { n.digits[cur] }));
            if n.get(cur) != 0 {
                cur += 1;
            }
            dig_pos -= 1;
        }
    } else {
        out.push('0');
    }
    if n.get(cur) != 0 || dig_pos < 0 {
        out.push('.');
        while dig_pos < 0 {
            out.push('0');
            dig_pos += 1;
        }
        while n.get(cur) != 0 {
            out.push(char::from(n.digits[cur]));
            cur += 1;
        }
    }
    if scientific {
        let e = n.scale - 1;
        out.push('E');
        out.push(if e < 0 { '-' } else { '+' });
        out.push_str(&format!("{:02}", e.abs()));
    }
    out
}

fn non_finite(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value > 0.0 {
        "Infinity".into()
    } else {
        "-Infinity".into()
    }
}

/// A custom numeric format made of `#`, `0`, one `.` and literal characters
/// (`NumberToStringFormat` without sections, groups, percent or exponents).
fn custom(value: f64, precision: usize, format: &str) -> String {
    let Some(mut n) = double_to_number(value, precision) else {
        return non_finite(value);
    };
    let fmt: Vec<char> = format.chars().collect();
    let mut digit_count = 0i32;
    let mut decimal_pos = -1i32;
    let mut first_digit = i32::MAX;
    let mut last_digit = 0i32;
    for &ch in &fmt {
        match ch {
            '#' => digit_count += 1,
            '0' => {
                if first_digit == i32::MAX {
                    first_digit = digit_count;
                }
                digit_count += 1;
                last_digit = digit_count;
            }
            '.' if decimal_pos < 0 => decimal_pos = digit_count,
            _ => {}
        }
    }
    if decimal_pos < 0 {
        decimal_pos = digit_count;
    }
    if n.get(0) == 0 {
        n.negative = false;
        n.scale = 0;
    } else {
        let pos = n.scale + digit_count - decimal_pos;
        round_number(&mut n, pos);
    }
    let first_digit = if first_digit < decimal_pos {
        decimal_pos - first_digit
    } else {
        0
    };
    let last_digit = if last_digit > decimal_pos {
        decimal_pos - last_digit
    } else {
        0
    };
    let mut dig_pos = n.scale.max(decimal_pos);
    let mut adjust = n.scale - decimal_pos;

    let mut out = String::new();
    if n.negative {
        out.push('-');
    }
    let mut cur = 0usize;
    let mut decimal_written = false;
    for &ch in &fmt {
        if adjust > 0 && matches!(ch, '#' | '0' | '.') {
            while adjust > 0 {
                out.push(char::from(if n.get(cur) == 0 { b'0' } else { n.digits[cur] }));
                if n.get(cur) != 0 {
                    cur += 1;
                }
                dig_pos -= 1;
                adjust -= 1;
            }
        }
        match ch {
            '#' | '0' => {
                let c = if adjust < 0 {
                    adjust += 1;
                    if dig_pos <= first_digit { Some('0') } else { None }
                } else if n.get(cur) != 0 {
                    cur += 1;
                    Some(char::from(n.digits[cur - 1]))
                } else if dig_pos > last_digit {
                    Some('0')
                } else {
                    None
                };
                if let Some(c) = c {
                    out.push(c);
                }
                dig_pos -= 1;
            }
            '.' => {
                if dig_pos == 0
                    && !decimal_written
                    && (last_digit < 0 || (decimal_pos < digit_count && n.get(cur) != 0))
                {
                    out.push('.');
                    decimal_written = true;
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// `double.ToString(CultureInfo.InvariantCulture)`: 15 significant digits.
#[must_use]
pub fn double_to_string(value: f64) -> String {
    general(value, DOUBLE_PRECISION)
}

/// `float.ToString(CultureInfo.InvariantCulture)`: 7 significant digits.
#[must_use]
pub fn single_to_string(value: f32) -> String {
    general(f64::from(value), FLOAT_PRECISION)
}

/// `double.ToString(format, CultureInfo.InvariantCulture)` for formats like `"0.###"`.
#[must_use]
pub fn double_format(value: f64, format: &str) -> String {
    custom(value, DOUBLE_PRECISION, format)
}

/// `float.ToString(format, CultureInfo.InvariantCulture)` for formats like `"0.###"`.
#[must_use]
pub fn single_format(value: f32, format: &str) -> String {
    custom(f64::from(value), FLOAT_PRECISION, format)
}

/// `int.ToString()`.
#[must_use]
pub fn int_to_string(value: i64) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_print_fifteen_significant_digits() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (-2.5, "-2.5"),
            (0.1 + 0.2, "0.3"),
            (1.0 / 3.0, "0.333333333333333"),
            (0.003 - 1e-16, "0.0029999999999999"),
            (0.005 + 0.041, "0.046"),
            (123_456_789_012_345.0, "123456789012345"),
            (1e15, "1E+15"),
            (1_234_567_890_123_456_789.0, "1.23456789012346E+18"),
            (0.0001, "0.0001"),
            (0.00001, "1E-05"),
            (-0.000_012_5, "-1.25E-05"),
            (9.999_999_999_999_999, "10"),
            (f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
        ];
        for &(v, s) in cases {
            assert_eq!(double_to_string(v), s, "{v:e}");
        }
    }

    #[test]
    fn floats_print_seven_significant_digits() {
        assert_eq!(single_to_string(44.999), "44.999");
        assert_eq!(single_to_string(400.0 / 255.0), "1.568627");
        assert_eq!(single_to_string(1000.0), "1000");
        assert_eq!(single_to_string(0.1), "0.1");
        assert_eq!(single_to_string(12_345_678.0), "1.234568E+07");
        assert_eq!(single_to_string(-0.000_123_4), "-0.0001234");
    }

    #[test]
    fn custom_formats_round_on_the_digit_string() {
        let cases: &[(f64, &str, &str)] = &[
            (3.0, "0.###", "3"),
            (13.0005, "0.###", "13.001"),
            (0.0004999999999999999, "0.###", "0.001"),
            (0.0004, "0.###", "0"),
            (-0.0004, "0.###", "0"),
            (-1.5, "0.###", "-1.5"),
            (1234.5678, "0.###", "1234.568"),
            (0.5, "0.00", "0.50"),
            (1.0 / 255.0 * 128.0, "0.00", "0.50"),
            (0.0, "0.00", "0.00"),
            (2.0, "0", "2"),
            (2.5, "0", "3"),
            (999.9996, "0.###", "1000"),
            (0.25, "#.##", ".25"),
            (5.0, "X0.0", "X5.0"),
        ];
        for &(v, f, s) in cases {
            assert_eq!(double_format(v, f), s, "{v} {f}");
        }
        assert_eq!(single_format(44.999, "0.###"), "44.999");
        assert_eq!(single_format(0.1f32 + 0.2, "0.###"), "0.3");
        assert_eq!(double_format(f64::NAN, "0.###"), "NaN");
        assert_eq!(int_to_string(-12), "-12");
    }

    #[test]
    fn rounding_carries_into_new_digits() {
        let mut n = double_to_number(9.995, 15).unwrap();
        round_number(&mut n, 3);
        assert_eq!((n.digits.clone(), n.scale), (b"1".to_vec(), 2));
        let mut z = Digits {
            digits: Vec::new(),
            scale: 0,
            negative: true,
        };
        round_number(&mut z, 0);
        assert!(!z.negative);
    }
}
