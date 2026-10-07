//! `[expression]` evaluation in custom code (header, footer, passes, buttons).
//!
//! Each `[...]` is replaced by its value formatted with three decimals (`[left]` →
//! `10.000`). An expression that cannot be evaluated is left as it was, brackets
//! included. Supported: numbers, variables, `+ - * / ^`, parentheses, unary minus
//! and the functions `abs`, `neg` and `sign` (as in LaserGRBL's evaluator).

use std::collections::BTreeMap;

use freekerf_gcode::Decimal;
use freekerf_gcode::number::{decimal_from_f64, format_fixed};

/// Variables available to expressions.
pub type Variables = BTreeMap<String, f64>;

/// Replaces every `[expr]` in `text` and trims the result (`EvaluateExpression`).
#[must_use]
pub fn evaluate_brackets(text: &str, vars: &Variables) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        // `\[(?:[^]]+)\]`: at least one character that is not `]`
        match after.find(']') {
            Some(close) if close > 0 => {
                out.push_str(&rest[..open]);
                let inner = &after[..close];
                match evaluate(inner, vars).and_then(format_value) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('[');
                        out.push_str(inner);
                        out.push(']');
                    }
                }
                rest = &after[close + 1..];
            }
            _ => {
                out.push_str(&rest[..=open]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_owned()
}

/// `{0:0.000}` of a decimal (half away from zero).
fn format_value(v: f64) -> Option<String> {
    let d: Decimal = decimal_from_f64(v)?;
    Some(format_fixed(d, 3))
}

/// Evaluates one expression; `None` on syntax errors, unknown variables, division
/// by zero or non-finite results.
#[must_use]
pub fn evaluate(text: &str, vars: &Variables) -> Option<f64> {
    let cleaned: String = text.chars().filter(|c| *c != ' ').collect();
    let mut p = Parser { s: cleaned.as_bytes(), pos: 0, vars };
    let v = p.sum()?;
    (p.pos == p.s.len() && v.is_finite()).then_some(v)
}

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
    vars: &'a Variables,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn sum(&mut self) -> Option<f64> {
        let mut v = self.product()?;
        loop {
            if self.eat(b'+') {
                v += self.product()?;
            } else if self.eat(b'-') {
                v -= self.product()?;
            } else {
                return Some(v);
            }
        }
    }

    fn product(&mut self) -> Option<f64> {
        let mut v = self.power()?;
        loop {
            if self.eat(b'*') {
                v *= self.power()?;
            } else if self.eat(b'/') {
                let d = self.power()?;
                if d == 0.0 {
                    return None;
                }
                v /= d;
            } else {
                return Some(v);
            }
        }
    }

    fn power(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        while self.eat(b'^') {
            v = v.powf(self.unary()?);
        }
        Some(v)
    }

    fn unary(&mut self) -> Option<f64> {
        if self.eat(b'-') {
            return self.unary().map(|v| -v);
        }
        if self.eat(b'+') {
            return self.unary();
        }
        for (name, f) in [("abs", f64::abs as fn(f64) -> f64), ("neg", |v: f64| -v), ("sign", |v: f64| {
            if v >= 0.0 { 1.0 } else { -1.0 }
        })] {
            if self.s[self.pos..].starts_with(name.as_bytes()) {
                let save = self.pos;
                self.pos += name.len();
                if matches!(self.peek(), Some(b'(' | b'-' | b'0'..=b'9' | b'.')) {
                    return self.unary().map(f);
                }
                self.pos = save;
            }
        }
        self.atom()
    }

    fn atom(&mut self) -> Option<f64> {
        if self.eat(b'(') {
            let v = self.sum()?;
            return self.eat(b')').then_some(v);
        }
        let start = self.pos;
        match self.peek()? {
            b'0'..=b'9' | b'.' => {
                while matches!(self.peek(), Some(b'0'..=b'9' | b'.')) {
                    self.pos += 1;
                }
                // scientific notation: 1e-3
                if matches!(self.peek(), Some(b'e' | b'E')) {
                    let save = self.pos;
                    self.pos += 1;
                    let _ = self.eat(b'-') || self.eat(b'+');
                    if matches!(self.peek(), Some(b'0'..=b'9')) {
                        while matches!(self.peek(), Some(b'0'..=b'9')) {
                            self.pos += 1;
                        }
                    } else {
                        self.pos = save;
                    }
                }
                std::str::from_utf8(&self.s[start..self.pos]).ok()?.parse().ok()
            }
            c if c.is_ascii_alphabetic() || c == b'$' || c == b'_' => {
                while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || matches!(c, b'.' | b'$' | b'_')) {
                    self.pos += 1;
                }
                let name = std::str::from_utf8(&self.s[start..self.pos]).ok()?;
                self.vars.get(name).copied()
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Variables {
        [("left", 10.0), ("width", 20.5), ("$30", 1000.0), ("WCO.X", -1.25)]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect()
    }

    #[test]
    fn arithmetic_and_precedence() {
        let v = vars();
        assert_eq!(evaluate("1+2*3", &v), Some(7.0));
        assert_eq!(evaluate("(1+2)*3", &v), Some(9.0));
        assert_eq!(evaluate("2^3^2", &v), Some(64.0));
        assert_eq!(evaluate("2*3^2", &v), Some(18.0));
        assert_eq!(evaluate("-left + 1", &v), Some(-9.0));
        assert_eq!(evaluate("$30/2", &v), Some(500.0));
        assert_eq!(evaluate("abs(WCO.X)", &v), Some(1.25));
        assert_eq!(evaluate("neg 2", &v), Some(-2.0));
        assert_eq!(evaluate("sign(-3)", &v), Some(-1.0));
        assert_eq!(evaluate("sign 0", &v), Some(1.0));
        assert_eq!(evaluate("1e3 + 1E-1", &v), Some(1000.1));
        assert_eq!(evaluate("+4", &v), Some(4.0));
        assert_eq!(evaluate("8/4-1", &v), Some(1.0));
    }

    #[test]
    fn errors() {
        let v = vars();
        for bad in ["1/0", "unknown", "(1", "1+", "", "1)", "#", "abs", "1e"] {
            assert_eq!(evaluate(bad, &v), None, "{bad}");
        }
    }

    #[test]
    fn brackets_are_replaced_and_formatted() {
        let v = vars();
        assert_eq!(evaluate_brackets("G0 X[left] Y[width/2]", &v), "G0 X10.000 Y10.250");
        assert_eq!(evaluate_brackets("S[$30*0.0005]", &v), "S0.500");
        assert_eq!(evaluate_brackets("  X[nope] ", &v), "X[nope]");
        assert_eq!(evaluate_brackets("[] [ok", &v), "[] [ok");
        assert_eq!(evaluate_brackets("M5", &v), "M5");
        assert_eq!(evaluate_brackets("[0.0005]", &v), "0.001");
        assert_eq!(evaluate_brackets("[-0.0005]", &v), "-0.001");
    }
}
