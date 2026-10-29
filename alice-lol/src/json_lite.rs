//! A small strict JSON reader for the law tools' inputs: objects (keys kept in order, a key
//! given twice is refused), arrays, strings without escapes, finite numbers, `true` / `false`
//! and null; no nesting deeper than 16. Inputs outside this subset are refused, not
//! guessed.

/// A small strict JSON value (objects keep their key order)
#[derive(Debug, Clone, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Self>),
    Obj(Vec<(String, Self)>),
}

struct P<'a> {
    b: &'a [u8],
    i: usize,
}

impl P<'_> {
    const fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn fail<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("JSON at byte {}: {what}", self.i))
    }
    fn eat(&mut self, c: u8) -> Result<(), String> {
        self.ws();
        if self.b.get(self.i) == Some(&c) {
            self.i += 1;
            Ok(())
        } else {
            self.fail(&format!("expected `{}`", c as char))
        }
    }
    #[allow(clippy::too_many_lines)] // one arm per kind of value
    fn value(&mut self, depth: usize) -> Result<J, String> {
        if depth > 16 {
            return self.fail("nested too deeply");
        }
        self.ws();
        match self.b.get(self.i) {
            Some(b'{') => {
                self.i += 1;
                let mut o = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Ok(J::Obj(o));
                }
                loop {
                    self.ws();
                    let J::Str(k) = self.value(depth + 1)? else {
                        return self.fail("an object key must be a string");
                    };
                    if o.iter().any(|(kk, _)| *kk == k) {
                        return self.fail(&format!("key `{k}` given twice"));
                    }
                    self.eat(b':')?;
                    o.push((k, self.value(depth + 1)?));
                    self.ws();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(J::Obj(o));
                        }
                        _ => return self.fail("expected `,` or `}`"),
                    }
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Ok(J::Arr(a));
                }
                loop {
                    a.push(self.value(depth + 1)?);
                    self.ws();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(J::Arr(a));
                        }
                        _ => return self.fail("expected `,` or `]`"),
                    }
                }
            }
            Some(b'"') => {
                self.i += 1;
                let start = self.i;
                while self.i < self.b.len() && self.b[self.i] != b'"' {
                    if self.b[self.i] == b'\\' || self.b[self.i] < 0x20 {
                        return self
                            .fail("escapes and control characters are not accepted in strings");
                    }
                    self.i += 1;
                }
                if self.i >= self.b.len() {
                    return self.fail("unterminated string");
                }
                let s = std::str::from_utf8(&self.b[start..self.i])
                    .map_err(|_| "JSON string is not UTF-8".to_owned())?
                    .to_owned();
                self.i += 1;
                Ok(J::Str(s))
            }
            Some(b't') if self.b[self.i..].starts_with(b"true") => {
                self.i += 4;
                Ok(J::Bool(true))
            }
            Some(b'f') if self.b[self.i..].starts_with(b"false") => {
                self.i += 5;
                Ok(J::Bool(false))
            }
            Some(b'n') if self.b[self.i..].starts_with(b"null") => {
                self.i += 4;
                Ok(J::Null)
            }
            Some(c) if c.is_ascii_digit() || *c == b'-' => {
                let start = self.i;
                while self.i < self.b.len()
                    && matches!(
                        self.b[self.i],
                        b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'
                    )
                {
                    self.i += 1;
                }
                let t = std::str::from_utf8(&self.b[start..self.i]).unwrap_or("");
                match t.parse::<f64>() {
                    Ok(v) if v.is_finite() => Ok(J::Num(v)),
                    _ => self.fail(&format!("`{t}` is not a finite number")),
                }
            }
            _ => self.fail("expected a value"),
        }
    }
}

/// Read one JSON value that fills the whole text
///
/// # Errors
///
/// A message naming the byte where the text leaves the subset.
pub fn parse(text: &str) -> Result<J, String> {
    let mut p = P {
        b: text.as_bytes(),
        i: 0,
    };
    let v = p.value(0)?;
    p.ws();
    if p.i != p.b.len() {
        return p.fail("trailing characters after the JSON value");
    }
    Ok(v)
}

/// The value of key `k` in an object
pub fn get<'a>(o: &'a [(String, J)], k: &str) -> Option<&'a J> {
    o.iter().find(|(kk, _)| kk == k).map(|(_, v)| v)
}
