// AlmideValue — universal data model for Codec protocol
// All public functions use `almide_rt_` prefix for consistent codegen dispatch.

/// An object key (#1679). The keys of every document a program parses come
/// from a small fixed vocabulary — the field names of its types — so they are
/// interned: the first sighting leaks one `&'static str`, every later one is
/// a pointer copy. `Cow` keeps the fallback honest: past the intern cap, or on
/// a slot collision, a key is an ordinary owned `String`. Clone of an
/// interned key is free, no refcount, and the type stays `Send`.
pub type AlmideKey = std::borrow::Cow<'static, str>;

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

const ALMIDE_KEY_SLOTS: usize = 1024;
const ALMIDE_KEY_LEAK_CAP: usize = 4096;
thread_local! {
    /// Direct-mapped key table: one probe, one byte compare, no hashing
    /// machinery. A collision does not evict — the newcomer stays owned.
    static ALMIDE_KEY_TABLE: std::cell::RefCell<(Vec<Option<&'static str>>, usize)> =
        std::cell::RefCell::new((vec![None; ALMIDE_KEY_SLOTS], 0));
}

#[inline]
fn key_slot(k: &str) -> usize {
    let mut h: u64 = k.len() as u64;
    for &b in k.as_bytes().iter().take(16) { h = (h.rotate_left(5) ^ b as u64).wrapping_mul(0x517c_c1b7_2722_0a95); }
    (h >> 32) as usize % ALMIDE_KEY_SLOTS
}

/// Intern `k` as an object key: a pointer copy when it has been seen, one
/// leaked allocation the first time (bounded by `ALMIDE_KEY_LEAK_CAP`), an owned
/// `String` otherwise.
pub(crate) fn intern_key(k: &str) -> AlmideKey {
    ALMIDE_KEY_TABLE.with(|t| {
        let mut t = t.borrow_mut();
        let slot = key_slot(k);
        match t.0[slot] {
            Some(hit) if hit == k => AlmideKey::Borrowed(hit),
            Some(_) => AlmideKey::Owned(k.to_string()),
            None if t.1 < ALMIDE_KEY_LEAK_CAP => {
                let leaked: &'static str = Box::leak(k.to_string().into_boxed_str());
                t.0[slot] = Some(leaked);
                t.1 += 1;
                AlmideKey::Borrowed(leaked)
            }
            None => AlmideKey::Owned(k.to_string()),
        }
    })
}

// ── Construction ──

pub fn almide_rt_value_str(s: &str) -> AlmideValue { AlmideValue::Str(s.to_string()) }
pub fn almide_rt_value_int(n: i64) -> AlmideValue { AlmideValue::Int(n) }
pub fn almide_rt_value_float(f: f64) -> AlmideValue { AlmideValue::Float(f) }
pub fn almide_rt_value_bool(b: bool) -> AlmideValue { AlmideValue::Bool(b) }
pub fn almide_rt_value_array(items: &Vec<AlmideValue>) -> AlmideValue { AlmideValue::Array(items.clone()) }
pub fn almide_rt_value_object(pairs: &Vec<(String, AlmideValue)>) -> AlmideValue { AlmideValue::Object(pairs.iter().map(|(k, v)| (intern_key(k), v.clone())).collect()) }
pub fn almide_rt_value_null() -> AlmideValue { AlmideValue::Null }
// Structural equality (`value.eq`). The wasm leg had this in its self-host
// registry all along; native only ever reached AlmideValue equality through user
// positions the walker lowers itself, so the first runtime-emitted call site
// (the #1522 record-default decode arm) came up E0425.
pub fn almide_rt_value_eq(a: AlmideValue, b: AlmideValue) -> bool { a == b }

// ── Access ──

/// #1675: the wire-kind name for decode diagnostics — matches the wasm
/// self-host's `__vkind` byte-for-byte (parity is contract surface).
fn value_kind(v: &AlmideValue) -> &'static str {
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

/// #1675: splice a path segment into a decode error — `<leaf>, at <path>`,
/// parent segments prepend (`city` -> `address.city`), index segments join
/// bracket-style (`tags[1]`). The wasm leg's `__err_at` twin composes the
/// same bytes.
pub fn almide_rt___err_at(e: String, seg: String) -> String {
    match e.split_once(", at ") {
        Some((head, path)) if path.starts_with('[') => format!("{head}, at {seg}{path}"),
        Some((head, path)) => format!("{head}, at {seg}.{path}"),
        None => format!("{e}, at {seg}"),
    }
}

/// #1675: the list-element frame — `[i]` joins the path bracket-style.
pub fn almide_rt___err_at_index(e: String, i: i64) -> String {
    almide_rt___err_at(e, format!("[{i}]"))
}

pub fn almide_rt_value_field(v: &AlmideValue, key: &str) -> Result<AlmideValue, String> {
    if let AlmideValue::Object(pairs) = v {
        for (k, val) in pairs {
            if k.as_ref() == key { return Ok(val.clone()); }
        }
        Err(format!("missing field '{}'", key))
    } else {
        Err(format!("expected Object, received {}", value_kind(v)))
    }
}

pub fn almide_rt_value_as_string(v: &AlmideValue) -> Result<String, String> {
    match v { AlmideValue::Str(s) => Ok(s.clone()), _ => Err(format!("expected Str, received {}", value_kind(v))) }
}
pub fn almide_rt_value_as_int(v: &AlmideValue) -> Result<i64, String> {
    match v { AlmideValue::Int(n) => Ok(*n), _ => Err(format!("expected Int, received {}", value_kind(v))) }
}
pub fn almide_rt_value_as_float(v: &AlmideValue) -> Result<f64, String> {
    // A JSON number has no int/float distinction, so an integer literal is a
    // valid Float — widen it (mirrors json.as_float/get_float, value.rs siblings,
    // and serde's f64 deserializer). Keeps Codec roundtrips total for Float
    // fields whose value happens to be integral (#658).
    match v { AlmideValue::Float(f) => Ok(*f), AlmideValue::Int(n) => Ok(*n as f64), _ => Err(format!("expected Float, received {}", value_kind(v))) }
}
pub fn almide_rt_value_as_bool(v: &AlmideValue) -> Result<bool, String> {
    match v { AlmideValue::Bool(b) => Ok(*b), _ => Err(format!("expected Bool, received {}", value_kind(v))) }
}
pub fn almide_rt_value_as_array(v: &AlmideValue) -> Result<Vec<AlmideValue>, String> {
    match v { AlmideValue::Array(a) => Ok(a.clone()), _ => Err(format!("expected Array, received {}", value_kind(v))) }
}

// ── List encode/decode ──

pub fn almide_rt_value_encode_list<T, F: Fn(T) -> AlmideValue>(items: Vec<T>, f: F) -> AlmideValue {
    AlmideValue::Array(items.into_iter().map(f).collect())
}
pub fn almide_rt_value_decode_list<T, F: Fn(AlmideValue) -> Result<T, String>>(v: AlmideValue, f: F) -> Result<Vec<T>, String> {
    match v {
        AlmideValue::Array(items) => items.into_iter().enumerate()
            .map(|(i, e)| f(e).map_err(|er| almide_rt___err_at_index(er, i as i64)))
            .collect(),
        other => Err(format!("expected Array, received {}", value_kind(&other))),
    }
}

/// By-reference twin of `almide_rt_value_decode_list` for an element decoder
/// that BORROWS its input (#1679): every derived `T.decode` takes `&AlmideValue`,
/// so the elements are read in place instead of moved out of a copy.
pub fn almide_rt_value_decode_list_ref<T, F: Fn(&AlmideValue) -> Result<T, String>>(v: &AlmideValue, f: F) -> Result<Vec<T>, String> {
    match v {
        AlmideValue::Array(items) => items.iter().enumerate()
            .map(|(i, e)| f(e).map_err(|er| almide_rt___err_at_index(er, i as i64)))
            .collect(),
        _ => Err(format!("expected Array, received {}", value_kind(v))),
    }
}

// ── Option encode/decode ──

pub fn almide_rt_value_option_encode<T, F: Fn(T) -> AlmideValue>(opt: Option<T>, f: F) -> AlmideValue {
    match opt { Some(v) => f(v), None => AlmideValue::Null }
}
pub fn almide_rt_value_decode_option<T, F: Fn(AlmideValue) -> Result<T, String>>(v: &AlmideValue, key: &str, f: F) -> Result<Option<T>, String> {
    match almide_rt_value_field(v, key) {
        Ok(AlmideValue::Null) => Ok(None),
        Ok(val) => f(val).map(Some),
        Err(_) => Ok(None),
    }
}
/// Owned-argument variant for derived `Option[CustomType]` decode. The codegen
/// passes the object and key by value (like `almide_rt_value_decode_list`), so this
/// wrapper borrows for the by-ref generic above (新②).
pub fn almide_rt_value_decode_option_custom<T, F: Fn(AlmideValue) -> Result<T, String>>(v: AlmideValue, key: String, f: F) -> Result<Option<T>, String> {
    almide_rt_value_decode_option(&v, &key, f)
}
/// By-reference twin of `almide_rt_value_decode_option_custom` (#1679): the
/// field is looked up without a copy and handed to a `&AlmideValue` decoder.
pub fn almide_rt_value_decode_option_custom_ref<T, F: Fn(&AlmideValue) -> Result<T, String>>(v: &AlmideValue, key: String, f: F) -> Result<Option<T>, String> {
    match almide_rt_value_field_ref(v, &key) {
        Ok(AlmideValue::Null) => Ok(None),
        Ok(val) => f(val).map(Some),
        Err(_) => Ok(None),
    }
}
/// `almide_rt_value_field` without the copy: a borrow into the object. The
/// native walker folds `&(almide_rt_value_field(v, k))?` — the shape every
/// derived decode reads a field through — into `almide_rt_value_field_ref(v, k)?`
/// (#1679): same lookup, same two error strings, no clone of the field.
pub fn almide_rt_value_field_ref<'a>(v: &'a AlmideValue, key: &str) -> Result<&'a AlmideValue, String> {
    if let AlmideValue::Object(pairs) = v {
        for (k, val) in pairs {
            if k.as_ref() == key { return Ok(val); }
        }
        Err(format!("missing field '{}'", key))
    } else {
        Err(format!("expected Object, received {}", value_kind(v)))
    }
}
pub fn almide_rt_value_decode_with_default<T: Clone, F: Fn(AlmideValue) -> Result<T, String>>(v: &AlmideValue, key: &str, default: T, f: F) -> Result<T, String> {
    match almide_rt_value_field(v, key) {
        Ok(AlmideValue::Null) => Ok(default),
        Ok(val) => f(val),
        Err(_) => Ok(default),
    }
}

// ── Concrete list helpers ──

pub fn almide_rt___encode_list_string(items: Vec<String>) -> AlmideValue { almide_rt_value_encode_list(items, |s| almide_rt_value_str(&s)) }
pub fn almide_rt___encode_list_int(items: Vec<i64>) -> AlmideValue { almide_rt_value_encode_list(items, almide_rt_value_int) }
pub fn almide_rt___encode_list_float(items: Vec<f64>) -> AlmideValue { almide_rt_value_encode_list(items, almide_rt_value_float) }
pub fn almide_rt___encode_list_bool(items: Vec<bool>) -> AlmideValue { almide_rt_value_encode_list(items, almide_rt_value_bool) }
// The primitive list decoders borrow their input (#1679): the derive's
// `__decode_list_<prim>(field)` call is borrow-wrapped by BorrowInsertion from
// this signature, and the walker folds that into a `value_field_ref` lookup.
pub fn almide_rt___decode_list_string(v: &AlmideValue) -> Result<Vec<String>, String> { almide_rt_value_decode_list_ref(v, almide_rt_value_as_string) }
pub fn almide_rt___decode_list_int(v: &AlmideValue) -> Result<Vec<i64>, String> { almide_rt_value_decode_list_ref(v, almide_rt_value_as_int) }
pub fn almide_rt___decode_list_float(v: &AlmideValue) -> Result<Vec<f64>, String> { almide_rt_value_decode_list_ref(v, almide_rt_value_as_float) }
pub fn almide_rt___decode_list_bool(v: &AlmideValue) -> Result<Vec<bool>, String> { almide_rt_value_decode_list_ref(v, almide_rt_value_as_bool) }

// ── Concrete option helpers ──

pub fn almide_rt___encode_option_string(v: Option<String>) -> AlmideValue { almide_rt_value_option_encode(v, |s| almide_rt_value_str(&s)) }
pub fn almide_rt___encode_option_int(v: Option<i64>) -> AlmideValue { almide_rt_value_option_encode(v, almide_rt_value_int) }
pub fn almide_rt___encode_option_float(v: Option<f64>) -> AlmideValue { almide_rt_value_option_encode(v, almide_rt_value_float) }
pub fn almide_rt___encode_option_bool(v: Option<bool>) -> AlmideValue { almide_rt_value_option_encode(v, almide_rt_value_bool) }
pub fn almide_rt___decode_option_string(v: AlmideValue, key: String) -> Result<Option<String>, String> { almide_rt_value_decode_option(&v, &key, |x| almide_rt_value_as_string(&x)) }
pub fn almide_rt___decode_option_int(v: AlmideValue, key: String) -> Result<Option<i64>, String> { almide_rt_value_decode_option(&v, &key, |x| almide_rt_value_as_int(&x)) }
pub fn almide_rt___decode_option_float(v: AlmideValue, key: String) -> Result<Option<f64>, String> { almide_rt_value_decode_option(&v, &key, |x| almide_rt_value_as_float(&x)) }
pub fn almide_rt___decode_option_bool(v: AlmideValue, key: String) -> Result<Option<bool>, String> { almide_rt_value_decode_option(&v, &key, |x| almide_rt_value_as_bool(&x)) }
pub fn almide_rt___decode_default_string(v: AlmideValue, key: String, default: String) -> Result<String, String> { almide_rt_value_decode_with_default(&v, &key, default, |x| almide_rt_value_as_string(&x)) }
pub fn almide_rt___decode_default_int(v: AlmideValue, key: String, default: i64) -> Result<i64, String> { almide_rt_value_decode_with_default(&v, &key, default, |x| almide_rt_value_as_int(&x)) }
pub fn almide_rt___decode_default_float(v: AlmideValue, key: String, default: f64) -> Result<f64, String> { almide_rt_value_decode_with_default(&v, &key, default, |x| almide_rt_value_as_float(&x)) }
pub fn almide_rt___decode_default_bool(v: AlmideValue, key: String, default: bool) -> Result<bool, String> { almide_rt_value_decode_with_default(&v, &key, default, |x| almide_rt_value_as_bool(&x)) }
// List[scalar] defaults (#1520): a missing/null key yields the default list;
// a present key decodes through the same per-element path the required-field
// form uses. Without these the derive emitted `__decode_default_value`, a
// name no runtime provides — check green, rustc E0425.
pub fn almide_rt___decode_default_list_string(v: AlmideValue, key: String, default: Vec<String>) -> Result<Vec<String>, String> { almide_rt_value_decode_with_default(&v, &key, default, |x| almide_rt_value_decode_list(x, |e| almide_rt_value_as_string(&e))) }
pub fn almide_rt___decode_default_list_int(v: AlmideValue, key: String, default: Vec<i64>) -> Result<Vec<i64>, String> { almide_rt_value_decode_with_default(&v, &key, default, |x| almide_rt_value_decode_list(x, |e| almide_rt_value_as_int(&e))) }
pub fn almide_rt___decode_default_list_float(v: AlmideValue, key: String, default: Vec<f64>) -> Result<Vec<f64>, String> { almide_rt_value_decode_with_default(&v, &key, default, |x| almide_rt_value_decode_list(x, |e| almide_rt_value_as_float(&e))) }
pub fn almide_rt___decode_default_list_bool(v: AlmideValue, key: String, default: Vec<bool>) -> Result<Vec<bool>, String> { almide_rt_value_decode_with_default(&v, &key, default, |x| almide_rt_value_decode_list(x, |e| almide_rt_value_as_bool(&e))) }

// ── AlmideValue utilities ──

/// Object keys in insertion order. Lives in the `value` runtime (not reusing
/// `almide_json_keys`) so a program that uses `value.keys` but never `json` still
/// links — the native runtime is included per-module, so a cross-module reference
/// would be undefined on a value-only program (#416 native-link fix).
pub fn almide_rt_value_keys(v: &AlmideValue) -> Vec<String> {
    match v {
        AlmideValue::Object(entries) => entries.iter().map(|(k, _)| k.to_string()).collect(),
        _ => vec![],
    }
}

/// Pick specific keys from an Object, discarding the rest.
pub fn almide_rt_value_pick(v: &AlmideValue, keys: &[String]) -> AlmideValue {
    match v {
        AlmideValue::Object(pairs) => {
            AlmideValue::Object(pairs.iter().filter(|(k, _)| keys.iter().any(|x| x.as_str() == k.as_ref())).cloned().collect())
        }
        other => other.clone(),
    }
}

/// Rename keys in an Object using a transform function.
pub fn almide_rt_value_rename_keys(v: &AlmideValue, f: impl Fn(String) -> String) -> AlmideValue {
    match v {
        AlmideValue::Object(pairs) => {
            AlmideValue::Object(pairs.iter().map(|(k, v)| (intern_key(&f(k.to_string())), v.clone())).collect())
        }
        other => other.clone(),
    }
}

/// Merge two Objects. Keys from `b` override keys from `a`.
pub fn almide_rt_value_merge(a: &AlmideValue, b: &AlmideValue) -> AlmideValue {
    match (a, b) {
        (AlmideValue::Object(pa), AlmideValue::Object(pb)) => {
            let mut pa = pa.clone();
            for (k, v) in pb {
                if let Some(pos) = pa.iter().position(|(ek, _)| ek == k) {
                    pa[pos] = (k.clone(), v.clone());
                } else {
                    pa.push((k.clone(), v.clone()));
                }
            }
            AlmideValue::Object(pa)
        }
        (_, b) => b.clone(),
    }
}

/// Remove specific keys from an Object.
pub fn almide_rt_value_omit(v: &AlmideValue, keys: &[String]) -> AlmideValue {
    match v {
        AlmideValue::Object(pairs) => {
            AlmideValue::Object(pairs.iter().filter(|(k, _)| !keys.iter().any(|x| x.as_str() == k.as_ref())).cloned().collect())
        }
        other => other.clone(),
    }
}

/// Convert snake_case key to camelCase.
pub fn almide_rt_value_to_camel_case(v: &AlmideValue) -> AlmideValue {
    almide_rt_value_rename_keys(v, |k| {
        let mut result = String::new();
        let mut capitalize_next = false;
        for c in k.chars() {
            if c == '_' { capitalize_next = true; }
            else if capitalize_next { result.push(c.to_ascii_uppercase()); capitalize_next = false; }
            else { result.push(c); }
        }
        result
    })
}

/// Convert camelCase key to snake_case.
pub fn almide_rt_value_to_snake_case(v: &AlmideValue) -> AlmideValue {
    almide_rt_value_rename_keys(v, |k| {
        let mut result = String::new();
        for (i, c) in k.chars().enumerate() {
            if c.is_ascii_uppercase() && i > 0 { result.push('_'); }
            result.push(c.to_ascii_lowercase());
        }
        result
    })
}

// ── Variant decode helper ──

/// Extract the tag and payload from a tagged variant object {"Tag": payload}
pub fn almide_rt_value_tagged_variant(v: AlmideValue) -> Result<(String, AlmideValue), String> {
    match v {
        AlmideValue::Object(pairs) => {
            if pairs.len() == 1 {
                let (tag, payload) = pairs.into_iter().next().unwrap();
                Ok((tag.into_owned(), payload))
            } else {
                Err(format!("expected object with exactly 1 key for variant, got {} keys", pairs.len()))
            }
        }
        _ => Err("expected Object for variant decode".to_string()),
    }
}

// ── Stringify ──

pub fn almide_rt_value_stringify(v: &AlmideValue) -> String {
    match v {
        AlmideValue::Null => "null".to_string(),
        AlmideValue::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        AlmideValue::Int(n) => n.to_string(),
        AlmideValue::Float(f) => format!("{}", f),
        AlmideValue::Str(s) => format!("\"{}\"", s
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")),
        AlmideValue::Array(items) => {
            let inner: Vec<String> = items.iter().map(almide_rt_value_stringify).collect();
            format!("[{}]", inner.join(","))
        }
        AlmideValue::Object(pairs) => {
            let inner: Vec<String> = pairs.iter().map(|(k, v)| {
                let ek = k.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t");
                format!("\"{}\":{}", ek, almide_rt_value_stringify(v))
            }).collect();
            format!("{{{}}}", inner.join(","))
        }
    }
}

// `AlmideValue` participates in derived `Repr` and string interpolation like any
// other type. A record/variant with a `AlmideValue` field generates both
// `self.<field>.almide_repr()` (the `AlmideRepr` path) and `{}` (the `Display`
// path used by the `<Type>_repr` free fn and `"${t}"` interpolation), so `AlmideValue`
// must impl both — without either, such a type fails to compile (E0599 / E0277).
// Both render the AlmideValue as its JSON text, identically, so the field reprs
// consistently across every path — and byte-identically to wasm, which reprs a
// `AlmideValue` through the same JSON serializer (see `emit_repr_value`).
impl AlmideRepr for AlmideValue {
    fn almide_repr(&self) -> String { almide_rt_value_stringify(self) }
}

impl std::fmt::Display for AlmideValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&almide_rt_value_stringify(self))
    }
}

// json_parse and json_stringify moved to json.rs
