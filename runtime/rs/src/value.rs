// AlmideValue — universal data model for Codec protocol
// All public functions use `almide_rt_` prefix for consistent codegen dispatch.

/// An object key (#1679). The keys of every document a program parses come
/// from a small fixed vocabulary — the field names of its types — so they are
/// interned: the first sighting fills one slot of a fixed table, every later
/// one is a pointer copy. `Cow` keeps the fallback honest: a key too long to
/// intern, or one whose slot holds another key, is an ordinary owned
/// `String`. Clone of an interned key is free, no refcount, and the type
/// stays `Send`.
pub type AlmideKey = std::borrow::Cow<'static, str>;

#[derive(Clone, Debug, PartialEq)]
pub enum AlmideValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(AlmideString),
    Array(Vec<AlmideValue>),
    Object(Vec<(AlmideKey, AlmideValue)>),
}

const ALMIDE_KEY_SLOTS: usize = 1024;
/// A key longer than this is never interned: field names are short, and the
/// bound keeps the table's whole footprint at most 1024 × 128 bytes.
const ALMIDE_KEY_INTERN_MAX_LEN: usize = 128;
/// Direct-mapped key table, ONE for the process (#4202): one probe, one byte
/// compare, no hashing machinery. A slot is filled once and never evicted —
/// a collision leaves the newcomer owned. It was per thread, with a leaked
/// allocation per new key, so every `fan` arm and `fs.fold_lines_chunked`
/// worker (a fresh thread each call) leaked up to its own table's worth of
/// keys of any length: RSS grew with every call. Now the whole process holds
/// at most `ALMIDE_KEY_SLOTS` keys of at most `ALMIDE_KEY_INTERN_MAX_LEN`
/// bytes, and nothing is leaked.
static ALMIDE_KEY_TABLE: [std::sync::OnceLock<Box<str>>; ALMIDE_KEY_SLOTS] =
    [const { std::sync::OnceLock::new() }; ALMIDE_KEY_SLOTS];

#[inline]
fn almide_rt_key_slot(k: &str) -> usize {
    let mut h: u64 = k.len() as u64;
    for &b in k.as_bytes().iter().take(16) { h = (h.rotate_left(5) ^ b as u64).wrapping_mul(0x517c_c1b7_2722_0a95); }
    (h >> 32) as usize % ALMIDE_KEY_SLOTS
}

/// Intern `k` as an object key: a pointer copy when its slot holds it (the
/// first key to reach an empty slot fills it), an owned `String` otherwise.
pub(crate) fn almide_rt_intern_key(k: &str) -> AlmideKey {
    if k.len() > ALMIDE_KEY_INTERN_MAX_LEN {
        return AlmideKey::Owned(k.to_string());
    }
    let held: &'static str = ALMIDE_KEY_TABLE[almide_rt_key_slot(k)].get_or_init(|| k.into());
    if held == k { AlmideKey::Borrowed(held) } else { AlmideKey::Owned(k.to_string()) }
}

// ── Construction ──

pub fn almide_rt_value_str(s: &str) -> AlmideValue { AlmideValue::Str(AlmideString::from(s)) }
pub fn almide_rt_value_int(n: i64) -> AlmideValue { AlmideValue::Int(n) }
pub fn almide_rt_value_float(f: f64) -> AlmideValue { AlmideValue::Float(f) }
pub fn almide_rt_value_bool(b: bool) -> AlmideValue { AlmideValue::Bool(b) }
pub fn almide_rt_value_array(items: &[AlmideValue]) -> AlmideValue { AlmideValue::Array(items.to_vec()) }
pub fn almide_rt_value_object(pairs: &[(AlmideString, AlmideValue)]) -> AlmideValue { AlmideValue::Object(almide_rt_object_upsert(pairs.iter().map(|(k, v)| (almide_rt_intern_key(k), v.clone())).collect())) }
/// No Object holds a key twice (C-401, #4112): a pair list that repeats a key
/// is folded as one upsert per pair — the key keeps the POSITION of its first
/// occurrence and takes the VALUE of its last, the C-380 rule
/// (ALS-C1). So every reader of the Object (`field`, `keys`, `stringify`,
/// `to_map`) sees one entry, and `json.stringify` never writes a key twice.
/// Small lists compare pairwise; from 16 pairs an index answers.
pub(crate) fn almide_rt_object_upsert(pairs: Vec<(AlmideKey, AlmideValue)>) -> Vec<(AlmideKey, AlmideValue)> {
    let dup = if pairs.len() < 16 {
        (1..pairs.len()).any(|i| pairs[..i].iter().any(|(k, _)| *k == pairs[i].0))
    } else {
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::with_capacity(pairs.len());
        !pairs.iter().all(|(k, _)| seen.insert(k.as_ref()))
    };
    if !dup {
        return pairs;
    }
    let mut out: Vec<(AlmideKey, AlmideValue)> = Vec::with_capacity(pairs.len());
    let mut at: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (k, v) in pairs {
        match at.get(k.as_ref()) {
            Some(&i) => out[i].1 = v,
            None => {
                at.insert(k.to_string(), out.len());
                out.push((k, v));
            }
        }
    }
    out
}
// The `@reuse` twins (#2157): a source at its last use — the list literal or
// `list.map` result a document is built from, almost always — is taken by
// value and its elements MOVED into the Value. The borrowing forms above
// cloned every element, so building a document copied each subtree once per
// nesting level, and freed the copies' originals right after.
pub fn almide_rt_value_array_owned(items: Vec<AlmideValue>) -> AlmideValue { AlmideValue::Array(items) }
pub fn almide_rt_value_object_owned(pairs: Vec<(AlmideString, AlmideValue)>) -> AlmideValue { AlmideValue::Object(almide_rt_object_upsert(pairs.into_iter().map(|(k, v)| (almide_rt_intern_key(&k), v)).collect())) }
pub fn almide_rt_value_null() -> AlmideValue { AlmideValue::Null }
// Structural equality (`value.eq`). The wasm leg had this in its self-host
// registry all along; native only ever reached AlmideValue equality through user
// positions the walker lowers itself, so the first runtime-emitted call site
// (the #1522 record-default decode arm) came up E0425.
pub fn almide_rt_value_eq(a: AlmideValue, b: AlmideValue) -> bool { a == b }

// ── Access ──

/// #1675: the wire-kind name for decode diagnostics — matches the wasm
/// self-host's `__vkind` byte-for-byte (parity is contract surface).
fn almide_rt_value_kind(v: &AlmideValue) -> &'static str {
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
pub fn almide_rt___err_at(e: AlmideString, seg: AlmideString) -> AlmideString {
    let __almide_out: String = {
        match e.split_once(", at ") {
            Some((head, path)) if path.starts_with('[') => format!("{head}, at {seg}{path}"),
            Some((head, path)) => format!("{head}, at {seg}.{path}"),
            None => format!("{e}, at {seg}"),
        }
    };
    __almide_out.into()
}

/// #1675: the list-element frame — `[i]` joins the path bracket-style.
pub fn almide_rt___err_at_index(e: AlmideString, i: i64) -> AlmideString { almide_rt___err_at(e, AlmideString::from(format!("[{i}]"))) }

pub fn almide_rt_value_field(v: &AlmideValue, key: &str) -> Result<AlmideValue, AlmideString> {
    let __almide_r: Result<AlmideValue, String> = (|| {
        if let AlmideValue::Object(pairs) = v {
            for (k, val) in pairs {
                if k.as_ref() == key { return Ok(val.clone()); }
            }
            Err(format!("missing field '{}'", almide_rt_errtext_escape(key)))
        } else {
            Err(format!("expected Object, received {}", almide_rt_value_kind(v)))
        }
    })();
    __almide_r.map_err(Into::into)
}

pub fn almide_rt_value_as_string(v: &AlmideValue) -> Result<AlmideString, AlmideString> { let __almide_v: Result<AlmideString, AlmideString> = { let __almide_r: Result<AlmideString, String> = (|| { match v { AlmideValue::Str(s) => Ok(s.clone()), _ => Err(format!("expected Str, received {}", almide_rt_value_kind(v))) } })(); __almide_r.map_err(Into::into) }; __almide_v.into_almide() }
pub fn almide_rt_value_as_int(v: &AlmideValue) -> Result<i64, AlmideString> { let __almide_r: Result<i64, String> = (|| { match v { AlmideValue::Int(n) => Ok(*n), _ => Err(format!("expected Int, received {}", almide_rt_value_kind(v))) } })(); __almide_r.map_err(Into::into) }
pub fn almide_rt_value_as_float(v: &AlmideValue) -> Result<f64, AlmideString> {
    let __almide_r: Result<f64, String> = (|| {
        // A JSON number has no int/float distinction, so an integer literal is a
        // valid Float — widen it (mirrors json.as_float/get_float, value.rs siblings,
        // and serde's f64 deserializer). Keeps Codec roundtrips total for Float
        // fields whose value happens to be integral (#658).
        match v { AlmideValue::Float(f) => Ok(*f), AlmideValue::Int(n) => Ok(*n as f64), _ => Err(format!("expected Float, received {}", almide_rt_value_kind(v))) }
    })();
    __almide_r.map_err(Into::into)
}
pub fn almide_rt_value_as_bool(v: &AlmideValue) -> Result<bool, AlmideString> { let __almide_r: Result<bool, String> = (|| { match v { AlmideValue::Bool(b) => Ok(*b), _ => Err(format!("expected Bool, received {}", almide_rt_value_kind(v))) } })(); __almide_r.map_err(Into::into) }
pub fn almide_rt_value_as_array(v: &AlmideValue) -> Result<Vec<AlmideValue>, AlmideString> { let __almide_r: Result<Vec<AlmideValue>, String> = (|| { match v { AlmideValue::Array(a) => Ok(a.clone()), _ => Err(format!("expected Array, received {}", almide_rt_value_kind(v))) } })(); __almide_r.map_err(Into::into) }
/// `value.as_object` (R-value-02): the Object's pairs in order, the `as_array`
/// family's Object cell; any other kind is the same `expected Object` err
/// `value.__field` gives.
pub fn almide_rt_value_as_object(v: &AlmideValue) -> Result<Vec<(AlmideString, AlmideValue)>, AlmideString> {
    match v {
        AlmideValue::Object(pairs) => Ok(pairs.iter().map(|(k, x)| (AlmideString::from(k.as_ref()), x.clone())).collect()),
        _ => Err(AlmideString::from(format!("expected Object, received {}", almide_rt_value_kind(v)))),
    }
}
/// The `@reuse` twin (#2157): a Value at its last use hands its items over
/// instead of having every element (each a whole subtree) cloned out of it.
pub fn almide_rt_value_as_array_owned(v: AlmideValue) -> Result<Vec<AlmideValue>, AlmideString> {
    match v {
        AlmideValue::Array(a) => Ok(a),
        other => Err(AlmideString::from(format!("expected Array, received {}", almide_rt_value_kind(&other)))),
    }
}

// ── List encode/decode ──

pub fn almide_rt_value_encode_list<T, F: Fn(T) -> AlmideValue>(items: Vec<T>, f: F) -> AlmideValue {
    AlmideValue::Array(items.into_iter().map(f).collect())
}
pub fn almide_rt_value_decode_list<T, F: Fn(AlmideValue) -> Result<T, AlmideString>>(v: AlmideValue, f: F) -> Result<Vec<T>, AlmideString> {
    let __almide_r: Result<Vec<T>, AlmideString> = (|| {
        match v {
            AlmideValue::Array(items) => items.into_iter().enumerate()
                .map(|(i, e)| f(e).map_err(|er| almide_rt___err_at_index(er, i as i64)))
                .collect(),
            other => Err(AlmideString::from(format!("expected Array, received {}", almide_rt_value_kind(&other)))),
        }
    })();
    __almide_r.map_err(Into::into)
}

/// By-reference twin of `almide_rt_value_decode_list` for an element decoder
/// that BORROWS its input (#1679): every derived `T.decode` takes `&AlmideValue`,
/// so the elements are read in place instead of moved out of a copy.
pub fn almide_rt_value_decode_list_ref<T, F: Fn(&AlmideValue) -> Result<T, AlmideString>>(v: &AlmideValue, f: F) -> Result<Vec<T>, AlmideString> {
    let __almide_r: Result<Vec<T>, AlmideString> = (|| {
        match v {
            AlmideValue::Array(items) => items.iter().enumerate()
                .map(|(i, e)| f(e).map_err(|er| almide_rt___err_at_index(er, i as i64)))
                .collect(),
            _ => Err(AlmideString::from(format!("expected Array, received {}", almide_rt_value_kind(v)))),
        }
    })();
    __almide_r.map_err(Into::into)
}

// ── Option encode/decode ──

pub fn almide_rt_value_option_encode<T, F: Fn(T) -> AlmideValue>(opt: Option<T>, f: F) -> AlmideValue {
    match opt { Some(v) => f(v), None => AlmideValue::Null }
}
pub fn almide_rt_value_decode_option<T, F: Fn(AlmideValue) -> Result<T, AlmideString>>(v: &AlmideValue, key: &str, f: F) -> Result<Option<T>, AlmideString> {
    let __almide_r: Result<Option<T>, String> = (|| {
        match almide_rt_value_field(v, key) {
            Ok(AlmideValue::Null) => Ok(None),
            Ok(val) => f(val).map(Some).map_err(String::from),
            Err(_) => Ok(None),
        }
    })();
    __almide_r.map_err(Into::into)
}
/// Owned-argument variant for derived `Option[CustomType]` decode. The codegen
/// passes the object and key by value (like `almide_rt_value_decode_list`), so this
/// wrapper borrows for the by-ref generic above (新②).
pub fn almide_rt_value_decode_option_custom<T, F: Fn(AlmideValue) -> Result<T, AlmideString>>(v: AlmideValue, key: AlmideString, f: F) -> Result<Option<T>, AlmideString> {
    almide_rt_value_decode_option(&v, &key, f)
}
/// By-reference twin of `almide_rt_value_decode_option_custom` (#1679): the
/// field is looked up without a copy and handed to a `&AlmideValue` decoder.
pub fn almide_rt_value_decode_option_custom_ref<T, F: Fn(&AlmideValue) -> Result<T, AlmideString>>(v: &AlmideValue, key: AlmideString, f: F) -> Result<Option<T>, AlmideString> {
    let __almide_r: Result<Option<T>, String> = (|| {
        match almide_rt_value_field_ref(v, &key) {
            Ok(AlmideValue::Null) => Ok(None),
            Ok(val) => f(val).map(Some).map_err(String::from),
            Err(_) => Ok(None),
        }
    })();
    __almide_r.map_err(Into::into)
}
/// `almide_rt_value_field` without the copy: a borrow into the object. The
/// native walker folds `&(almide_rt_value_field(v, k))?` — the shape every
/// derived decode reads a field through — into `almide_rt_value_field_ref(v, k)?`
/// (#1679): same lookup, same two error strings, no clone of the field.
pub fn almide_rt_value_field_ref<'a>(v: &'a AlmideValue, key: &str) -> Result<&'a AlmideValue, AlmideString> {
    let __almide_r: Result<&'a AlmideValue, String> = (|| {
        if let AlmideValue::Object(pairs) = v {
            for (k, val) in pairs {
                if k.as_ref() == key { return Ok(val); }
            }
            Err(format!("missing field '{}'", almide_rt_errtext_escape(key)))
        } else {
            Err(format!("expected Object, received {}", almide_rt_value_kind(v)))
        }
    })();
    __almide_r.map_err(Into::into)
}
/// `almide_rt_value_field_ref` with a slot hint (#1679, slot-indexed lookup). A
/// derived `T.decode` knows the declaration index of every field, and a
/// document written by `T.encode` (or by hand in declaration order — the
/// common case) carries its keys in that order, so the field is usually
/// sitting at `pairs[hint]`: one bounds check and one key compare, no scan.
/// The compare is length-then-pointer-then-bytes: the key literal the derive
/// passes and the interned `Cow::Borrowed` key are distinct allocations, so
/// the pointer test is only the cheap identity short-cut (a key the program
/// itself interned), and a same-length key falls through to one short byte
/// compare. A miss at the
/// hint (a reordered, sparse, or hand-built document) falls back to the plain
/// scan, so the result and the two error strings (C-084) are exactly
/// `almide_rt_value_field_ref`'s for every document.
#[inline]
pub fn almide_rt_value_field_ref_at<'a>(v: &'a AlmideValue, key: &str, hint: usize) -> Result<&'a AlmideValue, AlmideString> {
    let __almide_r: Result<&'a AlmideValue, String> = (|| {
        if let AlmideValue::Object(pairs) = v {
            if let Some((k, val)) = pairs.get(hint) {
                let k: &str = k.as_ref();
                if k.len() == key.len() && (std::ptr::eq(k.as_ptr(), key.as_ptr()) || k == key) {
                    return Ok(val);
                }
            }
        }
        almide_rt_value_field_ref(v, key).map_err(String::from)
    })();
    __almide_r.map_err(Into::into)
}
pub fn almide_rt_value_decode_with_default<T: Clone, F: Fn(AlmideValue) -> Result<T, AlmideString>>(v: &AlmideValue, key: &str, default: T, f: F) -> Result<T, AlmideString> {
    let __almide_r: Result<T, String> = (|| {
        match almide_rt_value_field(v, key) {
            Ok(AlmideValue::Null) => Ok(default),
            Ok(val) => f(val).map_err(String::from),
            Err(_) => Ok(default),
        }
    })();
    __almide_r.map_err(Into::into)
}

// ── Concrete list helpers ──

pub fn almide_rt___encode_list_string(items: Vec<AlmideString>) -> AlmideValue { almide_rt_value_encode_list(items, |s| almide_rt_value_str(&s)) }
pub fn almide_rt___encode_list_int(items: Vec<i64>) -> AlmideValue { almide_rt_value_encode_list(items, almide_rt_value_int) }
pub fn almide_rt___encode_list_float(items: Vec<f64>) -> AlmideValue { almide_rt_value_encode_list(items, almide_rt_value_float) }
pub fn almide_rt___encode_list_bool(items: Vec<bool>) -> AlmideValue { almide_rt_value_encode_list(items, almide_rt_value_bool) }
// The primitive list decoders borrow their input (#1679): the derive's
// `__decode_list_<prim>(field)` call is borrow-wrapped by BorrowInsertion from
// this signature, and the walker folds that into a `value_field_ref` lookup.
pub fn almide_rt___decode_list_string(v: &AlmideValue) -> Result<Vec<AlmideString>, AlmideString> { let __almide_v: Result<Vec<AlmideString>, AlmideString> = { almide_rt_value_decode_list_ref(v, almide_rt_value_as_string) }; __almide_v.into_almide() }
pub fn almide_rt___decode_list_int(v: &AlmideValue) -> Result<Vec<i64>, AlmideString> { almide_rt_value_decode_list_ref(v, almide_rt_value_as_int) }
pub fn almide_rt___decode_list_float(v: &AlmideValue) -> Result<Vec<f64>, AlmideString> { almide_rt_value_decode_list_ref(v, almide_rt_value_as_float) }
pub fn almide_rt___decode_list_bool(v: &AlmideValue) -> Result<Vec<bool>, AlmideString> { almide_rt_value_decode_list_ref(v, almide_rt_value_as_bool) }

// ── Concrete option helpers ──

pub fn almide_rt___encode_option_string(v: Option<AlmideString>) -> AlmideValue { almide_rt_value_option_encode(v, |s| almide_rt_value_str(&s)) }
pub fn almide_rt___encode_option_int(v: Option<i64>) -> AlmideValue { almide_rt_value_option_encode(v, almide_rt_value_int) }
pub fn almide_rt___encode_option_float(v: Option<f64>) -> AlmideValue { almide_rt_value_option_encode(v, almide_rt_value_float) }
pub fn almide_rt___encode_option_bool(v: Option<bool>) -> AlmideValue { almide_rt_value_option_encode(v, almide_rt_value_bool) }
// The primitive option and default decoders borrow the document too (#2052):
// they were the last decode helpers taking it BY VALUE, so a derived decode
// that used one had to own its input — moving or cloning the whole document
// per field — and an outer decode that borrowed (`alt: Addr?`) handed a
// `&Value` to the by-value option driver (rustc E0308). BorrowInsertion seeds
// these signatures (`seed_codec_helper_sigs`) so every derived decode borrows.
pub fn almide_rt___decode_option_string(v: &AlmideValue, key: AlmideString) -> Result<Option<AlmideString>, AlmideString> { let __almide_v: Result<Option<AlmideString>, AlmideString> = { almide_rt_value_decode_option(v, &key, |x| almide_rt_value_as_string(&x)) }; __almide_v.into_almide() }
pub fn almide_rt___decode_option_int(v: &AlmideValue, key: AlmideString) -> Result<Option<i64>, AlmideString> { almide_rt_value_decode_option(v, &key, |x| almide_rt_value_as_int(&x)) }
pub fn almide_rt___decode_option_float(v: &AlmideValue, key: AlmideString) -> Result<Option<f64>, AlmideString> { almide_rt_value_decode_option(v, &key, |x| almide_rt_value_as_float(&x)) }
pub fn almide_rt___decode_option_bool(v: &AlmideValue, key: AlmideString) -> Result<Option<bool>, AlmideString> { almide_rt_value_decode_option(v, &key, |x| almide_rt_value_as_bool(&x)) }
pub fn almide_rt___decode_default_string(v: &AlmideValue, key: AlmideString, default: AlmideString) -> Result<AlmideString, AlmideString> { let __almide_v: Result<AlmideString, AlmideString> = { almide_rt_value_decode_with_default(v, &key, default, |x| almide_rt_value_as_string(&x)) }; __almide_v.into_almide() }
pub fn almide_rt___decode_default_int(v: &AlmideValue, key: AlmideString, default: i64) -> Result<i64, AlmideString> { almide_rt_value_decode_with_default(v, &key, default, |x| almide_rt_value_as_int(&x)) }
pub fn almide_rt___decode_default_float(v: &AlmideValue, key: AlmideString, default: f64) -> Result<f64, AlmideString> { almide_rt_value_decode_with_default(v, &key, default, |x| almide_rt_value_as_float(&x)) }
pub fn almide_rt___decode_default_bool(v: &AlmideValue, key: AlmideString, default: bool) -> Result<bool, AlmideString> { almide_rt_value_decode_with_default(v, &key, default, |x| almide_rt_value_as_bool(&x)) }
// List[scalar] defaults (#1520): a missing/null key yields the default list;
// a present key decodes through the same per-element path the required-field
// form uses. Without these the derive emitted `__decode_default_value`, a
// name no runtime provides — check green, rustc E0425.
pub fn almide_rt___decode_default_list_string(v: &AlmideValue, key: AlmideString, default: Vec<AlmideString>) -> Result<Vec<AlmideString>, AlmideString> { let __almide_v: Result<Vec<AlmideString>, AlmideString> = { almide_rt_value_decode_with_default(v, &key, default, |x| almide_rt_value_decode_list(x, |e| almide_rt_value_as_string(&e))) }; __almide_v.into_almide() }
pub fn almide_rt___decode_default_list_int(v: &AlmideValue, key: AlmideString, default: Vec<i64>) -> Result<Vec<i64>, AlmideString> { almide_rt_value_decode_with_default(v, &key, default, |x| almide_rt_value_decode_list(x, |e| almide_rt_value_as_int(&e))) }
pub fn almide_rt___decode_default_list_float(v: &AlmideValue, key: AlmideString, default: Vec<f64>) -> Result<Vec<f64>, AlmideString> { almide_rt_value_decode_with_default(v, &key, default, |x| almide_rt_value_decode_list(x, |e| almide_rt_value_as_float(&e))) }
pub fn almide_rt___decode_default_list_bool(v: &AlmideValue, key: AlmideString, default: Vec<bool>) -> Result<Vec<bool>, AlmideString> { almide_rt_value_decode_with_default(v, &key, default, |x| almide_rt_value_decode_list(x, |e| almide_rt_value_as_bool(&e))) }

// ── AlmideValue utilities ──

/// Object keys in insertion order. Lives in the `value` runtime (not reusing
/// `almide_json_keys`) so a program that uses `value.__keys` but never `json` still
/// links — the native runtime is included per-module, so a cross-module reference
/// would be undefined on a value-only program (#416 native-link fix).
pub fn almide_rt_value_keys(v: &AlmideValue) -> Vec<AlmideString> {
    let __almide_v: Vec<String> = {
        match v {
            AlmideValue::Object(entries) => entries.iter().map(|(k, _)| k.to_string()).collect(),
            _ => vec![],
        }
    };
    __almide_v.into_almide()
}

/// Pick specific keys from an Object, discarding the rest. A non-object has
/// none of the keys, so it picks to the empty object (C-401, #4112): an
/// allowlist over a value that is not an object lets nothing through, where
/// it used to hand the value back whole.
pub fn almide_rt_value_pick(v: &AlmideValue, keys: &[AlmideString]) -> AlmideValue {
    match v {
        AlmideValue::Object(pairs) => {
            // One hash set of the key list (#4207): O(n + k), not a scan of
            // `keys` per field.
            let want: std::collections::HashSet<&str> = keys.iter().map(|x| x.as_str()).collect();
            AlmideValue::Object(pairs.iter().filter(|(k, _)| want.contains(k.as_ref())).cloned().collect())
        }
        _ => AlmideValue::Object(Vec::new()),
    }
}

/// Merge two Objects. Keys from `b` override keys from `a`.
pub fn almide_rt_value_merge(a: &AlmideValue, b: &AlmideValue) -> AlmideValue {
    match (a, b) {
        (AlmideValue::Object(pa), AlmideValue::Object(pb)) => {
            // The FIRST slot holding each key, built once (#4207): O(|a| + |b|)
            // where a scan of the growing result per key of b was O(|a|·|b|).
            // The fold is unchanged — a key of b overrides the first of a's
            // repeats in place, a new key is appended at its first position
            // and later repeats in b override that slot, so b's last value wins.
            let mut out = pa.clone();
            let mut at: std::collections::HashMap<&str, usize> = std::collections::HashMap::with_capacity(pa.len() + pb.len());
            for (i, (k, _)) in pa.iter().enumerate() {
                at.entry(k.as_ref()).or_insert(i);
            }
            for (k, v) in pb {
                match at.get(k.as_ref()) {
                    Some(&pos) => out[pos] = (k.clone(), v.clone()),
                    None => {
                        at.insert(k.as_ref(), out.len());
                        out.push((k.clone(), v.clone()));
                    }
                }
            }
            AlmideValue::Object(out)
        }
        (_, b) => b.clone(),
    }
}

/// Remove specific keys from an Object.
pub fn almide_rt_value_omit(v: &AlmideValue, keys: &[AlmideString]) -> AlmideValue {
    match v {
        AlmideValue::Object(pairs) => {
            let drop: std::collections::HashSet<&str> = keys.iter().map(|x| x.as_str()).collect();
            AlmideValue::Object(pairs.iter().filter(|(k, _)| !drop.contains(k.as_ref())).cloned().collect())
        }
        other => other.clone(),
    }
}



// ── Variant decode helper ──

/// Extract the tag and payload from a tagged variant object {"Tag": payload}
pub fn almide_rt_value_tagged_variant(v: AlmideValue) -> Result<(AlmideString, AlmideValue), AlmideString> {
    let __almide_v: Result<(String, AlmideValue), AlmideString> = {
        let __almide_r: Result<(String, AlmideValue), String> = (|| {
            match v {
                AlmideValue::Object(pairs) => {
                    let n = pairs.len();
                    match (n, pairs.into_iter().next()) {
                        (1, Some((tag, payload))) => Ok((tag.into_owned(), payload)),
                        _ => Err(format!("expected object with exactly 1 key for variant, got {} keys", n)),
                    }
                }
                _ => Err("expected Object for variant decode".to_string()),
            }
        })();
        __almide_r.map_err(Into::into)
    };
    __almide_v.into_almide()
}

// ── Stringify ──

/// A Float leaf of a Value in its JSON text: Rust's `{}` Display for a finite
/// value (`3`, `-0`, `1e300`), and `null` for NaN and ±infinity (#2499, C-356):
/// JSON has no spelling for them, `json.parse` refused the `NaN` / `inf` /
/// `-inf` this used to write, and the two lineages (refuse: Go, Ruby; write
/// `null`: JavaScript, serde_json) leave `null` as the only total answer for a
/// `String`-returning stringify. Shared by every leg: the self-hosted
/// `__vstr_float` (stdlib/value_core.almd), the structural wasm helper
/// (crates/almide-wasm/src/value_helpers.rs), the pretty printer below in
/// json.rs, and the interp oracle.
pub fn almide_rt_value_float_json(f: f64) -> AlmideString { let __almide_v: String = { if f.is_finite() { format!("{}", f) } else { "null".to_string() } }; __almide_v.into_almide() }
/// The ONE JSON string-escaping rule (RFC 8259 §7, #2802), shared by every
/// serializer in this runtime — compact `almide_rt_value_stringify` (values
/// and keys), the pretty printer in json.rs, and the SSE response builder:
/// `\` → `\\`, `"` → `\"`, U+000A / U+000D / U+0009 → `\n` / `\r` / `\t`,
/// every OTHER U+0000..U+001F → `\u00xx` (lowercase hex, no `\b` / `\f` short
/// forms), and everything else — U+007F and all non-ASCII included — raw
/// UTF-8. The self-hosted `__json_quote` (stdlib/value_core.almd), the
/// structural wasm `$vjson_quote` (crates/almide-wasm/src/json_helpers.rs) and
/// the interp's `DynNode::to_json` are the other copies; the three-way byte
/// identity over all 32 control characters plus `"` and `\` is gated by
/// tests/json_quote_matrix_test.rs.
///
/// Every byte the rule rewrites is ASCII, so the text between two of them is
/// copied as one slice (it always ends on a char boundary) instead of one
/// `push(char)` per char (#2157).
pub fn almide_rt_value_json_escape_into(out: &mut String, s: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let b = s.as_bytes();
    let mut run = 0;
    for (i, &c) in b.iter().enumerate() {
        if c != b'\\' && c != b'"' && c >= 0x20 {
            continue;
        }
        out.push_str(&s[run..i]);
        run = i + 1;
        match c {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            _ => {
                out.push_str("\\u00");
                out.push(HEX[(c >> 4) as usize] as char);
                out.push(HEX[(c & 15) as usize] as char);
            }
        }
    }
    out.push_str(&s[run..]);
}
/// `almide_rt_value_json_escape_into` wrapped in the surrounding quotes.
pub fn almide_rt_value_json_quote(s: &str) -> AlmideString {
    let __almide_v: String = {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        almide_rt_value_json_escape_into(&mut out, s);
        out.push('"');
        out
    };
    __almide_v.into_almide()
}
/// The compact JSON text of `v`, written into ONE buffer (#2157). The text
/// used to be assembled bottom-up — a `String` per node, a `Vec<String>` per
/// container, a `format!` and a `join` to splice them — so every byte was
/// copied once per nesting level and each leaf cost several allocations:
/// 50 000 four-field records took longer to stringify natively than the whole
/// round trip took on the wasm leg. The bytes are unchanged.
pub fn almide_rt_value_stringify(v: &AlmideValue) -> AlmideString {
    let mut out = String::new();
    almide_value_write_json(&mut out, v);
    out.into()
}

fn almide_value_write_json(out: &mut String, v: &AlmideValue) {
    match v {
        AlmideValue::Null => out.push_str("null"),
        AlmideValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        AlmideValue::Int(n) => almide_value_push_int(out, *n),
        AlmideValue::Float(f) => out.push_str(&almide_rt_value_float_json(*f)),
        AlmideValue::Str(s) => almide_value_push_quoted(out, s),
        AlmideValue::Array(items) => {
            out.push('[');
            for (i, x) in items.iter().enumerate() {
                if i > 0 { out.push(','); }
                almide_value_write_json(out, x);
            }
            out.push(']');
        }
        AlmideValue::Object(pairs) => {
            out.push('{');
            for (i, (k, x)) in pairs.iter().enumerate() {
                if i > 0 { out.push(','); }
                almide_value_push_quoted(out, k);
                out.push(':');
                almide_value_write_json(out, x);
            }
            out.push('}');
        }
    }
}

fn almide_value_push_quoted(out: &mut String, s: &str) {
    out.push('"');
    almide_rt_value_json_escape_into(out, s);
    out.push('"');
}

/// Decimal digits of `n`, as `n.to_string()` writes them, without the
/// formatter and the intermediate `String`.
fn almide_value_push_int(out: &mut String, n: i64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    let mut m = n.unsigned_abs();
    loop {
        i -= 1;
        buf[i] = b'0' + (m % 10) as u8;
        m /= 10;
        if m == 0 { break; }
    }
    if n < 0 { out.push('-'); }
    out.push_str(std::str::from_utf8(&buf[i..]).unwrap_or_default());
}

// `AlmideValue` participates in derived `Repr` and string interpolation like any
// other type. A record/variant with a `AlmideValue` field generates both
// `self.<field>.almide_repr()` (the `AlmideRepr` path) and `{}` (the `Display`
// path used by the `<Type>_repr` free fn and `"${t}"` interpolation), so `AlmideValue`
// must impl both — without either, such a type fails to compile (E0599 / E0277).
// Both render the AlmideValue as its JSON text, identically, so the field reprs
// consistently across every path — and byte-identically to wasm, which reprs a
// `AlmideValue` through the same JSON serializer (see `emit_repr_value`).
// ADR-0013: `AlmideValue` already holds `AlmideString` (step 6), so it
// crosses the runtime boundary unchanged.
almide_into_str_leaf!(AlmideValue);

impl AlmideRepr for AlmideValue {
    fn almide_repr(&self) -> String { String::from(almide_rt_value_stringify(self)) }
}

impl std::fmt::Display for AlmideValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&almide_rt_value_stringify(self))
    }
}

// json_parse and json_stringify moved to json.rs
