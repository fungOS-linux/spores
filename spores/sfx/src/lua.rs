//! A strict parser for the data-only Lua used by pad definitions.
//!
//! This is not a Lua interpreter. It accepts one `return` of tables, strings,
//! numbers, booleans, and nil, with comments. That is enough for a sound
//! designer to edit `ui/synth.lua` without a Rust rebuild, while keeping the
//! guest free of a scripting runtime.

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(f64),
    Str(String),
    Table(Vec<(Option<String>, Value)>),
}

impl Value {
    pub fn field(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Table(entries) => entries
                .iter()
                .find(|(key, _)| key.as_deref() == Some(name))
                .map(|(_, value)| value),
            _ => None,
        }
    }

    pub fn array(&self) -> Option<Vec<&Value>> {
        match self {
            Value::Table(entries) => {
                let mut values = Vec::new();
                for (key, value) in entries {
                    if key.is_some() {
                        return None;
                    }
                    values.push(value);
                }
                Some(values)
            }
            _ => None,
        }
    }
}

struct Parser<'a> {
    source: &'a [u8],
    at: usize,
}

pub fn parse(source: &str) -> Result<Value, String> {
    let mut parser = Parser {
        source: source.as_bytes(),
        at: 0,
    };
    parser.skip();
    parser.keyword("return")?;
    let value = parser.value()?;
    parser.skip();
    if parser.at != parser.source.len() {
        return Err(format!("unexpected trailing input at byte {}", parser.at));
    }
    Ok(value)
}

impl<'a> Parser<'a> {
    fn skip(&mut self) {
        loop {
            while self.at < self.source.len() && self.source[self.at].is_ascii_whitespace() {
                self.at += 1;
            }
            if self.source.get(self.at) == Some(&b'-')
                && self.source.get(self.at + 1) == Some(&b'-')
            {
                self.at += 2;
                while self.at < self.source.len() && self.source[self.at] != b'\n' {
                    self.at += 1;
                }
                continue;
            }
            break;
        }
    }

    fn keyword(&mut self, word: &str) -> Result<(), String> {
        let end = self.at + word.len();
        if self.source.get(self.at..end) != Some(word.as_bytes()) {
            return Err(format!("expected {word:?} at byte {}", self.at));
        }
        self.at = end;
        Ok(())
    }

    fn value(&mut self) -> Result<Value, String> {
        self.skip();
        match self.source.get(self.at) {
            Some(b'{') => self.table(),
            Some(b'"') | Some(b'\'') => Ok(Value::Str(self.string()?)),
            Some(b'-') | Some(b'0'..=b'9') => self.number(),
            Some(_) => {
                let word = self.identifier()?;
                match word.as_str() {
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    "nil" => Ok(Value::Nil),
                    other => Err(format!("unexpected bare value {other:?}")),
                }
            }
            None => Err("unexpected end of input".into()),
        }
    }

    fn table(&mut self) -> Result<Value, String> {
        self.keyword("{")?;
        let mut entries = Vec::new();
        loop {
            self.skip();
            if self.source.get(self.at) == Some(&b'}') {
                self.at += 1;
                return Ok(Value::Table(entries));
            }
            if self.source.get(self.at) == Some(&b'[') {
                self.at += 1;
                let key = self.value()?;
                self.skip();
                if self.source.get(self.at) != Some(&b']') {
                    return Err(format!("expected ] at byte {}", self.at));
                }
                self.at += 1;
                self.skip();
                if self.source.get(self.at) != Some(&b'=') {
                    return Err(format!("expected = after key at byte {}", self.at));
                }
                self.at += 1;
                let value = self.value()?;
                entries.push((Some(key_text(&key)?), value));
            } else if let Some(name) = self.peek_identifier() {
                let save = self.at;
                self.at += name.len();
                self.skip();
                if self.source.get(self.at) == Some(&b'=') {
                    self.at += 1;
                    let value = self.value()?;
                    entries.push((Some(name), value));
                } else {
                    self.at = save;
                    entries.push((None, self.value()?));
                }
            } else {
                entries.push((None, self.value()?));
            }
            self.skip();
            match self.source.get(self.at) {
                Some(b',') | Some(b';') => self.at += 1,
                Some(b'}') => {}
                _ => return Err(format!("expected , or }} at byte {}", self.at)),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        let quote = self.source[self.at];
        self.at += 1;
        let mut out = String::new();
        while let Some(&byte) = self.source.get(self.at) {
            self.at += 1;
            match byte {
                b'\\' => {
                    let escaped = *self
                        .source
                        .get(self.at)
                        .ok_or("unterminated string escape")?;
                    self.at += 1;
                    out.push(match escaped {
                        b'n' => '\n',
                        b't' => '\t',
                        b'\\' => '\\',
                        b'"' => '"',
                        b'\'' => '\'',
                        other => return Err(format!("unknown escape \\{}", other as char)),
                    });
                }
                b if b == quote => return Ok(out),
                b if b.is_ascii() => out.push(b as char),
                _ => {
                    let rest = std::str::from_utf8(&self.source[self.at - 1..])
                        .map_err(|_| "string is not UTF-8")?;
                    let ch = rest.chars().next().ok_or("unterminated string")?;
                    out.push(ch);
                    self.at += ch.len_utf8() - 1;
                }
            }
        }
        Err("unterminated string".into())
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.at;
        if self.source.get(self.at) == Some(&b'-') {
            self.at += 1;
        }
        while let Some(&byte) = self.source.get(self.at) {
            if !(byte.is_ascii_digit() || matches!(byte, b'.' | b'e' | b'E' | b'+' | b'-')) {
                break;
            }
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.source[start..self.at]).map_err(|_| "bad number")?;
        text.parse::<f64>()
            .map(Value::Number)
            .map_err(|_| format!("invalid number {text:?}"))
    }

    fn identifier(&mut self) -> Result<String, String> {
        let start = self.at;
        while let Some(&byte) = self.source.get(self.at) {
            if !(byte.is_ascii_alphanumeric() || byte == b'_') {
                break;
            }
            self.at += 1;
        }
        if start == self.at {
            return Err(format!("expected identifier at byte {}", self.at));
        }
        String::from_utf8(self.source[start..self.at].to_vec())
            .map_err(|_| "identifier is not UTF-8".into())
    }

    fn peek_identifier(&self) -> Option<String> {
        if !self.source.get(self.at)?.is_ascii_alphabetic()
            && self.source.get(self.at) != Some(&b'_')
        {
            return None;
        }
        let start = self.at;
        let mut end = self.at;
        while let Some(&byte) = self.source.get(end) {
            if !(byte.is_ascii_alphanumeric() || byte == b'_') {
                break;
            }
            end += 1;
        }
        std::str::from_utf8(&self.source[start..end])
            .ok()
            .map(str::to_string)
    }
}

fn key_text(value: &Value) -> Result<String, String> {
    match value {
        Value::Str(text) => Ok(text.clone()),
        Value::Number(number) => Ok(number.to_string()),
        _ => Err("table keys must be strings or numbers".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_pad_shape() {
        let value = parse(
            r#"
            -- a comment
            return {
              version = 1,
              pads = {
                { id = "blip", ms = 80, partials = { { freq = 523.25, gain = 1 } } },
              },
            }
            "#,
        )
        .unwrap();
        let pads = value.field("pads").unwrap().array().unwrap();
        assert_eq!(pads.len(), 1);
        assert_eq!(pads[0].field("id"), Some(&Value::Str("blip".into())));
        assert_eq!(pads[0].field("ms"), Some(&Value::Number(80.0)));
    }

    #[test]
    fn rejects_code() {
        assert!(parse("return { os.execute('x') }").is_err());
    }
}
