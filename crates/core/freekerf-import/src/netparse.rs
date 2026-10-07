//! Number parsing as .NET does it with `NumberStyles.Float` and the invariant
//! culture: optional white space (`\t`..`\r` and space) around, an optional sign,
//! ASCII digits with one optional `.`, an optional exponent, and the symbols
//! `Infinity`, `-Infinity`, `NaN`. Anything else is a failure (LaserGRBL then uses 0).

/// `double.TryParse(s, NumberStyles.Float, InvariantInfo)`.
#[must_use]
pub fn parse_double(s: &str) -> Option<f64> {
    let t = s.trim_matches(|c: char| matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' '));
    match t {
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        "NaN" | "+NaN" | "-NaN" => return Some(f64::NAN),
        _ => {}
    }
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut digits = i - int_start;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let frac = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        digits += i - frac;
    }
    if digits == 0 {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let exp = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp {
            i = j;
        }
    }
    if i != b.len() {
        return None;
    }
    // Rust's parser takes a trailing '.' only with digits before it, as here
    let text = t.strip_suffix('.').unwrap_or(t);
    let text = if text.starts_with('.') || text.starts_with("-.") || text.starts_with("+.") {
        text.replacen('.', "0.", 1)
    } else {
        text.to_owned()
    };
    text.parse().ok()
}

/// `long.Parse(s, NumberStyles.Integer, InvariantCulture)`.
#[must_use]
pub fn parse_long(s: &str) -> Option<i64> {
    let t = s.trim_matches(|c: char| matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' '));
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    t.strip_prefix('+').unwrap_or(t).parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles() {
        for (s, v) in [
            ("1", 1.0),
            (" -2.5 ", -2.5),
            ("+.5", 0.5),
            ("-.5", -0.5),
            ("1.", 1.0),
            ("1e3", 1000.0),
            ("1.5E-2", 0.015),
            ("\t7\r\n", 7.0),
        ] {
            assert_eq!(parse_double(s), Some(v), "{s}");
        }
        for s in ["", ".", "-", "1e", "1.2.3", "abc", "1 2", "inf", "1,5", "0x10"] {
            assert_eq!(parse_double(s), None, "{s}");
        }
        assert_eq!(parse_double("Infinity"), Some(f64::INFINITY));
        assert_eq!(parse_double("-Infinity"), Some(f64::NEG_INFINITY));
        assert!(parse_double("NaN").unwrap().is_nan());
    }

    #[test]
    fn longs() {
        assert_eq!(parse_long(" 42 "), Some(42));
        assert_eq!(parse_long("+7"), Some(7));
        assert_eq!(parse_long("-7"), Some(-7));
        for s in ["", "-", "1.0", "1e3", "x"] {
            assert_eq!(parse_long(s), None, "{s}");
        }
    }
}
