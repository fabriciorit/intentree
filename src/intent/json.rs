//! Hand-written JSON with the exact output of Python's `json.dumps(..., ensure_ascii=False)`,
//! the Python formatting that shows up in messages (repr of text and float) and the canonical
//! number of sentences.

/// JSON value with key order preserved.
#[derive(Clone, Debug, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    Str(String),
    Num(f64),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    pub fn s(t: &str) -> J {
        J::Str(t.to_string())
    }

    pub fn obj(pairs: Vec<(&str, J)>) -> J {
        J::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    /// Shape of `json.dumps(x)` without indentation: separators ", " and ": ".
    pub fn compact(&self) -> String {
        let mut o = String::new();
        self.emit(&mut o, None, 0);
        o
    }

    /// Shape of `json.dumps(x, indent=1)`.
    pub fn indented(&self) -> String {
        let mut o = String::new();
        self.emit(&mut o, Some(1), 0);
        o
    }

    fn emit(&self, o: &mut String, indent: Option<usize>, level: usize) {
        let line_break = |o: &mut String, n: usize| {
            if let Some(k) = indent {
                o.push('\n');
                o.push_str(&" ".repeat(k * n));
            }
        };
        let sep = if indent.is_some() { "," } else { ", " };
        match self {
            J::Null => o.push_str("null"),
            J::Bool(b) => o.push_str(if *b { "true" } else { "false" }),
            J::Str(t) => o.push_str(&text(t)),
            J::Num(x) => o.push_str(&repr_float(*x)),
            J::Arr(v) if v.is_empty() => o.push_str("[]"),
            J::Obj(v) if v.is_empty() => o.push_str("{}"),
            J::Arr(v) => {
                o.push('[');
                for (i, x) in v.iter().enumerate() {
                    if i > 0 {
                        o.push_str(sep);
                    }
                    line_break(o, level + 1);
                    x.emit(o, indent, level + 1);
                }
                line_break(o, level);
                o.push(']');
            }
            J::Obj(v) => {
                o.push('{');
                for (i, (k, x)) in v.iter().enumerate() {
                    if i > 0 {
                        o.push_str(sep);
                    }
                    line_break(o, level + 1);
                    o.push_str(&text(k));
                    o.push_str(": ");
                    x.emit(o, indent, level + 1);
                }
                line_break(o, level);
                o.push('}');
            }
        }
    }
}

/// Reads a JSON document (strict: no comments, no trailing commas). Numbers become `f64`;
/// object keys keep their written order.
pub fn parse(src: &str) -> Result<J, String> {
    let mut p = Parser { b: src.as_bytes(), src, i: 0 };
    let v = p.value(0)?;
    p.ws();
    if p.i != p.b.len() {
        return Err(p.fail("unexpected text after the value"));
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    src: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn fail(&self, m: &str) -> String {
        format!("invalid JSON at byte {}: {m}", self.i)
    }

    fn ws(&mut self) {
        while matches!(self.b.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> Result<(), String> {
        self.ws();
        if self.b.get(self.i) == Some(&c) {
            self.i += 1;
            Ok(())
        } else {
            Err(self.fail(&format!("expected '{}'", c as char)))
        }
    }

    fn word(&mut self, w: &str, v: J) -> Result<J, String> {
        if self.src[self.i..].starts_with(w) {
            self.i += w.len();
            Ok(v)
        } else {
            Err(self.fail("unknown value"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<J, String> {
        if depth > 64 {
            return Err(self.fail("nested too deep"));
        }
        self.ws();
        match self.b.get(self.i) {
            None => Err(self.fail("unexpected end")),
            Some(b'n') => self.word("null", J::Null),
            Some(b't') => self.word("true", J::Bool(true)),
            Some(b'f') => self.word("false", J::Bool(false)),
            Some(b'"') => Ok(J::Str(self.string()?)),
            Some(b'[') => {
                self.i += 1;
                let mut v = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Ok(J::Arr(v));
                }
                loop {
                    v.push(self.value(depth + 1)?);
                    self.ws();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(J::Arr(v));
                        }
                        _ => return Err(self.fail("expected ',' or ']'")),
                    }
                }
            }
            Some(b'{') => {
                self.i += 1;
                let mut v = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Ok(J::Obj(v));
                }
                loop {
                    self.ws();
                    if self.b.get(self.i) != Some(&b'"') {
                        return Err(self.fail("expected a string key"));
                    }
                    let k = self.string()?;
                    self.eat(b':')?;
                    v.push((k, self.value(depth + 1)?));
                    self.ws();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(J::Obj(v));
                        }
                        _ => return Err(self.fail("expected ',' or '}'")),
                    }
                }
            }
            Some(_) => {
                let start = self.i;
                while matches!(self.b.get(self.i), Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')) {
                    self.i += 1;
                }
                self.src[start..self.i].parse::<f64>().map(J::Num).map_err(|_| self.fail("invalid number"))
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let h = self.src.get(self.i..self.i + 4).ok_or_else(|| self.fail("short \\u escape"))?;
        let n = u32::from_str_radix(h, 16).map_err(|_| self.fail("invalid \\u escape"))?;
        self.i += 4;
        Ok(n)
    }

    fn string(&mut self) -> Result<String, String> {
        self.i += 1; // opening quote
        let mut o = String::new();
        loop {
            let rest = &self.src[self.i..];
            let Some(c) = rest.chars().next() else { return Err(self.fail("unterminated string")) };
            self.i += c.len_utf8();
            match c {
                '"' => return Ok(o),
                '\\' => {
                    let e = self.b.get(self.i).copied().ok_or_else(|| self.fail("unterminated escape"))?;
                    self.i += 1;
                    match e {
                        b'"' => o.push('"'),
                        b'\\' => o.push('\\'),
                        b'/' => o.push('/'),
                        b'b' => o.push('\u{8}'),
                        b'f' => o.push('\u{c}'),
                        b'n' => o.push('\n'),
                        b'r' => o.push('\r'),
                        b't' => o.push('\t'),
                        b'u' => {
                            let mut n = self.hex4()?;
                            if (0xD800..0xDC00).contains(&n) && self.src[self.i..].starts_with("\\u") {
                                let save = self.i;
                                self.i += 2;
                                let lo = self.hex4()?;
                                if (0xDC00..0xE000).contains(&lo) {
                                    n = 0x10000 + ((n - 0xD800) << 10) + (lo - 0xDC00);
                                } else {
                                    self.i = save;
                                }
                            }
                            o.push(char::from_u32(n).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err(self.fail("invalid escape")),
                    }
                }
                c if (c as u32) < 0x20 => return Err(self.fail("control character in a string")),
                c => o.push(c),
            }
        }
    }
}

impl J {
    /// The text, if this is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            J::Str(t) => Some(t),
            _ => None,
        }
    }
}

/// JSON text like `json.dumps` with `ensure_ascii=False`.
pub fn text(t: &str) -> String {
    let mut o = String::with_capacity(t.len() + 2);
    o.push('"');
    for c in t.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            '\u{8}' => o.push_str("\\b"),
            '\u{c}' => o.push_str("\\f"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Significant digits and scientific exponent of Rust's `{:e}` / `{:.Ne}`.
fn digits(s: &str) -> (bool, String, i32) {
    let (mant, exp) = s.split_once('e').expect("scientific format");
    let neg = mant.starts_with('-');
    let d: String = mant.chars().filter(|c| c.is_ascii_digit()).collect();
    (neg, d, exp.parse().expect("exponent"))
}

fn exponent(e: i32) -> String {
    format!("e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
}

/// Digits `d` with the decimal point after `decpt` digits (may be <= 0 or > len).
fn fixed(d: &str, decpt: i32) -> String {
    let n = d.len() as i32;
    if decpt <= 0 {
        format!("0.{}{d}", "0".repeat((-decpt) as usize))
    } else if decpt >= n {
        format!("{d}{}", "0".repeat((decpt - n) as usize))
    } else {
        format!("{}.{}", &d[..decpt as usize], &d[decpt as usize..])
    }
}

/// Python's `repr(float)`: the fewest digits that round-trip to the same value.
pub fn repr_float(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let (neg, d, e) = digits(&format!("{x:e}"));
    let sign = if neg { "-" } else { "" };
    if !(-4..16).contains(&e) {
        let mant = if d.len() > 1 { format!("{}.{}", &d[..1], &d[1..]) } else { d.clone() };
        return format!("{sign}{mant}{}", exponent(e));
    }
    let f = fixed(&d, e + 1);
    if f.contains('.') {
        format!("{sign}{f}")
    } else {
        format!("{sign}{f}.0")
    }
}

/// Canonical number of sentences: decimal with no exponent, with the fewest digits that
/// round-trip to the same value (`f64`'s `Display`). The tokenizer only reads `\d+(\.\d+)?`, so
/// an exponent form (`1e+16`) would not reparse.
pub fn number(x: f64) -> String {
    format!("{x}")
}

/// Python's `repr(str)` (single quotes, unless the text has a single quote and no double quote).
pub fn repr_text(t: &str) -> String {
    let q = if t.contains('\'') && !t.contains('"') { '"' } else { '\'' };
    let mut o = String::new();
    o.push(q);
    for c in t.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if c == q => {
                o.push('\\');
                o.push(c);
            }
            c if (c as u32) < 0x20 || (0x7f..=0xa0).contains(&(c as u32)) || c == '\u{ad}' => {
                o.push_str(&format!("\\x{:02x}", c as u32))
            }
            c if matches!(c as u32, 0x2028 | 0x2029 | 0xfeff | 0x200b..=0x200f) => {
                o.push_str(&format!("\\u{:04x}", c as u32))
            }
            c => o.push(c),
        }
    }
    o.push(q);
    o
}

/// Python's `repr(list[str])`.
pub fn repr_list(v: &[String]) -> String {
    format!("[{}]", v.iter().map(|s| repr_text(s)).collect::<Vec<_>>().join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_like_python() {
        for (x, r) in [
            (30.0, "30.0"),
            (0.5, "0.5"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1e16, "1e+16"),
            (1e15, "1000000000000000.0"),
            (1.5e16, "1.5e+16"),
            (123.456, "123.456"),
            (0.1, "0.1"),
        ] {
            assert_eq!(repr_float(x), r, "{x}");
        }
    }

    #[test]
    fn canonical_number_reparses_to_the_same_value() {
        for (x, t) in [(30.0, "30"), (0.5, "0.5"), (0.00001, "0.00001"), (1e16, "10000000000000000"), (1234567.0, "1234567"), (7.0, "7")] {
            assert_eq!(number(x), t);
        }
        // varied values: every canonical number is only digits and a dot, and round-trips to the same f64
        let mut z: u64 = 1;
        for _ in 0..2000 {
            z = z.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let x = f64::from_bits(z >> 2) % 1e20;
            let x = if x.is_finite() { x.abs() } else { 1.0 };
            let t = number(x);
            assert!(t.chars().all(|c| c.is_ascii_digit() || c == '.'), "{t}");
            assert_eq!(t.parse::<f64>().unwrap(), x, "{t}");
        }
    }

    #[test]
    fn json_indented() {
        let v = J::obj(vec![("a", J::Arr(vec![])), ("b", J::Obj(vec![])), ("c", J::Arr(vec![J::Num(1.0), J::Null]))]);
        assert_eq!(v.indented(), "{\n \"a\": [],\n \"b\": {},\n \"c\": [\n  1.0,\n  null\n ]\n}");
        assert_eq!(v.compact(), "{\"a\": [], \"b\": {}, \"c\": [1.0, null]}");
        assert_eq!(text("á\"\\\n\u{1}"), "\"á\\\"\\\\\\n\\u0001\"");
        assert_eq!(repr_text("it's"), "\"it's\"");
        assert_eq!(repr_list(&["p.state_machine".into()]), "['p.state_machine']");
    }
}
