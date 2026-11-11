//! A small reader for device description files: the subset of TOML they need, and JSON.
//!
//! TOML: `key = value`, `[table]`, `[[array of tables]]`, comments, strings ("…" / '…'),
//! integers (decimal, 0x hex, `_` separators), booleans, flat arrays (also over several lines).
//! JSON: objects, arrays, strings, numbers, booleans, null.
//! Both give the same `Value` tree, so one converter handles both formats.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
    Array(Vec<Value>),
    Table(BTreeMap<String, Value>),
    Null,
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Table(t) => t.get(key),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            // "0x008F" written as a string is accepted too
            Value::Str(s) => parse_int(s),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }
    pub fn keys(&self) -> Vec<&str> {
        match self {
            Value::Table(t) => t.keys().map(|k| k.as_str()).collect(),
            _ => Vec::new(),
        }
    }
}

fn parse_int(s: &str) -> Option<i64> {
    let t: String = s.trim().chars().filter(|c| *c != '_').collect();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r.to_string()),
        None => (false, t),
    };
    let v = if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        i64::from_str_radix(h, 16).ok()?
    } else {
        t.parse::<i64>().ok()?
    };
    Some(if neg { -v } else { v })
}

/// Error with a line number.
fn err(line: usize, msg: impl std::fmt::Display) -> String {
    format!("line {line}: {msg}")
}

// ------------------------------------------------------------------ TOML subset

pub fn parse_toml(text: &str) -> Result<Value, String> {
    let mut root: BTreeMap<String, Value> = BTreeMap::new();
    // where `key = value` lines go: None = root, Some((name, is_array))
    let mut section: Option<(String, bool)> = None;
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let n = i + 1;
        let mut line = strip_comment(lines[i]).trim().to_string();
        i += 1;
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix("[[").and_then(|l| l.strip_suffix("]]")) {
            let name = name.trim().to_string();
            check_key(&name, n)?;
            match root.entry(name.clone()).or_insert_with(|| Value::Array(Vec::new())) {
                Value::Array(a) => a.push(Value::Table(BTreeMap::new())),
                _ => return Err(err(n, format!("“{name}” is already a value"))),
            }
            section = Some((name, true));
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = name.trim().to_string();
            check_key(&name, n)?;
            if root.contains_key(&name) {
                return Err(err(n, format!("table “{name}” is defined twice")));
            }
            root.insert(name.clone(), Value::Table(BTreeMap::new()));
            section = Some((name, false));
            continue;
        }
        let Some(eq) = line.find('=') else {
            return Err(err(n, "expected `key = value`"));
        };
        // an array may continue on the next lines
        while bracket_depth(&line[eq + 1..]) > 0 && i < lines.len() {
            line.push(' ');
            line.push_str(strip_comment(lines[i]).trim());
            i += 1;
        }
        let key = line[..eq].trim().trim_matches('"').to_string();
        check_key(&key, n)?;
        let (value, rest) = parse_toml_value(line[eq + 1..].trim(), n)?;
        if !rest.trim().is_empty() {
            return Err(err(n, format!("unexpected text after the value: “{}”", rest.trim())));
        }
        let table = match &section {
            None => &mut root,
            Some((name, is_array)) => match root.get_mut(name) {
                Some(Value::Table(t)) if !is_array => t,
                Some(Value::Array(a)) => match a.last_mut() {
                    Some(Value::Table(t)) => t,
                    _ => return Err(err(n, "internal: no table")),
                },
                _ => return Err(err(n, "internal: no section")),
            },
        };
        if table.insert(key.clone(), value).is_some() {
            return Err(err(n, format!("“{key}” is set twice")));
        }
    }
    Ok(Value::Table(root))
}

fn check_key(k: &str, n: usize) -> Result<(), String> {
    if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(err(n, format!("bad key “{k}”")));
    }
    Ok(())
}

/// Drop a `#` comment that is not inside a string.
fn strip_comment(l: &str) -> &str {
    let mut quote: Option<char> = None;
    for (i, c) in l.char_indices() {
        match (quote, c) {
            (None, '#') => return &l[..i],
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            _ => {}
        }
    }
    l
}

fn bracket_depth(s: &str) -> i32 {
    let mut d = 0;
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match (quote, c) {
            (None, '[') => d += 1,
            (None, ']') => d -= 1,
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            _ => {}
        }
    }
    d
}

fn parse_toml_value(s: &str, n: usize) -> Result<(Value, &str), String> {
    let s = s.trim_start();
    if let Some(r) = s.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = r.char_indices();
        while let Some((i, c)) = chars.next() {
            match c {
                '"' => return Ok((Value::Str(out), &r[i + 1..])),
                '\\' => match chars.next() {
                    Some((_, 'n')) => out.push('\n'),
                    Some((_, 't')) => out.push('\t'),
                    Some((_, c)) => out.push(c),
                    None => break,
                },
                c => out.push(c),
            }
        }
        return Err(err(n, "string is not closed"));
    }
    if let Some(r) = s.strip_prefix('\'') {
        let end = r.find('\'').ok_or_else(|| err(n, "string is not closed"))?;
        return Ok((Value::Str(r[..end].to_string()), &r[end + 1..]));
    }
    if let Some(mut r) = s.strip_prefix('[') {
        let mut items = Vec::new();
        loop {
            r = r.trim_start();
            if let Some(rest) = r.strip_prefix(']') {
                return Ok((Value::Array(items), rest));
            }
            let (v, rest) = parse_toml_value(r, n)?;
            items.push(v);
            r = rest.trim_start();
            if let Some(rest) = r.strip_prefix(',') {
                r = rest;
            } else if !r.starts_with(']') {
                return Err(err(n, "expected `,` or `]` in the array"));
            }
        }
    }
    let end = s.find(|c: char| c == ',' || c == ']' || c.is_whitespace()).unwrap_or(s.len());
    let word = &s[..end];
    let v = match word {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        w => Value::Int(parse_int(w).ok_or_else(|| err(n, format!("not a value: “{w}”")))?),
    };
    Ok((v, &s[end..]))
}

// ------------------------------------------------------------------ JSON

pub fn parse_json(text: &str) -> Result<Value, String> {
    let mut p = Json { s: text.as_bytes(), i: 0 };
    let v = p.value()?;
    p.ws();
    if p.i != p.s.len() {
        return Err(p.err("unexpected text after the end"));
    }
    Ok(v)
}

struct Json<'a> {
    s: &'a [u8],
    i: usize,
}

impl Json<'_> {
    fn err(&self, msg: &str) -> String {
        let line = self.s[..self.i.min(self.s.len())].iter().filter(|b| **b == b'\n').count() + 1;
        err(line, msg)
    }
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        match self.s.get(self.i) {
            Some(b'{') => {
                self.i += 1;
                let mut t = BTreeMap::new();
                if self.eat(b'}') {
                    return Ok(Value::Table(t));
                }
                loop {
                    self.ws();
                    let k = match self.value()? {
                        Value::Str(k) => k,
                        _ => return Err(self.err("object key must be a string")),
                    };
                    if !self.eat(b':') {
                        return Err(self.err("expected `:`"));
                    }
                    let v = self.value()?;
                    t.insert(k, v);
                    if self.eat(b'}') {
                        return Ok(Value::Table(t));
                    }
                    if !self.eat(b',') {
                        return Err(self.err("expected `,` or `}`"));
                    }
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut a = Vec::new();
                if self.eat(b']') {
                    return Ok(Value::Array(a));
                }
                loop {
                    a.push(self.value()?);
                    if self.eat(b']') {
                        return Ok(Value::Array(a));
                    }
                    if !self.eat(b',') {
                        return Err(self.err("expected `,` or `]`"));
                    }
                }
            }
            Some(b'"') => {
                self.i += 1;
                let mut out = Vec::new();
                while let Some(&c) = self.s.get(self.i) {
                    self.i += 1;
                    match c {
                        b'"' => return String::from_utf8(out).map(Value::Str).map_err(|_| self.err("bad UTF-8")),
                        b'\\' => {
                            let e = self.s.get(self.i).copied().unwrap_or(b'"');
                            self.i += 1;
                            match e {
                                b'n' => out.push(b'\n'),
                                b't' => out.push(b'\t'),
                                b'u' => {
                                    let h = self.s.get(self.i..self.i + 4).and_then(|h| std::str::from_utf8(h).ok());
                                    let c = h.and_then(|h| u32::from_str_radix(h, 16).ok()).and_then(char::from_u32);
                                    let c = c.ok_or_else(|| self.err("bad \\u escape"))?;
                                    self.i += 4;
                                    let mut b = [0u8; 4];
                                    out.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
                                }
                                e => out.push(e),
                            }
                        }
                        c => out.push(c),
                    }
                }
                Err(self.err("string is not closed"))
            }
            Some(_) => {
                let start = self.i;
                while self.i < self.s.len() && !b",]} \t\r\n".contains(&self.s[self.i]) {
                    self.i += 1;
                }
                let w = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("");
                Ok(match w {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    "null" => Value::Null,
                    w => match parse_int(w) {
                        Some(v) => Value::Int(v),
                        // fractions are not used in device files
                        None => return Err(self.err(&format!("not a value: “{w}”"))),
                    },
                })
            }
            None => Err(self.err("unexpected end")),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn toml_subset() {
        let v = parse_toml(
            r#"
# comment
pid = 0x00_8F   # hex with separator
name = "Razer \"Test\" # not a comment"
wireless = false
features = [
  "dpi", "polling",  # trailing comment
]
[extra]
x = -5
[[zones]]
id = 'scroll'
led = 1
[[zones]]
id = "logo"
"#,
        )
        .unwrap();
        assert_eq!(v.get("pid").unwrap().as_int(), Some(0x8F));
        assert_eq!(v.get("name").unwrap().as_str(), Some("Razer \"Test\" # not a comment"));
        assert_eq!(v.get("wireless").unwrap().as_bool(), Some(false));
        assert_eq!(v.get("features").unwrap().as_array().unwrap().len(), 2);
        assert_eq!(v.get("extra").unwrap().get("x").unwrap().as_int(), Some(-5));
        let z = v.get("zones").unwrap().as_array().unwrap();
        assert_eq!((z.len(), z[1].get("id").unwrap().as_str()), (2, Some("logo")));
    }

    #[test]
    fn toml_errors() {
        assert!(parse_toml("pid 5").unwrap_err().starts_with("line 1"));
        assert!(parse_toml("a = 1\na = 2").unwrap_err().contains("twice"));
        assert!(parse_toml("a = \"open").is_err());
        assert!(parse_toml("a = [1, 2").is_err());
        assert!(parse_toml("a = 1 2").is_err());
    }

    #[test]
    fn json() {
        let v = parse_json(r#"{"pid": "0x0090", "n": 26000, "ok": true, "z": [{"id": "logo!"}], "x": null}"#).unwrap();
        assert_eq!(v.get("pid").unwrap().as_int(), Some(0x90));
        assert_eq!(v.get("n").unwrap().as_int(), Some(26000));
        assert_eq!(v.get("z").unwrap().as_array().unwrap()[0].get("id").unwrap().as_str(), Some("logo!"));
        assert!(parse_json("{\"a\": 1,}").is_err());
        assert!(parse_json("{\"a\": 1} x").is_err());
    }
}
