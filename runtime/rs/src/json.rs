// json extern — JSON parse, stringify, and AlmideValue query/manipulation
// AlmideValue type is defined in value.rs (included before this file)

// ── Parse + Stringify ──

pub fn almide_rt_json_stringify(v: &AlmideValue) -> AlmideString { almide_rt_value_stringify(v) }

pub fn almide_rt_json_parse(text: &str) -> Result<AlmideValue, AlmideString> {
    let __almide_r: Result<AlmideValue, String> = (|| {
        // Byte-offset recursive descent (#1678). The first version collected the
        // input into a `Vec<char>` — 4 bytes per char plus a full decode pass
        // before the first token — and built every string one `push(char)` at a
        // time. The self-hosted transcription (`stdlib/json_parse.almd`) had
        // already moved to byte offsets; this brings the native oracle level with
        // it. The accepted language and every lenient edge are UNCHANGED: an
        // unterminated string yields what was read, an unknown escape consumes
        // itself and writes nothing, `\u` advances four chars unconditionally, a
        // lone surrogate is dropped, whitespace is `char::is_whitespace` (Unicode),
        // and an error reports the CHAR index of the offending position (ALS-T3)
        // — computed by one linear count at error time only.
        //
        // Nesting is bounded (R-L0-47, R-json-04, #4020): an array or object
        // opened inside 1000 others is `json: nesting deeper than 1000`, where
        // it used to recurse until the stack overflowed. `depth` counts the
        // containers around the value being parsed; stdlib/json_parse.almd
        // checks at the same point with the same text.
        let mut pos = 0usize;
        #[inline]
        fn char_len_at(t: &str, pos: usize) -> usize {
            t[pos..].chars().next().map_or(0, |c| c.len_utf8())
        }
        /// Advance one char (clamped at the end — the old parser overshot and every
        /// later probe is `pos < len`, so the two are observably the same).
        #[inline]
        fn step_char(t: &str, pos: &mut usize) { if *pos < t.len() { *pos += char_len_at(t, *pos); } }
        /// Advance up to `n` chars, clamped.
        #[inline]
        fn step_chars(t: &str, pos: &mut usize, n: usize) { for _ in 0..n { if *pos >= t.len() { break; } *pos += char_len_at(t, *pos); } }
        fn skip_ws(t: &str, pos: &mut usize) {
            let b = t.as_bytes();
            while *pos < b.len() {
                let c = b[*pos];
                if c < 0x80 {
                    if (c as char).is_whitespace() { *pos += 1; } else { break; }
                } else {
                    match t[*pos..].chars().next() {
                        Some(ch) if ch.is_whitespace() => *pos += ch.len_utf8(),
                        _ => break,
                    }
                }
            }
        }
        const JSON_MAX_DEPTH: usize = 1000;
        fn parse_value(t: &str, pos: &mut usize, depth: usize) -> Result<AlmideValue, String> {
            skip_ws(t, pos);
            let b = t.as_bytes();
            if *pos >= b.len() { return Err("unexpected end of input".into()); }
            match b[*pos] {
                b'"' => parse_str_value(t, pos),
                b'{' | b'[' if depth >= JSON_MAX_DEPTH => Err("json: nesting deeper than 1000".into()),
                b'{' => parse_object(t, pos, depth + 1),
                b'[' => parse_array(t, pos, depth + 1),
                b't' | b'f' => parse_bool(t, pos),
                b'n' => parse_null(t, pos),
                c if c == b'-' || c.is_ascii_digit() => parse_number(t, pos),
                _ => {
                    let ch = t[*pos..].chars().next().unwrap_or('\u{FFFD}');
                    Err(format!("unexpected char '{}' at pos {}", ch, t[..*pos].chars().count()))
                }
            }
        }
        fn parse_string(t: &str, pos: &mut usize) -> Result<String, String> {
            let b = t.as_bytes();
            // Skip the opening quote — or whatever char is here: object keys are
            // hunted through this path unconditionally (`{x: 1}`), as before.
            step_char(t, pos);
            let mut s = String::new();
            loop {
                // Fast path: copy the run up to the next quote or backslash as one
                // slice. Both delimiters are ASCII, so the run ends on a char boundary.
                let start = *pos;
                while *pos < b.len() && b[*pos] != b'"' && b[*pos] != b'\\' { *pos += 1; }
                if *pos > start { s.push_str(&t[start..*pos]); }
                if *pos >= b.len() { return Ok(s); }
                if b[*pos] == b'"' { *pos += 1; return Ok(s); }
                *pos += 1; // past the backslash
                parse_escape(t, pos, &mut s);
            }
        }
        /// The escape after a backslash: a one-char escape, `\u` and four hex digits, an unknown
        /// escape (consumed, writes nothing), or nothing at end of input.
        fn parse_escape(t: &str, pos: &mut usize, s: &mut String) {
            match t.as_bytes().get(*pos) {
                Some(b'u') => { *pos += 1; parse_unicode_escape(t, pos, s); }
                Some(&c) => match simple_escape(c) {
                    Some(ch) => { s.push(ch); *pos += 1; }
                    None => step_char(t, pos),
                },
                None => {}
            }
        }
        fn simple_escape(c: u8) -> Option<char> {
            Some(match c {
                b'n' => '\n', b't' => '\t', b'r' => '\r', b'b' => '\u{0008}', b'f' => '\u{000c}',
                b'"' => '"', b'\\' => '\\', b'/' => '/',
                _ => return None,
            })
        }
        /// Four chars of hex as a UTF-16 unit; a high surrogate (D800..=DBFF)
        /// immediately followed by a "\uYYYY" low surrogate (DC00..=DFFF) joins
        /// into one astral code point, matching serde_json and the wasm parser.
        /// #651. The four chars are consumed whether or not they were hex.
        fn parse_unicode_escape(t: &str, pos: &mut usize, s: &mut String) {
            let b = t.as_bytes();
            let hs = *pos;
            step_chars(t, pos, 4);
            let hex = &t[hs..*pos];
            if hex.len() != 4 { return; }
            let Ok(unit) = u32::from_str_radix(hex, 16) else { return };
            let low_follows = b.get(*pos) == Some(&b'\\') && b.get(*pos + 1) == Some(&b'u');
            if !(0xD800..=0xDBFF).contains(&unit) || !low_follows {
                if let Some(ch) = char::from_u32(unit) { s.push(ch); }
                return;
            }
            let ls = *pos + 2;
            let mut le = ls;
            step_chars(t, &mut le, 4);
            let Ok(low) = u32::from_str_radix(&t[ls..le], 16) else { return };
            if !(0xDC00..=0xDFFF).contains(&low) { return; }
            let cp = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
            if let Some(ch) = char::from_u32(cp) { s.push(ch); }
            *pos += 6; // consumed "\uYYYY"
        }
        /// A string value: the escape-free run becomes the `AlmideString`
        /// straight from the input slice (#2157) — one allocation, where the
        /// general path builds a `String` and then copies it into a new block.
        /// Anything with an escape takes the general path, as `parse_key` does.
        fn parse_str_value(t: &str, pos: &mut usize) -> Result<AlmideValue, String> {
            let b = t.as_bytes();
            let start = *pos + 1;
            let mut i = start;
            while i < b.len() && b[i] != b'"' && b[i] != b'\\' { i += 1; }
            if i < b.len() && b[i] == b'"' {
                *pos = i + 1;
                return Ok(AlmideValue::Str(AlmideString::from(&t[start..i])));
            }
            parse_string(t, pos).map(|s| AlmideValue::Str(AlmideString::from(s)))
        }
        /// An object key: the escape-free run is interned straight from the input
        /// slice (no `String` in between); anything with an escape takes the
        /// general string path and is interned from its result.
        fn parse_key(t: &str, pos: &mut usize) -> Result<AlmideKey, String> {
            let b = t.as_bytes();
            let save = *pos;
            step_char(t, pos);
            let start = *pos;
            let mut i = start;
            while i < b.len() && b[i] != b'"' && b[i] != b'\\' { i += 1; }
            if i < b.len() && b[i] == b'"' {
                *pos = i + 1;
                return Ok(almide_rt_intern_key(&t[start..i]));
            }
            *pos = save;
            parse_string(t, pos).map(|s| almide_rt_intern_key(&s))
        }
        fn parse_number(t: &str, pos: &mut usize) -> Result<AlmideValue, String> {
            let b = t.as_bytes();
            let start = *pos;
            if b[*pos] == b'-' { *pos += 1; }
            while *pos < b.len() && b[*pos].is_ascii_digit() { *pos += 1; }
            let mut is_float = false;
            if *pos < b.len() && b[*pos] == b'.' {
                is_float = true; *pos += 1;
                while *pos < b.len() && b[*pos].is_ascii_digit() { *pos += 1; }
            }
            if *pos < b.len() && (b[*pos] == b'e' || b[*pos] == b'E') {
                is_float = true; *pos += 1;
                if *pos < b.len() && (b[*pos] == b'+' || b[*pos] == b'-') { *pos += 1; }
                while *pos < b.len() && b[*pos].is_ascii_digit() { *pos += 1; }
            }
            let s = &t[start..*pos];
            if is_float { s.parse::<f64>().map(AlmideValue::Float).map_err(|e| e.to_string()) }
            else { s.parse::<i64>().map(AlmideValue::Int).map_err(|e| e.to_string()) }
        }
        fn parse_bool(t: &str, pos: &mut usize) -> Result<AlmideValue, String> {
            let rest = &t.as_bytes()[*pos..];
            if rest.starts_with(b"true") { *pos += 4; Ok(AlmideValue::Bool(true)) }
            else if rest.starts_with(b"false") { *pos += 5; Ok(AlmideValue::Bool(false)) }
            else { Err("expected bool".into()) }
        }
        fn parse_null(t: &str, pos: &mut usize) -> Result<AlmideValue, String> {
            if t.as_bytes()[*pos..].starts_with(b"null") { *pos += 4; Ok(AlmideValue::Null) } else { Err("expected null".into()) }
        }
        fn parse_array(t: &str, pos: &mut usize, depth: usize) -> Result<AlmideValue, String> {
            let b = t.as_bytes();
            *pos += 1;
            skip_ws(t, pos);
            // Sized for the common small container: a Vec that grows from 0 reallocates
            // at 4 and 8 on the way to a typical 8-field object (#1678 follow-up, measured
            // 3-8% on the parse benchmark).
            let mut items = Vec::with_capacity(8);
            if *pos < b.len() && b[*pos] == b']' { *pos += 1; return Ok(AlmideValue::Array(items)); }
            loop {
                items.push(parse_value(t, pos, depth)?);
                skip_ws(t, pos);
                if *pos < b.len() && b[*pos] == b',' { *pos += 1; skip_ws(t, pos); } else { break; }
            }
            skip_ws(t, pos);
            if *pos < b.len() && b[*pos] == b']' { *pos += 1; }
            Ok(AlmideValue::Array(items))
        }
        fn parse_object(t: &str, pos: &mut usize, depth: usize) -> Result<AlmideValue, String> {
            let b = t.as_bytes();
            *pos += 1;
            skip_ws(t, pos);
            let mut pairs = Vec::with_capacity(8);
            if *pos < b.len() && b[*pos] == b'}' { *pos += 1; return Ok(AlmideValue::Object(pairs)); }
            loop {
                skip_ws(t, pos);
                let key = parse_key(t, pos)?;
                skip_ws(t, pos);
                if *pos < b.len() && b[*pos] == b':' { *pos += 1; }
                let val = parse_value(t, pos, depth)?;
                pairs.push((key, val));
                skip_ws(t, pos);
                if *pos < b.len() && b[*pos] == b',' { *pos += 1; } else { break; }
            }
            skip_ws(t, pos);
            if *pos < b.len() && b[*pos] == b'}' { *pos += 1; }
            Ok(AlmideValue::Object(pairs))
        }
        parse_value(text, &mut pos, 0)
    })();
    __almide_r.map_err(Into::into)
}

// ── Key-based getters ──

pub fn almide_json_get(j: &AlmideValue, key: &str) -> Option<AlmideValue> {
    match j {
        AlmideValue::Object(entries) => entries.iter().find(|(k, _)| k.as_ref() == key).map(|(_, v)| v.clone()),
        _ => None,
    }
}

pub fn almide_json_get_string(j: &AlmideValue, key: &str) -> Option<AlmideString> { let __almide_v: Option<AlmideString> = (|| {
    match almide_json_get(j, key)? { AlmideValue::Str(s) => Some(s), _ => None }
})(); __almide_v.into_almide() }

/// An Int field only: a Float value is `None`, even an integral one (`3.0`),
/// never truncated — the same answer as `value.as_int` and Codec Int decode, and
/// the self-hosted `json_get_int` the wasm legs link (#2476). The widening runs
/// one way only: `get_float` accepts an Int (C-085).
pub fn almide_json_get_int(j: &AlmideValue, key: &str) -> Option<i64> {
    match almide_json_get(j, key)? { AlmideValue::Int(n) => Some(n), _ => None }
}

pub fn almide_json_get_float(j: &AlmideValue, key: &str) -> Option<f64> {
    match almide_json_get(j, key)? { AlmideValue::Float(f) => Some(f), AlmideValue::Int(n) => Some(n as f64), _ => None }
}

pub fn almide_json_get_bool(j: &AlmideValue, key: &str) -> Option<bool> {
    match almide_json_get(j, key)? { AlmideValue::Bool(b) => Some(b), _ => None }
}

pub fn almide_json_get_array(j: &AlmideValue, key: &str) -> Option<Vec<AlmideValue>> {
    match almide_json_get(j, key)? { AlmideValue::Array(a) => Some(a), _ => None }
}

// ── Keyless extractors ──

pub fn almide_json_to_string(j: &AlmideValue) -> Option<AlmideString> { let __almide_v: Option<AlmideString> = (|| {
    match j { AlmideValue::Str(s) => Some(s.clone()), _ => None }
})(); __almide_v.into_almide() }

pub fn almide_json_to_int(j: &AlmideValue) -> Option<i64> {
    match j { AlmideValue::Int(n) => Some(*n), _ => None }
}

pub fn almide_json_as_float(j: &AlmideValue) -> Option<f64> {
    match j { AlmideValue::Float(f) => Some(*f), AlmideValue::Int(n) => Some(*n as f64), _ => None }
}

pub fn almide_json_as_bool(j: &AlmideValue) -> Option<bool> {
    match j { AlmideValue::Bool(b) => Some(*b), _ => None }
}

pub fn almide_json_as_array(j: &AlmideValue) -> Option<Vec<AlmideValue>> {
    match j { AlmideValue::Array(a) => Some(a.clone()), _ => None }
}

// ── Object operations ──

pub fn almide_json_keys(j: &AlmideValue) -> Vec<AlmideString> { let __almide_v: Vec<String> = (|| {
    match j { AlmideValue::Object(entries) => entries.iter().map(|(k, _)| k.to_string()).collect(), _ => vec![] }
})(); __almide_v.into_almide() }

pub fn almide_json_to_map(j: &AlmideValue) -> Option<AlmideMap<AlmideString, AlmideString>> { let __almide_v: Option<AlmideMap<String, String>> = (|| {
    match j {
        AlmideValue::Object(entries) => {
            let map: AlmideMap<String, String> = entries.iter().map(|(k, v)| {
                let s = match v {
                    AlmideValue::Str(s) => s.to_string(),
                    _ => String::from(almide_rt_value_stringify(v)),
                };
                (k.to_string(), s)
            }).collect();
            Some(map)
        }
        _ => None,
    }
})(); __almide_v.into_almide() }

pub fn almide_json_object(entries: &[(AlmideString, AlmideValue)]) -> AlmideValue {
    AlmideValue::Object(entries.iter().map(|(k, v)| (almide_rt_intern_key(k), v.clone())).collect())
}

pub fn almide_json_from_float(n: f64) -> AlmideValue { AlmideValue::Float(n) }
pub fn almide_json_from_string(s: &str) -> AlmideValue { AlmideValue::Str(AlmideString::from(s)) }
pub fn almide_json_from_int(n: i64) -> AlmideValue { AlmideValue::Int(n) }
pub fn almide_json_from_bool(b: bool) -> AlmideValue { AlmideValue::Bool(b) }
pub fn almide_json_array(items: &[AlmideValue]) -> AlmideValue { AlmideValue::Array(items.to_vec()) }

// ── Stringify pretty ──

pub fn almide_json_stringify_pretty(j: &AlmideValue) -> AlmideString { let __almide_v: String = (|| {
    stringify_value(j, 0)
})(); __almide_v.into_almide() }

/// JSON string quoting is `almide_rt_value_json_quote` (value.rs) — the one
/// RFC 8259 §7 rule every serializer shares (#2802). The previous `{:?}`
/// (Rust `escape_debug`) escaped combining marks as `\u{301}` — not valid
/// JSON escaping, and a byte divergence against the wasm leg.
fn json_quote(s: &str) -> String {
    String::from(almide_rt_value_json_quote(s))
}

fn stringify_value(v: &AlmideValue, depth: usize) -> String {
    let ind = "  ".repeat(depth);
    let ind1 = "  ".repeat(depth + 1);
    match v {
        AlmideValue::Null => "null".into(),
        AlmideValue::Bool(b) => if *b { "true" } else { "false" }.into(),
        AlmideValue::Int(n) => n.to_string(),
        AlmideValue::Float(f) => String::from(almide_rt_value_float_json(*f)),
        AlmideValue::Str(s) => json_quote(s),
        AlmideValue::Array(items) => {
            if items.is_empty() { return "[]".into(); }
            let parts: Vec<String> = items.iter().map(|v| format!("{}{}", ind1, stringify_value(v, depth + 1))).collect();
            format!("[\n{}\n{}]", parts.join(",\n"), ind)
        }
        AlmideValue::Object(entries) => {
            if entries.is_empty() { return "{}".into(); }
            let parts: Vec<String> = entries.iter().map(|(k, v)| format!("{}{}: {}", ind1, json_quote(k), stringify_value(v, depth + 1))).collect();
            format!("{{\n{}\n{}}}", parts.join(",\n"), ind)
        }
    }
}

// ── AlmideJsonPath type and operations ──


#[derive(Debug, Clone, PartialEq)]
pub enum AlmideJsonPath {
    JpRoot,
    JpField(std::boxed::Box<AlmideJsonPath>, String),
    JpIndex(std::boxed::Box<AlmideJsonPath>, i64),
}

impl std::fmt::Display for AlmideJsonPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlmideJsonPath::JpRoot => write!(f, "$"),
            AlmideJsonPath::JpField(parent, name) => write!(f, "{}.{}", parent, name),
            AlmideJsonPath::JpIndex(parent, i) => write!(f, "{}[{}]", parent, i),
        }
    }
}

// A record may hold a path, and a record's repr calls each field's (#2647).
impl AlmideRepr for AlmideJsonPath {
    fn almide_repr(&self) -> String { self.to_string() }
}

// Wrapper functions for stdlib codegen (json.root(), json.field(), json.index())
pub fn almide_rt_json_root() -> AlmideJsonPath { AlmideJsonPath::JpRoot }
pub fn almide_rt_json_field(path: AlmideJsonPath, name: &str) -> AlmideJsonPath { AlmideJsonPath::JpField(std::boxed::Box::new(path), name.to_string()) }
pub fn almide_rt_json_index(path: AlmideJsonPath, i: i64) -> AlmideJsonPath { AlmideJsonPath::JpIndex(std::boxed::Box::new(path), i) }

/// Resolve a AlmideJsonPath to a list of traversal steps, root-first.
fn resolve_path(path: &AlmideJsonPath) -> Vec<AlmidePathStep> {
    let mut steps = Vec::new();
    let mut current = path;
    loop {
        match current {
            AlmideJsonPath::JpRoot => break,
            AlmideJsonPath::JpField(parent, name) => {
                steps.push(AlmidePathStep::Field(name.clone()));
                current = parent;
            }
            AlmideJsonPath::JpIndex(parent, i) => {
                steps.push(AlmidePathStep::Index(*i));
                current = parent;
            }
        }
    }
    steps.reverse();
    steps
}

enum AlmidePathStep {
    Field(String),
    Index(i64),
}

fn get_by_step(v: &AlmideValue, step: &AlmidePathStep) -> Option<AlmideValue> {
    match step {
        AlmidePathStep::Field(key) => almide_json_get(v, key),
        AlmidePathStep::Index(i) => match v {
            AlmideValue::Array(items) => {
                let idx = if *i < 0 { items.len() as i64 + *i } else { *i } as usize;
                items.get(idx).cloned()
            }
            _ => None,
        },
    }
}

pub fn almide_json_get_path(j: &AlmideValue, path: &AlmideJsonPath) -> Option<AlmideValue> {
    let steps = resolve_path(path);
    let mut current = j.clone();
    for step in &steps {
        current = get_by_step(&current, step)?;
    }
    Some(current)
}

pub fn almide_json_set_path(j: &AlmideValue, path: &AlmideJsonPath, value: AlmideValue) -> Result<AlmideValue, AlmideString> { let __almide_r: Result<AlmideValue, String> = (|| {
    let steps = resolve_path(path);
    Ok(set_at_steps(j, &steps, &value))
})(); __almide_r.map_err(Into::into) }

pub fn almide_json_remove_path(j: &AlmideValue, path: &AlmideJsonPath) -> AlmideValue {
    let steps = resolve_path(path);
    remove_at_steps(j, &steps)
}

fn set_at_steps(j: &AlmideValue, steps: &[AlmidePathStep], value: &AlmideValue) -> AlmideValue {
    if steps.is_empty() { return value.clone(); }
    match &steps[0] {
        AlmidePathStep::Field(key) => match j {
            AlmideValue::Object(entries) => {
                let rest = &steps[1..];
                let mut new_entries: Vec<(AlmideKey, AlmideValue)> = entries.iter()
                    .map(|(k, v)| if k.as_ref() == key.as_str() { (k.clone(), set_at_steps(v, rest, value)) } else { (k.clone(), v.clone()) })
                    .collect();
                if !entries.iter().any(|(k, _)| k.as_ref() == key.as_str()) {
                    new_entries.push((almide_rt_intern_key(key), set_at_steps(&AlmideValue::Object(vec![]), rest, value)));
                }
                AlmideValue::Object(new_entries)
            }
            _ => AlmideValue::Object(vec![(almide_rt_intern_key(key), set_at_steps(&AlmideValue::Object(vec![]), &steps[1..], value))]),
        },
        AlmidePathStep::Index(i) => match j {
            AlmideValue::Array(items) => {
                let idx = if *i < 0 { items.len() as i64 + *i } else { *i } as usize;
                let mut new_items = items.clone();
                if idx < new_items.len() {
                    new_items[idx] = set_at_steps(&new_items[idx], &steps[1..], value);
                }
                AlmideValue::Array(new_items)
            }
            _ => j.clone(),
        },
    }
}

fn remove_at_steps(j: &AlmideValue, steps: &[AlmidePathStep]) -> AlmideValue {
    if steps.is_empty() { return AlmideValue::Null; }
    match &steps[0] {
        AlmidePathStep::Field(key) => match j {
            AlmideValue::Object(entries) => {
                if steps.len() == 1 {
                    AlmideValue::Object(entries.iter().filter(|(k, _)| k.as_ref() != key.as_str()).cloned().collect())
                } else {
                    AlmideValue::Object(entries.iter().map(|(k, v)| {
                        if k.as_ref() == key.as_str() { (k.clone(), remove_at_steps(v, &steps[1..])) } else { (k.clone(), v.clone()) }
                    }).collect())
                }
            }
            other => other.clone(),
        },
        AlmidePathStep::Index(i) => match j {
            AlmideValue::Array(items) => {
                let idx = if *i < 0 { items.len() as i64 + *i } else { *i } as usize;
                if steps.len() == 1 {
                    AlmideValue::Array(items.iter().enumerate().filter(|(ii, _)| *ii != idx).map(|(_, v)| v.clone()).collect())
                } else {
                    let mut new_items = items.clone();
                    if idx < new_items.len() {
                        new_items[idx] = remove_at_steps(&new_items[idx], &steps[1..]);
                    }
                    AlmideValue::Array(new_items)
                }
            }
            other => other.clone(),
        },
    }
}
