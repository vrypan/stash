use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub enum JsonValue {
    Object(BTreeMap<String, JsonValue>),
    String(String),
    Number(i64),
    Bool(()),
    Null,
}

pub fn parse(input: &str) -> Result<JsonValue, String> {
    let mut p = Parser {
        input: input.as_bytes(),
        pos: 0,
    };
    let value = p.parse_value()?;
    p.skip_ws();
    if p.pos != p.input.len() {
        return Err("trailing data".into());
    }
    Ok(value)
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let ch = self.peek()?;
        self.pos += 1;
        Some(ch)
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'"') => Ok(JsonValue::String(self.parse_string()?)),
            Some(b'-' | b'0'..=b'9') => self.parse_number().map(JsonValue::Number),
            Some(b't') => {
                self.expect_bytes(b"true")?;
                Ok(JsonValue::Bool(()))
            }
            Some(b'f') => {
                self.expect_bytes(b"false")?;
                Ok(JsonValue::Bool(()))
            }
            Some(b'n') => {
                self.expect_bytes(b"null")?;
                Ok(JsonValue::Null)
            }
            _ => Err("unexpected token".into()),
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.expect(b'{')?;
        let mut out = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(out));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            out.insert(key, value);
            self.skip_ws();
            match self.bump() {
                Some(b',') => continue,
                Some(b'}') => break,
                _ => return Err("expected ',' or '}'".into()),
            }
        }
        Ok(JsonValue::Object(out))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        while let Some(ch) = self.bump() {
            match ch {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = self.bump().ok_or_else(|| "incomplete escape".to_string())?;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let code = self.parse_hex4()?;
                            if let Some(ch) = char::from_u32(code) {
                                out.push(ch);
                            } else {
                                return Err("invalid unicode escape".into());
                            }
                        }
                        _ => return Err("invalid escape".into()),
                    }
                }
                _ => out.push(ch as char),
            }
        }
        Err("unterminated string".into())
    }

    fn parse_hex4(&mut self) -> Result<u32, String> {
        let mut code = 0u32;
        for _ in 0..4 {
            let ch = self.bump().ok_or_else(|| "short unicode escape".to_string())?;
            code <<= 4;
            code |= match ch {
                b'0'..=b'9' => (ch - b'0') as u32,
                b'a'..=b'f' => (ch - b'a' + 10) as u32,
                b'A'..=b'F' => (ch - b'A' + 10) as u32,
                _ => return Err("invalid hex digit".into()),
            };
        }
        Ok(code)
    }

    fn parse_number(&mut self) -> Result<i64, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let s = std::str::from_utf8(&self.input[start..self.pos]).map_err(|_| "invalid number")?;
        s.parse::<i64>().map_err(|_| "invalid number".into())
    }

    fn expect(&mut self, want: u8) -> Result<(), String> {
        match self.bump() {
            Some(ch) if ch == want => Ok(()),
            _ => Err(format!("expected '{}'", want as char)),
        }
    }

    fn expect_bytes(&mut self, want: &[u8]) -> Result<(), String> {
        if self.input.get(self.pos..self.pos + want.len()) == Some(want) {
            self.pos += want.len();
            Ok(())
        } else {
            Err("unexpected token".into())
        }
    }
}

pub fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = std::fmt::Write::write_fmt(&mut out, format_args!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}
