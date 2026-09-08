// decode — SAME-SHAPE reference for decode.almd (#1679, #1673).
//
// Ordinary hand-written Rust against the SAME data model the Almide program
// decodes from: `AlmideValue` as runtime/rs/src/value.rs defines it today
// (interned `Cow<'static, str>` keys, objects as an insertion-ordered
// `Vec<(key, value)>`, owned `String` payloads), copied here so the file
// stands alone under `rustc -O`. The decode is what a competent person writes
// against that shape without touching the data model:
//   - a field read is a BORROWED linear scan over the pairs (`field`), the
//     same two error strings the runtime's `almide_rt_value_field_ref` has;
//   - a `String` field is `to_string()`-ed into the owned record, because the
//     record owns its fields (Almide's `String` is an owned Rust `String`).
// That is the 168 ns row of #1679 — not the 53 ns borrowed-`&str` record
// (`UserRef<'a>`), which is a different record type, and not a slot-indexed
// object, which is a different data model. Anything cheaper here would be
// measuring a program the language cannot express yet.
//
// The document is built once, outside the timed loop, exactly as the Almide
// side parses it once; only the N decodes are the row.

use std::borrow::Cow;

pub type AlmideKey = Cow<'static, str>;

#[derive(Clone, Debug, PartialEq)]
pub enum AlmideValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<AlmideValue>),
    Object(Vec<(AlmideKey, AlmideValue)>),
}

fn kind(v: &AlmideValue) -> &'static str {
    match v {
        AlmideValue::Null => "Null",
        AlmideValue::Bool(_) => "Bool",
        AlmideValue::Int(_) => "Int",
        AlmideValue::Float(_) => "Float",
        AlmideValue::Str(_) => "Str",
        AlmideValue::Array(_) => "Array",
        AlmideValue::Object(_) => "Object",
    }
}

/// Borrowed field lookup: a linear scan over the association list.
fn field<'a>(v: &'a AlmideValue, key: &str) -> Result<&'a AlmideValue, String> {
    if let AlmideValue::Object(pairs) = v {
        for (k, val) in pairs {
            if k.as_ref() == key {
                return Ok(val);
            }
        }
        Err(format!("missing field '{}'", key))
    } else {
        Err(format!("expected Object, received {}", kind(v)))
    }
}

fn as_string(v: &AlmideValue) -> Result<String, String> {
    match v {
        AlmideValue::Str(s) => Ok(s.to_string()),
        _ => Err(format!("expected Str, received {}", kind(v))),
    }
}
fn as_int(v: &AlmideValue) -> Result<i64, String> {
    match v {
        AlmideValue::Int(n) => Ok(*n),
        _ => Err(format!("expected Int, received {}", kind(v))),
    }
}
fn as_float(v: &AlmideValue) -> Result<f64, String> {
    match v {
        AlmideValue::Float(f) => Ok(*f),
        AlmideValue::Int(n) => Ok(*n as f64),
        _ => Err(format!("expected Float, received {}", kind(v))),
    }
}
fn as_bool(v: &AlmideValue) -> Result<bool, String> {
    match v {
        AlmideValue::Bool(b) => Ok(*b),
        _ => Err(format!("expected Bool, received {}", kind(v))),
    }
}
fn as_string_list(v: &AlmideValue) -> Result<Vec<String>, String> {
    match v {
        AlmideValue::Array(items) => items.iter().map(as_string).collect(),
        _ => Err(format!("expected Array, received {}", kind(v))),
    }
}

#[allow(dead_code)]
struct Address {
    city: String,
    zip: String,
    country: String,
}

#[allow(dead_code)]
struct User {
    id: i64,
    name: String,
    email: String,
    age: i64,
    active: bool,
    score: f64,
    tags: Vec<String>,
    address: Address,
}

fn decode_address(v: &AlmideValue) -> Result<Address, String> {
    Ok(Address {
        city: as_string(field(v, "city")?)?,
        zip: as_string(field(v, "zip")?)?,
        country: as_string(field(v, "country")?)?,
    })
}

fn decode_user(v: &AlmideValue) -> Result<User, String> {
    Ok(User {
        id: as_int(field(v, "id")?)?,
        name: as_string(field(v, "name")?)?,
        email: as_string(field(v, "email")?)?,
        age: as_int(field(v, "age")?)?,
        active: as_bool(field(v, "active")?)?,
        score: as_float(field(v, "score")?)?,
        tags: as_string_list(field(v, "tags")?)?,
        address: decode_address(field(v, "address")?)?,
    })
}

fn weight(u: &User) -> i64 {
    u.id
        + u.age
        + u.name.chars().count() as i64
        + u.email.chars().count() as i64
        + u.tags.len() as i64
        + u.address.city.chars().count() as i64
        + if u.active { 1 } else { 0 }
}

fn s(x: &str) -> AlmideValue {
    AlmideValue::Str(x.to_string())
}

fn document() -> AlmideValue {
    AlmideValue::Object(vec![
        (Cow::Borrowed("id"), AlmideValue::Int(42)),
        (Cow::Borrowed("name"), s("Ada Lovelace")),
        (Cow::Borrowed("email"), s("ada@example.com")),
        (Cow::Borrowed("age"), AlmideValue::Int(36)),
        (Cow::Borrowed("active"), AlmideValue::Bool(true)),
        (Cow::Borrowed("score"), AlmideValue::Float(97.5)),
        (Cow::Borrowed("tags"), AlmideValue::Array(vec![s("math"), s("engine"), s("poetry")])),
        (
            Cow::Borrowed("address"),
            AlmideValue::Object(vec![
                (Cow::Borrowed("city"), s("London")),
                (Cow::Borrowed("zip"), s("W1J 9AF")),
                (Cow::Borrowed("country"), s("UK")),
            ]),
        ),
    ])
}

fn main() {
    let n: i64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);
    let v = document();
    let mut acc: i64 = 0;
    for i in 0..n {
        match decode_user(&v) {
            Ok(u) => acc += weight(&u) + (i % 3),
            Err(_) => acc -= 1,
        }
    }
    println!("n: {} acc: {}", n, acc);
}
