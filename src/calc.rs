use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalcError(pub String);

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub fn evaluate(input: &str) -> Result<f64, CalcError> {
    let normalized = normalize(input)
        .replace(',', ".")
        .replace('×', "*")
        .replace('÷', "/")
        .replace('−', "-");
    let mut parser = Parser::new(&normalized);
    let value = parser.expr()?;
    parser.skip_ws();
    if let Some(ch) = parser.peek_raw() {
        return Err(CalcError(format!("лишний символ «{ch}»")));
    }
    Ok(value)
}

/// Whitespace between digits is a thousands separator. Other whitespace is
/// retained as one ordinary space so expressions such as `2 + 3` still work.
pub fn normalize(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            let left_is_digit = i > 0 && chars[i - 1].is_ascii_digit();
            let mut right = i + 1;
            while right < chars.len() && chars[right].is_whitespace() {
                right += 1;
            }
            if !(left_is_digit && right < chars.len() && chars[right].is_ascii_digit()) {
                out.push(' ');
            }
            i = right;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

pub fn format_number(value: f64, decimal_comma: bool) -> String {
    if value.is_nan() {
        return "не число".into();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() { "∞" } else { "-∞" }.into();
    }
    let rounded = (value * 1_000_000_000_000.0).round() / 1_000_000_000_000.0;
    let mut text = format!("{rounded:.15}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" {
        text = "0".into();
    }
    if decimal_comma {
        text = text.replace('.', ",");
    }
    text
}

pub fn pretty(input: &str) -> String {
    input.replace('*', "×").replace('/', "÷").replace('-', "−")
}

pub fn last_operation(input: &str) -> Option<String> {
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut depth = 0i32;
    for &(byte_index, ch) in chars.iter().rev() {
        match ch {
            ')' => depth += 1,
            '(' => depth -= 1,
            '+' | '-' | '*' | '/' | '^' if depth == 0 && byte_index > 0 => {
                let left = input[..byte_index].trim_end().chars().last();
                if matches!(left, Some(c) if c.is_ascii_digit() || matches!(c, ')' | '%' | '.' | ',')) {
                    return Some(input[byte_index..].trim().to_owned());
                }
            }
            _ => {}
        }
    }
    None
}

struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    _source: &'a str,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Self { chars: source.chars().collect(), pos: 0, _source: source }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_ws();
        self.peek_raw()
    }

    fn peek_raw(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn expr(&mut self) -> Result<f64, CalcError> {
        let mut value = self.term()?;
        loop {
            match self.peek() {
                Some('+') => { self.pos += 1; value += self.term()?; }
                Some('-') => { self.pos += 1; value -= self.term()?; }
                _ => return Ok(value),
            }
        }
    }

    fn term(&mut self) -> Result<f64, CalcError> {
        let mut value = self.power()?;
        loop {
            match self.peek() {
                Some('*') => { self.pos += 1; value *= self.power()?; }
                Some('/') => { self.pos += 1; value /= self.power()?; }
                Some('(') | Some('.') => value *= self.power()?,
                Some(ch) if ch.is_ascii_digit() => value *= self.power()?,
                _ => return Ok(value),
            }
        }
    }

    fn power(&mut self) -> Result<f64, CalcError> {
        let base = self.unary()?;
        if self.peek() == Some('^') {
            self.pos += 1;
            Ok(base.powf(self.power()?))
        } else {
            Ok(base)
        }
    }

    fn unary(&mut self) -> Result<f64, CalcError> {
        match self.peek() {
            Some('-') => { self.pos += 1; return Ok(-self.unary()?); }
            Some('+') => { self.pos += 1; return self.unary(); }
            _ => {}
        }
        let mut value = self.primary()?;
        while self.peek() == Some('%') {
            self.pos += 1;
            value /= 100.0;
        }
        Ok(value)
    }

    fn primary(&mut self) -> Result<f64, CalcError> {
        if self.peek() == Some('(') {
            self.pos += 1;
            let value = self.expr()?;
            if self.peek() == Some(')') {
                self.pos += 1;
            }
            return Ok(value);
        }

        self.skip_ws();
        let start = self.pos;
        while matches!(self.peek_raw(), Some(c) if c.is_ascii_digit() || c == '.') {
            self.pos += 1;
        }
        if matches!(self.peek_raw(), Some('e' | 'E')) && self.pos > start {
            let exponent = self.pos;
            self.pos += 1;
            if matches!(self.peek_raw(), Some('+' | '-')) {
                self.pos += 1;
            }
            let digits = self.pos;
            while matches!(self.peek_raw(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
            if self.pos == digits {
                self.pos = exponent;
            }
        }
        if self.pos == start {
            return Err(CalcError(match self.peek_raw() {
                Some(ch) => format!("неожиданный «{ch}»"),
                None => "неполное выражение".into(),
            }));
        }
        self.chars[start..self.pos]
            .iter()
            .collect::<String>()
            .parse::<f64>()
            .map_err(|_| CalcError("неверное число".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_eval(input: &str, expected: f64) {
        let actual = evaluate(input).unwrap();
        assert!((actual - expected).abs() < 1e-10, "{input}: {actual} != {expected}");
    }

    #[test]
    fn grouped_numbers_and_both_decimal_separators() {
        assert_eval("1 000 000", 1_000_000.0);
        assert_eval("1\u{a0}000\u{202f}000", 1_000_000.0);
        assert_eval("1 000,5 + 0,5", 1001.0);
        assert_eval("1 000.5 + 0.5", 1001.0);
    }

    #[test]
    fn operations() {
        assert_eval("2 + 3 * 4", 14.0);
        assert_eval("2(3+4)", 14.0);
        assert_eval("50%", 0.5);
        assert_eval("2^3^2", 512.0);
    }
}

