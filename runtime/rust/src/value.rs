//! Value — universal data model for Codec protocol
//!
//! Used by auto-derived encode/decode functions.

use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

// ── Construction ──

pub fn value_str(s: String) -> Value { Value::Str(s) }
pub fn value_int(n: i64) -> Value { Value::Int(n) }
pub fn value_float(f: f64) -> Value { Value::Float(f) }
pub fn value_bool(b: bool) -> Value { Value::Bool(b) }
pub fn value_array(items: Vec<Value>) -> Value { Value::Array(items) }
pub fn value_object(pairs: Vec<(String, Value)>) -> Value { Value::Object(pairs) }
pub fn value_null() -> Value { Value::Null }

// ── Access ──

pub fn value_field(v: Value, key: String) -> Result<Value, String> {
    if let Value::Object(pairs) = v {
        for (k, val) in pairs {
            if k == key { return Ok(val); }
        }
        Err(format!("missing field '{}'", key))
    } else {
        Err("expected Object".to_string())
    }
}

pub fn value_as_string(v: Value) -> Result<String, String> {
    match v { Value::Str(s) => Ok(s), _ => Err("expected Str".to_string()) }
}
pub fn value_as_int(v: Value) -> Result<i64, String> {
    match v { Value::Int(n) => Ok(n), _ => Err("expected Int".to_string()) }
}
pub fn value_as_float(v: Value) -> Result<f64, String> {
    match v { Value::Float(f) => Ok(f), _ => Err("expected Float".to_string()) }
}
pub fn value_as_bool(v: Value) -> Result<bool, String> {
    match v { Value::Bool(b) => Ok(b), _ => Err("expected Bool".to_string()) }
}
pub fn value_as_array(v: Value) -> Result<Vec<Value>, String> {
    match v { Value::Array(a) => Ok(a), _ => Err("expected Array".to_string()) }
}

// ── List encode/decode ──

pub fn value_encode_list<T, F: Fn(T) -> Value>(items: Vec<T>, f: F) -> Value {
    Value::Array(items.into_iter().map(f).collect())
}

pub fn value_decode_list<T, F: Fn(Value) -> Result<T, String>>(v: Value, f: F) -> Result<Vec<T>, String> {
    match v {
        Value::Array(items) => items.into_iter().map(f).collect(),
        _ => Err("expected Array".to_string()),
    }
}

// ── Option encode/decode ──

pub fn value_option_encode<T, F: Fn(T) -> Value>(opt: Option<T>, f: F) -> Value {
    match opt { Some(v) => f(v), None => Value::Null }
}

pub fn value_decode_option<T, F: Fn(Value) -> Result<T, String>>(v: &Value, key: &str, f: F) -> Result<Option<T>, String> {
    match value_field(v.clone(), key.to_string()) {
        Ok(Value::Null) => Ok(None),
        Ok(val) => f(val).map(Some),
        Err(_) => Ok(None),
    }
}

pub fn value_decode_with_default<T: Clone, F: Fn(Value) -> Result<T, String>>(v: &Value, key: &str, default: T, f: F) -> Result<T, String> {
    match value_field(v.clone(), key.to_string()) {
        Ok(Value::Null) => Ok(default),
        Ok(val) => f(val),
        Err(_) => Ok(default),
    }
}

// ── Concrete helpers (called from auto-derive codegen) ──

pub fn __encode_list_string(items: Vec<String>) -> Value { value_encode_list(items, value_str) }
pub fn __encode_list_int(items: Vec<i64>) -> Value { value_encode_list(items, value_int) }
pub fn __encode_list_float(items: Vec<f64>) -> Value { value_encode_list(items, value_float) }
pub fn __encode_list_bool(items: Vec<bool>) -> Value { value_encode_list(items, value_bool) }

pub fn __decode_list_string(v: Value) -> Result<Vec<String>, String> { value_decode_list(v, value_as_string) }
pub fn __decode_list_int(v: Value) -> Result<Vec<i64>, String> { value_decode_list(v, value_as_int) }
pub fn __decode_list_float(v: Value) -> Result<Vec<f64>, String> { value_decode_list(v, value_as_float) }
pub fn __decode_list_bool(v: Value) -> Result<Vec<bool>, String> { value_decode_list(v, value_as_bool) }

pub fn __encode_option_string(v: Option<String>) -> Value { value_option_encode(v, value_str) }
pub fn __encode_option_int(v: Option<i64>) -> Value { value_option_encode(v, value_int) }
pub fn __encode_option_float(v: Option<f64>) -> Value { value_option_encode(v, value_float) }
pub fn __encode_option_bool(v: Option<bool>) -> Value { value_option_encode(v, value_bool) }

pub fn __decode_option_string(v: Value, key: String) -> Result<Option<String>, String> { value_decode_option(&v, &key, value_as_string) }
pub fn __decode_option_int(v: Value, key: String) -> Result<Option<i64>, String> { value_decode_option(&v, &key, value_as_int) }
pub fn __decode_option_float(v: Value, key: String) -> Result<Option<f64>, String> { value_decode_option(&v, &key, value_as_float) }
pub fn __decode_option_bool(v: Value, key: String) -> Result<Option<bool>, String> { value_decode_option(&v, &key, value_as_bool) }

pub fn __decode_default_string(v: Value, key: String, default: String) -> Result<String, String> { value_decode_with_default(&v, &key, default, value_as_string) }
pub fn __decode_default_int(v: Value, key: String, default: i64) -> Result<i64, String> { value_decode_with_default(&v, &key, default, value_as_int) }
pub fn __decode_default_float(v: Value, key: String, default: f64) -> Result<f64, String> { value_decode_with_default(&v, &key, default, value_as_float) }
pub fn __decode_default_bool(v: Value, key: String, default: bool) -> Result<bool, String> { value_decode_with_default(&v, &key, default, value_as_bool) }

// ── Stringify ──

pub fn value_stringify(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        Value::Int(n) => n.to_string(),
        Value::Float(f) => format!("{}", f),
        Value::Str(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(value_stringify).collect();
            format!("[{}]", inner.join(","))
        }
        Value::Object(pairs) => {
            let inner: Vec<String> = pairs.iter().map(|(k, v)| format!("\"{}\":{}", k, value_stringify(v))).collect();
            format!("{{{}}}", inner.join(","))
        }
    }
}

pub fn json_stringify(v: Value) -> String { value_stringify(&v) }

// ── JSON Parse ──

pub fn json_parse(text: String) -> Result<Value, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0;
    fn skip_ws(chars: &[char], pos: &mut usize) {
        while *pos < chars.len() && chars[*pos].is_whitespace() { *pos += 1; }
    }
    fn parse_value(chars: &[char], pos: &mut usize) -> Result<Value, String> {
        skip_ws(chars, pos);
        if *pos >= chars.len() { return Err("unexpected end of input".into()); }
        match chars[*pos] {
            '"' => parse_string(chars, pos).map(Value::Str),
            '{' => parse_object(chars, pos),
            '[' => parse_array(chars, pos),
            't' | 'f' => parse_bool(chars, pos),
            'n' => parse_null(chars, pos),
            c if c == '-' || c.is_ascii_digit() => parse_number(chars, pos),
            c => Err(format!("unexpected char '{}' at pos {}", c, pos)),
        }
    }
    fn parse_string(chars: &[char], pos: &mut usize) -> Result<String, String> {
        *pos += 1;
        let mut s = String::new();
        while *pos < chars.len() && chars[*pos] != '"' {
            if chars[*pos] == '\\' { *pos += 1; match chars.get(*pos) { Some('n') => s.push('\n'), Some('t') => s.push('\t'), Some('"') => s.push('"'), Some('\\') => s.push('\\'), Some('/') => s.push('/'), Some('u') => { s.push('?'); *pos += 4; } _ => {} } } else { s.push(chars[*pos]); }
            *pos += 1;
        }
        if *pos < chars.len() { *pos += 1; }
        Ok(s)
    }
    fn parse_number(chars: &[char], pos: &mut usize) -> Result<Value, String> {
        let start = *pos;
        if chars[*pos] == '-' { *pos += 1; }
        while *pos < chars.len() && chars[*pos].is_ascii_digit() { *pos += 1; }
        let mut is_float = false;
        if *pos < chars.len() && chars[*pos] == '.' { is_float = true; *pos += 1; while *pos < chars.len() && chars[*pos].is_ascii_digit() { *pos += 1; } }
        if *pos < chars.len() && (chars[*pos] == 'e' || chars[*pos] == 'E') { is_float = true; *pos += 1; if *pos < chars.len() && (chars[*pos] == '+' || chars[*pos] == '-') { *pos += 1; } while *pos < chars.len() && chars[*pos].is_ascii_digit() { *pos += 1; } }
        let s: String = chars[start..*pos].iter().collect();
        if is_float { s.parse::<f64>().map(Value::Float).map_err(|e| e.to_string()) }
        else { s.parse::<i64>().map(Value::Int).map_err(|e| e.to_string()) }
    }
    fn parse_bool(chars: &[char], pos: &mut usize) -> Result<Value, String> {
        if chars[*pos..].starts_with(&['t','r','u','e']) { *pos += 4; Ok(Value::Bool(true)) }
        else if chars[*pos..].starts_with(&['f','a','l','s','e']) { *pos += 5; Ok(Value::Bool(false)) }
        else { Err("expected bool".into()) }
    }
    fn parse_null(chars: &[char], pos: &mut usize) -> Result<Value, String> {
        if chars[*pos..].starts_with(&['n','u','l','l']) { *pos += 4; Ok(Value::Null) }
        else { Err("expected null".into()) }
    }
    fn parse_array(chars: &[char], pos: &mut usize) -> Result<Value, String> {
        *pos += 1; skip_ws(chars, pos);
        let mut items = Vec::new();
        if *pos < chars.len() && chars[*pos] == ']' { *pos += 1; return Ok(Value::Array(items)); }
        loop {
            items.push(parse_value(chars, pos)?);
            skip_ws(chars, pos);
            if *pos < chars.len() && chars[*pos] == ',' { *pos += 1; skip_ws(chars, pos); } else { break; }
        }
        skip_ws(chars, pos);
        if *pos < chars.len() && chars[*pos] == ']' { *pos += 1; }
        Ok(Value::Array(items))
    }
    fn parse_object(chars: &[char], pos: &mut usize) -> Result<Value, String> {
        *pos += 1; skip_ws(chars, pos);
        let mut pairs = Vec::new();
        if *pos < chars.len() && chars[*pos] == '}' { *pos += 1; return Ok(Value::Object(pairs)); }
        loop {
            skip_ws(chars, pos);
            let key = parse_string(chars, pos)?;
            skip_ws(chars, pos);
            if *pos < chars.len() && chars[*pos] == ':' { *pos += 1; }
            let val = parse_value(chars, pos)?;
            pairs.push((key, val));
            skip_ws(chars, pos);
            if *pos < chars.len() && chars[*pos] == ',' { *pos += 1; } else { break; }
        }
        skip_ws(chars, pos);
        if *pos < chars.len() && chars[*pos] == '}' { *pos += 1; }
        Ok(Value::Object(pairs))
    }
    parse_value(&chars, &mut pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let v = value_object(vec![
            ("name".into(), value_str("Alice".into())),
            ("age".into(), value_int(30)),
        ]);
        let s = json_stringify(v.clone());
        let v2 = json_parse(s).unwrap();
        assert_eq!(v, v2);
    }

    #[test]
    fn test_parse_array() {
        let v = json_parse("[1, 2, 3]".into()).unwrap();
        assert_eq!(v, Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]));
    }

    #[test]
    fn test_field_missing() {
        let v = value_object(vec![("x".into(), value_int(1))]);
        assert!(value_field(v, "y".into()).is_err());
    }
}
