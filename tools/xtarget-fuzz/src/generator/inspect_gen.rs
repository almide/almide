//! Program text for `xtarget-fuzz inspect` (proofs/stdlib-inspection.toml):
//! from one catalogued signature, the timing programs of criterion 2, the
//! diamond project of criterion 5. Text only — the runs and verdicts live in
//! `src/inspect.rs`.
//!
//! Instantiation is fixed, not random: every free type variable is `Int`
//! (a pinned Map-key / Result-error slot is `String`), so the same function
//! always yields the same programs and two runs measure the same thing.

use std::collections::HashMap;
use std::fmt::Write;

use super::catalogue::Signature;
use super::nominal;
use super::sig_type::SigType;
use super::types::{render_print, GenType};
use super::Project;

/// One function, instantiated, with the parameter its size is measured in.
#[derive(Debug, Clone)]
pub struct Target {
    pub name: String,
    params: Vec<(String, SigType)>,
    subst: HashMap<String, GenType>,
    ret: GenType,
    /// The parameter scaled to `n` (the first List / Map / Set / String / Bytes / Matrix / Value one, else a
    /// size-like Int); `None`: the function has no size to scale.
    pub size_param: Option<usize>,
    /// A parameter of the return type: the function can rebind it in a loop,
    /// a tail call and a fold (criterion 2's use-shapes).
    pub rebind_param: Option<usize>,
    /// The `mut` parameter: bound with `var`, and in the diamond each path
    /// mutates its own copy and prints it.
    mut_param: Option<usize>,
    /// The function's [`SHAPE_RECIPES`] entry, if it has one.
    recipe: Option<&'static Recipe>,
    /// The function's [`ROUTED`] entry: its probes run inside a handler the
    /// router called, the routed request filling that parameter.
    routed: Option<&'static Routed>,
    /// An in-domain argument derived from the declaration ([`Domain`]),
    /// for the parameter at the index, when no recipe names the function.
    domain: Option<(usize, Domain)>,
    /// A type variable was free (not pinned by the signature): the function
    /// is inspected at a heap instantiation too ([`Self::new_filled`]).
    pub generic: bool,
    /// An `effect fn` (a hostless one, catalogue.rs): the diamond's paths
    /// are effect functions too.
    effect: bool,
    /// The decoder the result is observed through ([`super::catalogue::LEG_DEPENDENT_ENCODERS`]).
    observe: Option<&'static str>,
}

/// An argument inside the function's domain, derived from its declaration
/// rather than from its name (#3671): the generic values made the parsers
/// return `err`, the checked conversions `none`, and those runs measured the
/// refusal instead of the function.
#[derive(Debug, Clone, PartialEq)]
enum Domain {
    /// A narrowing conversion (an Int or Float to `Option[<fixed width>]`):
    /// the integral value 2, which every fixed-width type holds. The generic
    /// 1.5 is refused as non-integral, and a size-like `n` leaves Int8 / UInt8
    /// past 127.
    Narrow(GenType),
    /// The inverse of a decoder `X -> Result[P]` / `Option[P]`: what its
    /// module's own producer `P -> X` makes of a payload — a payload of size
    /// n when P has a size. A numeral (an Int or Float printed by its own
    /// module) has no size of its own, so it scales by n leading zero digits,
    /// which every positional numeral reads as the same number.
    Inverse { producer: String, payload: GenType, numeral: bool },
}

impl Domain {
    fn expr(&self, n: u64) -> String {
        match self {
            Domain::Narrow(GenType::Float) => "2.0".into(),
            Domain::Narrow(_) => "2".into(),
            Domain::Inverse { producer, payload, numeral: true } => format!("string.repeat(\"0\", {n}) + {producer}({})", small(payload)),
            Domain::Inverse { producer, payload, numeral: false } if has_size(payload) => format!("{producer}({})", sized(payload, n)),
            Domain::Inverse { producer, payload, .. } => format!("{producer}({})", small(payload)),
        }
    }

    /// The argument grows with n.
    fn scales(&self) -> bool {
        matches!(self, Domain::Inverse { numeral: true, .. }) || matches!(self, Domain::Inverse { payload, .. } if has_size(payload))
    }

    /// The domain of a one-parameter function returning a Result or an
    /// Option, from its signature and its module's catalogued functions.
    fn derive(sig: &Signature, x: &GenType, ret: &GenType, catalogue: &[Signature]) -> Option<Domain> {
        let payload = match ret {
            GenType::Result(p) | GenType::Option(p) => (**p).clone(),
            _ => return None,
        };
        if matches!((ret, &payload, x), (GenType::Option(_), GenType::Sized(_), GenType::Int | GenType::Float)) {
            return Some(Domain::Narrow(x.clone()));
        }
        // Producers are matched on closed types only: a generic one has no
        // fixed `P -> X` to invert.
        let closed = HashMap::new();
        let producers: Vec<&Signature> = catalogue
            .iter()
            .filter(|g| g.module == sig.module && g.func != sig.func && !g.effect && g.mut_param.is_none() && g.params.len() == 1)
            .filter(|g| g.params[0].instantiate(&closed).as_ref() == Some(&payload) && g.ret.instantiate(&closed).as_ref() == Some(x))
            .collect();
        let producer = match producers.as_slice() {
            [g] => *g,
            many => {
                let dual = inverse_name(&sig.func)?;
                *many.iter().find(|g| g.func == dual)?
            }
        };
        // A numeral is what the number type's own module prints it as
        // (`int.to_string`, `float.to_string`); `string.from_codepoint` also
        // makes a String from an Int, but a character, not a numeral.
        let numeral = *x == GenType::String && matches!((&payload, producer.module), (GenType::Int, "int") | (GenType::Float, "float"));
        Some(Domain::Inverse { producer: format!("{}.{}", producer.module, producer.func), payload, numeral })
    }
}

/// The function a decoder undoes, when its module has more than one
/// producer of the decoder's input (`int.to_string` and `int.to_hex` both
/// print an Int): by the stdlib's naming convention `from_X` undoes `to_X`,
/// `decode..` undoes `encode..`, `parse_X` undoes `to_X`, and `parse`
/// undoes `to_string`. A
/// convention over name SHAPES, not a list of functions: a decoder that
/// follows it is covered when it is added, and one that does not is left to
/// the generic values (and to [`SHAPE_RECIPES`]).
fn inverse_name(f: &str) -> Option<String> {
    if let Some(x) = f.strip_prefix("from_") {
        return Some(format!("to_{x}"));
    }
    if f.contains("decode") {
        return Some(f.replace("decode", "encode"));
    }
    if let Some(x) = f.strip_prefix("parse_") {
        return Some(format!("to_{x}"));
    }
    (f == "parse").then(|| "to_string".to_string())
}

/// A value of `t` has a size [`sized`] can grow.
fn has_size(t: &GenType) -> bool {
    matches!(
        t,
        GenType::List(_)
            | GenType::Map(_)
            | GenType::Set(_)
            | GenType::String
            | GenType::Bytes
            | GenType::Matrix
            | GenType::Value
            | GenType::Nominal(_)
    )
}

const SIZE_LIKE_INT: &[&str] = &["n", "count", "times", "len", "size", "end", "width"];

/// Element-typed parameters a search compares against: given a value no
/// sized input holds, so the search scans to the end (its worst case).
const SEARCH_PARAMS: &[&str] = &["x", "value", "elem", "item", "needle", "target", "sub", "pattern", "from"];

/// The arguments of a function whose operands must agree in shape (#3637):
/// each named parameter takes its expression, `{n}` standing for the size,
/// and `size` is the parameter the size is measured in (when the recipe
/// gives it no expression it takes the generic sized value). The generic
/// values (`matrix.zeros(n, 2)`, 2×2 operands, 2 for every count) violate
/// these functions' preconditions, so every shape exited 1.
///
/// A recipe also declares its input in-domain: every probe of the function
/// stops with [`OFF_DOMAIN`] when the call still returns `none` or `false`
/// (and with [`ERR_PATH`] on `err`, as every Result-returning call does).
#[derive(Debug)]
struct Recipe {
    func: &'static str,
    size: &'static str,
    args: &'static [(&'static str, &'static str)],
}

/// Rope: 2 heads × head_dim 2 fill the 4 columns; n is the row (position)
/// count.
const ROPE: &[(&str, &str)] = &[("x", "matrix.zeros({n}, 4)"), ("n_heads", "2"), ("head_dim", "2")];

/// n keys `k<i * 7919 mod 1000003>`: distinct (7919 is invertible mod the
/// prime) and in no run of consecutive numerals.
const SCATTERED_MAP: &str = "list.range(0, {n}) |> list.fold(map.new(), (m, i) => map.set(m, \"k${i * 7919 % 1000003}\", {small_elem}))";
/// The present key of the map's `({i} * 7919 mod n)`-th entry.
const SCATTERED_KEY: &str = "\"k${{i} * 7919 % {n} * 7919 % 1000003}\"";

/// A response of n headers named `k` + the digits of `100000000 + i` least
/// significant first: two names meet at a position independent of n.
const FLAT_NAMED_RESPONSE: &str = "http.with_headers(200, \"aé\", (list.range(0, {n}) |> list.fold(map.new(), (m, i) => map.set(m, \"k\" + string.reverse(\"${100000000 + i}\"), \"v\"))))";
/// The last of those names.
const FLAT_LAST_NAME: &str = "\"k\" + string.reverse(\"${100000000 + {n} - 1}\")";

/// n bytes with no short period, for the zlib codecs.
const CODEC_BYTES: &str = "bytes.from_list(list.range(0, {n}) |> list.map((i) => i * 7919 % 251))";

mod effect_recipes;

const SHAPE_RECIPES: &[Recipe] = &[
    Recipe { func: "matrix.rope_rotate", size: "x", args: ROPE },
    Recipe { func: "matrix.rope_rotate_at", size: "x", args: ROPE },
    Recipe { func: "matrix.rope_rotate_neox_at", size: "x", args: ROPE },
    // Causal attention needs rows(q) <= rows(k), and k / v as wide as q:
    // n is the key/value length (a KV cache) against 2 query rows, 4 wide
    // over 2 heads. The rebound q keeps its 2 × 4 shape.
    Recipe {
        func: "matrix.masked_multi_head_attention",
        size: "k",
        args: &[("q", "matrix.zeros(2, 4)"), ("k", "matrix.zeros({n}, 4)"), ("v", "matrix.zeros({n}, 4)"), ("n_heads", "2")],
    },
    // A (T, in_ch) input, an (out_ch, in_ch × kernel) weight, one bias per
    // out_ch; kernel 3, stride 1, padding 1 give T output rows of out_ch =
    // in_ch, so the rebound input keeps its shape. n is T.
    Recipe {
        func: "matrix.conv1d",
        size: "input",
        args: &[
            ("input", "matrix.zeros({n}, 2)"),
            ("weight", "matrix.zeros(2, 6)"),
            ("bias", "[0.5, 1.5]"),
            ("kernel", "3"),
            ("stride", "1"),
            ("padding", "1"),
        ],
    },
    // Decoders must be timed on input they accept (#3666): the generic
    // `string.repeat("aé", n)` is refused at its first character, so every
    // shape measured the early-error exit. n bytes of every value, encoded;
    // the encoding runs once, outside the timed calls.
    Recipe { func: "base64.decode", size: "s", args: &[("s", "base64.encode(bytes.from_list(list.range(0, {n}) |> list.map((i) => i % 256)))")] },
    Recipe {
        func: "base64.decode_url",
        size: "s",
        args: &[("s", "base64.encode_url(bytes.from_list(list.range(0, {n}) |> list.map((i) => i % 256)))")],
    },
    // 2n hex digits.
    Recipe { func: "hex.decode", size: "s", args: &[("s", "hex.encode(bytes.from_list(list.range(0, {n}) |> list.map((i) => i % 256)))")] },
    // A JSON array of n fixed-width ints: its length is a·n + b.
    Recipe {
        func: "json.parse",
        size: "text",
        args: &[("text", "\"[\" + (list.range(0, {n}) |> list.map((i) => int.to_string(100000000 + i)) |> list.join(\",\")) + \"]\"")],
    },
    // The entries below are keyed by name because what makes their input
    // valid is stated only in the doc comment, and no type separates it
    // (#3671). `String -> Bool` is the type of every validator: the class of
    // characters each accepts ("All ASCII 0-9", "All whitespace", "Has
    // letters, all uppercase") is prose, and the generic "aéaé…" is refused
    // at its first character, so the full scan was never measured.
    Recipe { func: "string.is_digit", size: "s", args: &[("s", "string.repeat(\"0123456789\", {n})")] },
    Recipe { func: "string.is_whitespace", size: "s", args: &[("s", "string.repeat(\" \\t\", {n})")] },
    // Uppercase, with the two-byte letter the generic string carries.
    Recipe { func: "string.is_upper", size: "s", args: &[("s", "string.repeat(\"AÉ\", {n})")] },
    // UTF-8 byte offsets "none if bad or mid-char": the generic end 2 lands
    // inside `é`. 3 is the end of one whole "aé".
    Recipe { func: "string.byte_slice", size: "s", args: &[("s", "string.repeat(\"aé\", {n})"), ("start", "0"), ("end", "3")] },
    // A lookup "err / none if missing": the key the sized object holds last
    // (its keys are `k<100000000 + i>`), so the hit scans the whole object.
    Recipe { func: "value.field", size: "v", args: &[("key", "\"k${100000000 + {n} - 1}\"")] },
    Recipe { func: "json.get_int", size: "j", args: &[("key", "\"k${100000000 + {n} - 1}\"")] },
    // The header lookups "none / empty if missing": the sized response and
    // request carry n headers keyed the same way (nominal.rs).
    Recipe { func: "http.get_header", size: "resp", args: &[("resp", FLAT_NAMED_RESPONSE), ("key", FLAT_LAST_NAME)] },
    // header_values compares every name with the key: names whose digits run
    // least significant first meet the key's at a position independent of n
    // (see `{sized_flat}`); "k1000xxxxx" against the last key drifted the
    // per-name cost on both legs (rms 8%, no class).
    Recipe {
        func: "http.header_values",
        size: "resp",
        args: &[("resp", FLAT_NAMED_RESPONSE), ("key", FLAT_LAST_NAME)],
    },
    Recipe {
        func: "http.req_header",
        size: "req",
        args: &[("req", "http.new_request(\"GET\", \"/p/1\", \"aé\", (list.range(0, {n}) |> list.fold(map.new(), (m, i) => map.set(m, \"k\" + string.reverse(\"${100000000 + i}\"), \"v\"))))"), ("key", FLAT_LAST_NAME)],
    },
    // The path lookups "none / err if the path is missing": the last key of
    // the sized object, so the hit walks it whole.
    Recipe { func: "json.get_path", size: "j", args: &[("path", "json.field(json.root(), \"k${100000000 + {n} - 1}\")")] },
    Recipe { func: "json.set_path", size: "j", args: &[("path", "json.field(json.root(), \"k${100000000 + {n} - 1}\")")] },
    Recipe { func: "json.remove_path", size: "j", args: &[("path", "json.field(json.root(), \"k${100000000 + {n} - 1}\")")] },
    // A router refuses two routes with one pattern: n distinct patterns.
    Recipe {
        func: "http.router",
        size: "routes",
        args: &[(
            "routes",
            "list.range(0, {n}) |> list.map((i) => http.route(\"GET /r${100000000 + i}/{id}\", (req) => http.response(200, http.param(req, \"id\") ?? \"-\")))",
        )],
    },
    // A body of n fixed-width ints the decode callback accepts (`ok` of its
    // length, `{as_t}` making the length a T), so the parse and the decode
    // both run whole and the call returns ok: the 400 responses are its
    // refusals.
    Recipe {
        func: "http.decode_json",
        size: "req",
        args: &[
            ("req", "http.new_request(\"POST\", \"/j\", \"[\" + (list.range(0, {n}) |> list.map((i) => int.to_string(100000000 + i)) |> list.join(\",\")) + \"]\", map.new())"),
            ("decode", "(v) => match value.as_array(v) { ok(xs) => ok({as_t}(list.len(xs))), err(e) => err(e) }"),
        ],
    },
    // The name the route binds: the probes run inside its handler, the
    // routed request filling `req` ([`ROUTED`]), so the lookup hits.
    Recipe { func: "http.param", size: "req", args: &[("name", "\"id\"")] },
    // n pass-through middleware: what `wrap` composes.
    Recipe { func: "http.wrap", size: "middleware", args: &[("middleware", "list.range(0, {n}) |> list.map((i) => (next) => next)")] },
    // A pattern is not a needle (SEARCH_PARAMS): n copies of every directive.
    Recipe { func: "datetime.format", size: "pattern", args: &[("pattern", "string.repeat(\"%Y-%m-%d %H:%M:%S|\", {n})")] },
    // The Float32 constructors take their size as a row count.
    Recipe { func: "matrix.zeros_f32", size: "rows", args: &[("rows", "{n}"), ("cols", "2")] },
    Recipe { func: "matrix.ones_f32", size: "rows", args: &[("rows", "{n}"), ("cols", "2")] },
    // Percent-escapes and `+` to decode, n of each: the generic "aéaé…" has
    // nothing to decode.
    Recipe { func: "http.url_decode", size: "s", args: &[("s", "string.repeat(\"a%C3%A9+\", {n})")] },
    // Deleting an absent key is a no-op. The plain call's per-iteration
    // step re-sets "k0" (see `vary`), so deleting "k0" hits on every call,
    // and the map holds "k0" from the start for the diamond.
    Recipe {
        func: "map.delete",
        size: "m",
        args: &[("m", "map.set({sized}, \"k0\", {small_elem})"), ("key", "\"k0\"")],
    },
    // The asserts' passing path, the one a stamp claims: a failing assert
    // aborts the probe. `contains` and `snapshot` scale with the strings
    // they compare (the needle at the end, two equal strings).
    Recipe { func: "testing.assert_gt", size: "a", args: &[("a", "{n}"), ("b", "1")] },
    Recipe { func: "testing.assert_lt", size: "b", args: &[("a", "1"), ("b", "{n}")] },
    Recipe { func: "testing.assert_approx", size: "a", args: &[("a", "int.to_float({n})"), ("b", "int.to_float({n})"), ("tolerance", "0.5")] },
    Recipe { func: "testing.assert_contains", size: "haystack", args: &[("haystack", "string.repeat(\"a\", {n}) + \"b\""), ("needle", "\"b\"")] },
    Recipe { func: "testing.assert_snapshot", size: "actual", args: &[("actual", "string.repeat(\"aé\", {n})"), ("expected", "string.repeat(\"aé\", {n})")] },
    Recipe { func: "testing.assert_some", size: "opt", args: &[("opt", "some({sized_elem})")] },
    Recipe { func: "testing.assert_ok", size: "result", args: &[("result", "ok({sized_elem})")] },
    Recipe { func: "testing.assert_none", size: "", args: &[("opt", "none")] },
    Recipe { func: "testing.assert_err", size: "", args: &[("result", "err(\"e\")")] },
    // Dedup builders over scattered elements (see `{sized_scattered}`).
    Recipe { func: "set.from_list", size: "xs", args: &[("xs", "{sized_scattered}")] },
    Recipe { func: "list.unique", size: "xs", args: &[("xs", "{sized_scattered}")] },
    // The minimum's comparisons at a cost independent of n (`{sized_flat}`).
    Recipe { func: "list.min", size: "xs", args: &[("xs", "{sized_flat}")] },
    // A lookup's key walks the present keys, one per step: a single fixed
    // key measured one probe path (see `per_step`), and a hit is the
    // success path the stamp claims. The keys are scattered (SCATTERED_MAP):
    // with n consecutive numerals the wasm index's mean probe fell by 20%
    // from n = 20000 to 80000 and then held, a series no class fits.
    Recipe { func: "map.get", size: "m", args: &[("m", SCATTERED_MAP), ("key", SCATTERED_KEY)] },
    Recipe { func: "map.get_or", size: "m", args: &[("m", SCATTERED_MAP), ("key", SCATTERED_KEY)] },
    Recipe { func: "map.contains", size: "m", args: &[("m", SCATTERED_MAP), ("key", SCATTERED_KEY)] },
    Recipe {
        func: "map.insert",
        size: "m",
        args: &[("m", SCATTERED_MAP), ("key", SCATTERED_KEY), ("value", "{small}")],
    },
    // The count is the size, whatever the element: at String the element
    // is otherwise the first sized parameter, and the probe measured copying
    // one long string twice.
    Recipe { func: "list.repeat", size: "n", args: &[("n", "{n}")] },
    Recipe { func: "list.iterate", size: "n", args: &[("n", "{n}")] },
    // The codecs over n bytes that do not repeat in any short period (a run
    // of zeros is one LZ77 match on native); a decoder's input is its own
    // encoder's output, so the run decodes successfully on both legs.
    Recipe { func: "zlib.compress", size: "data", args: &[("data", CODEC_BYTES)] },
    Recipe { func: "zlib.compress_level", size: "data", args: &[("data", CODEC_BYTES), ("level", "6")] },
    Recipe { func: "zlib.deflate", size: "data", args: &[("data", CODEC_BYTES)] },
    Recipe { func: "zlib.deflate_level", size: "data", args: &[("data", CODEC_BYTES), ("level", "6")] },
    Recipe { func: "zlib.gzip", size: "data", args: &[("data", CODEC_BYTES)] },
    Recipe { func: "zlib.decompress", size: "data", args: &[("data", "zlib.compress(bytes.from_list(list.range(0, {n}) |> list.map((i) => i * 7919 % 251)))!")] },
    Recipe { func: "zlib.inflate", size: "data", args: &[("data", "zlib.deflate(bytes.from_list(list.range(0, {n}) |> list.map((i) => i * 7919 % 251)))!")] },
    Recipe { func: "zlib.gunzip", size: "data", args: &[("data", "zlib.gzip(bytes.from_list(list.range(0, {n}) |> list.map((i) => i * 7919 % 251)))!")] },
    // Native stdout is a 64 KiB BufWriter: a shorter write is copied into it,
    // a longer one goes straight to the kernel, where neither meter counts it.
    // n bytes straddled the capacity (moved n per call to 40000, then 0), so
    // the writes are 4n bytes: every size is past it, in one regime.
    Recipe { func: "io.write", size: "data", args: &[("data", "bytes.from_list(list.range(0, 4 * {n}) |> list.map((i) => 97 + i % 26))")] },
    Recipe { func: "io.write_bytes", size: "data", args: &[("data", "list.range(0, 4 * {n}) |> list.map((i) => 97 + i % 26)")] },
    // zip / zip_with stop at the shorter list: against the generic 2-element
    // partner the result had 2 elements whatever n was, so the order measured
    // was the truncation's. The partner takes the sized one's length.
    Recipe { func: "list.zip", size: "xs", args: &[("ys", "{sized}")] },
    Recipe { func: "list.zip_with", size: "xs", args: &[("ys", "{sized}")] },
    // Elementwise operands "truncate to overlap": against the generic 2×2
    // operand the result is 2×2 whatever n is, so the order measured was the
    // truncation's. Every operand takes the n × 2 shape of the sized one.
    Recipe { func: "matrix.add", size: "a", args: &[("b", "matrix.zeros({n}, 2)")] },
    Recipe { func: "matrix.sub", size: "a", args: &[("b", "matrix.zeros({n}, 2)")] },
    Recipe { func: "matrix.fma", size: "a", args: &[("b", "matrix.zeros({n}, 2)")] },
    Recipe { func: "matrix.fma3", size: "a", args: &[("b", "matrix.zeros({n}, 2)"), ("c", "matrix.zeros({n}, 2)")] },
    // "No-op if it won't fit" / "clamped to fit": the generic 2 for every
    // offset copied an empty range (2..2) and read past a 2-byte source.
    // Distinct byte values, so the diamond shows the copy.
    Recipe {
        func: "bytes.copy_within",
        size: "b",
        args: &[("b", "bytes.from_list(list.range(0, {n}) |> list.map((i) => i % 256))"), ("src_start", "0"), ("src_end", "2"), ("dst", "2")],
    },
    Recipe { func: "bytes.copy_from", size: "dst", args: &[("src_off", "0")] },
    // A pattern the sized "aéaé…" matches whole; the generic pattern was
    // the sized string and the subject 2 characters, so it never matched.
    Recipe { func: "regex.full_match", size: "s", args: &[("pat", "\"(aé)*\"")] },
];

/// A function whose in-domain input exists only inside a request handler
/// the router called: `http.param` answers `some` only for a request the
/// router bound path parameters into, and the binding (`http.__with_params`)
/// is private to the router. Every probe of the function runs inside the
/// handler of a one-route router, the routed request filling `param`; the
/// program calls the router once with a request to `target` (`{n}` the
/// size) and prints what the handler answered. The routing runs in the
/// probe and in its baseline alike, so its cost cancels out of the
/// difference: the timed calls are the function's own. A router that did
/// not route the request (any status but 200) stops the probe with
/// [`OFF_DOMAIN`].
#[derive(Debug)]
struct Routed {
    func: &'static str,
    /// The parameter the routed request fills.
    param: &'static str,
    /// The route's pattern.
    pattern: &'static str,
    /// The request target, `{n}` standing for the size.
    target: &'static str,
}

/// `{id}` bound to n percent-escaped `aé` (3n bytes decoded): param's cost
/// is the copy of the value it finds. One routing of a path of length L
/// costs O(L²) on both legs (the router's escape check walks the path's
/// characters with `list.drop`), outside the timed difference but inside
/// the run cap: the probe is run at a reduced `--n`.
const ROUTED: &[Routed] = &[Routed { func: "http.param", param: "req", pattern: "GET /p/{id}", target: "\"/p/\" + string.repeat(\"a%C3%A9\", {n})" }];

/// The panic a probe program stops with when a call whose input is
/// in-domain (a [`SHAPE_RECIPES`] entry or a derived [`Domain`]) returned
/// `none` or `false`: it refused the input, and that run measured the
/// refusal (#3671). `src/inspect.rs` reads it off the run's stderr.
pub const OFF_DOMAIN: &str = "inspect: the probed call refused its in-domain input";

/// The panic a probe program stops with when the probed call returned `err`:
/// it took the error path, and a run that did is a failed run, never a
/// measurement (#3666). `src/inspect.rs` reads it off the run's stderr.
pub const ERR_PATH: &str = "inspect: the probed call returned err";

/// Functions that stop at the first `true` from their predicate: given a
/// predicate that is always `false`, so they visit every element.
const STOPS_ON_TRUE: &[&str] = &["any", "find", "find_index", "position", "find_map", "index_where"];

impl Target {
    /// `catalogue` is where a decoder's inverse is looked up ([`Domain`]).
    /// Every free type variable is Int; [`Self::new_filled`] picks another.
    #[cfg(test)]
    pub fn new(sig: &Signature, catalogue: &[Signature]) -> Result<Target, String> {
        Self::new_filled(sig, catalogue, GenType::Int)
    }

    /// The function with every free type variable bound to `fill`. A
    /// generic function is inspected at Int AND at a heap type (String):
    /// its heap-element path — refcounts, slot copies — is a different
    /// lowering from Int's, and an Int-only measurement never ran it
    /// (wasm `list.flat_map` over Strings allocated O(n²) bytes while its
    /// Int stamp was clean, 2026-10-16).
    pub fn new_filled(sig: &Signature, catalogue: &[Signature], fill: GenType) -> Result<Target, String> {
        let name = format!("{}.{}", sig.module, sig.func);
        let mut subst = sig.forced_bindings();
        let mut vars = Vec::new();
        for p in &sig.params {
            p.collect_vars(&mut vars);
        }
        sig.ret.collect_vars(&mut vars);
        for v in vars {
            subst.entry(v).or_insert_with(|| fill.clone());
        }
        // A bodied effect fn's call is a `Result[T, String]` to the checker
        // (`Signature::lifted`): that is what the probe binds and wraps.
        let declared = match &sig.ret {
            SigType::Result(..) => sig.ret.clone(),
            r if sig.lifted => SigType::Result(Box::new(r.clone()), Box::new(SigType::String)),
            r => r.clone(),
        };
        let ret = declared.instantiate(&subst).ok_or_else(|| format!("{name}: return type has no value form"))?;
        let params: Vec<(String, SigType)> = sig.param_names.iter().cloned().zip(sig.params.iter().cloned()).collect();
        let concrete = |t: &SigType| t.instantiate(&subst);
        let recipe = SHAPE_RECIPES.iter().chain(effect_recipes::EFFECT_RECIPES).find(|r| r.func == name);
        let routed = ROUTED.iter().find(|r| r.func == name);
        let param_at = |p: &str| params.iter().position(|(n, _)| n == p);
        if let Some(r) = routed {
            if param_at(r.param).is_none_or(|i| concrete(&params[i].1) != Some(GenType::Nominal("HttpRequest"))) {
                return Err(format!("{name}: routed `{}` is not an HttpRequest parameter", r.param));
            }
            if recipe.is_none() {
                return Err(format!("{name}: a routed function needs a shape recipe (its in-domain input)"));
            }
        }
        if let Some(r) = recipe {
            // A recipe naming a parameter the signature no longer has would
            // silently fall back to the generic values that violate it.
            if let Some((p, _)) = r.args.iter().find(|(p, _)| param_at(p).is_none()) {
                return Err(format!("{name}: shape recipe names `{p}`, which is not a parameter"));
            }
            // An empty size: a sizeless function whose recipe only keeps
            // its operands in domain (an assert's passing argument).
            if !r.size.is_empty() && param_at(r.size).is_none() {
                return Err(format!("{name}: shape recipe's size `{}` is not a parameter", r.size));
            }
            if r.args.iter().any(|(p, e)| *p == r.size && !e.contains("{n}") && !e.contains("{sized")) {
                return Err(format!("{name}: shape recipe's size `{}` does not take n", r.size));
            }
        }
        // Effect functions name things the sandbox holds; their domain is
        // the sandbox's, not a value the harness derives.
        let domain = match (recipe, params.as_slice()) {
            (None, [(_, p)]) if !sig.effect => concrete(p).and_then(|x| Domain::derive(sig, &x, &ret, catalogue)).map(|d| (0, d)),
            _ => None,
        };
        let size_param = match (recipe, &domain) {
            (Some(r), _) => param_at(r.size),
            (None, Some((i, d))) => d.scales().then_some(*i),
            (None, None) => params
                .iter()
                .position(|(_, t)| concrete(t).as_ref().is_some_and(has_size))
                .or_else(|| params.iter().position(|(n, t)| *t == SigType::Int && SIZE_LIKE_INT.contains(&n.as_str()))),
        };
        let rebind_param = params.iter().position(|(_, t)| concrete(t).as_ref() == Some(&ret));
        let generic = {
            let forced = sig.forced_bindings();
            subst.keys().any(|k| !forced.contains_key(k))
        };
        Ok(Target {
            name,
            params,
            subst,
            ret,
            size_param,
            rebind_param,
            mut_param: sig.mut_param,
            recipe,
            routed,
            domain,
            generic,
            effect: sig.effect,
            observe: sig.observed_through,
        })
    }

    /// The probes give this function an input inside its domain (a recipe
    /// or a derived [`Domain`]), so a `none` / `false` from it is a refusal
    /// the probe stops on ([`OFF_DOMAIN`]), not a result to measure.
    pub fn in_domain(&self) -> bool {
        self.recipe.is_some() || self.domain.is_some()
    }

    /// The recipe's expression for parameter `i`, if it names it.
    fn recipe_arg(&self, i: usize) -> Option<&'static str> {
        let pname = &self.params[i].0;
        self.recipe?.args.iter().find(|(p, _)| p == pname).map(|(_, e)| *e)
    }

    /// The derived in-domain argument for parameter `i`, if there is one.
    fn domain_arg(&self, i: usize) -> Option<&Domain> {
        self.domain.as_ref().filter(|(at, _)| *at == i).map(|(_, d)| d)
    }

    /// Parameter `i`'s recipe expression at size `n`, `{i}` read as `counter`.
    fn recipe_expr(&self, i: usize, n: u64, counter: &str) -> String {
        let e = self.recipe_arg(i).unwrap_or_default();
        let ty = self.ty(i);
        let sized_v = ty.as_ref().map(|g| sized(g, n)).unwrap_or_default();
        let elem_v = ty.as_ref().and_then(elem_of).map(|g| small(&g)).unwrap_or_default();
        let small_v = ty.as_ref().map(small).unwrap_or_default();
        let sized_elem_v = ty.as_ref().and_then(elem_of).map(|g| sized(&g, n)).unwrap_or_default();
        // `{sized_flat}`: as `{sized}`, but a String element's digits run
        // least significant first, so two elements' common prefix does not
        // lengthen as n shrinks — "s100000000" against "s1000xxxxx" compares
        // more bytes at n = 20000 than at 320000, a per-element drift a
        // byte-wise comparison shows as sublinear growth.
        let flat_v = match &ty {
            Some(GenType::List(e)) if **e == GenType::String => {
                format!("list.range(0, {n}) |> list.map((i) => string.reverse(\"${{100000000 + i}}\"))")
            }
            _ => sized_v.clone(),
        };
        // `{sized_scattered}`: as `{sized}`, n distinct elements in no run of
        // consecutive numerals (`i * 7919 mod 1000003`), for an index keyed
        // on them: consecutive keys under a multiplicative hash cluster by n.
        let scattered_v = match &ty {
            Some(GenType::List(e)) if **e == GenType::Int => format!("list.range(0, {n}) |> list.map((i) => i * 7919 % 1000003)"),
            Some(GenType::List(e)) if **e == GenType::String => {
                format!("list.range(0, {n}) |> list.map((i) => \"s${{i * 7919 % 1000003}}\")")
            }
            _ => sized_v.clone(),
        };
        // `{as_t}`: the conversion of an Int to the instantiated `T` (a
        // generic recipe's callback returns a T at Int and at String alike).
        let as_t = match self.subst.get("T") {
            Some(GenType::String) => "int.to_string",
            _ => "",
        };
        e.replace("{sized_flat}", &flat_v)
            .replace("{sized_scattered}", &scattered_v)
            .replace("{sized}", &sized_v)
            .replace("{small_elem}", &elem_v)
            .replace("{sized_elem}", &sized_elem_v)
            .replace("{small}", &small_v)
            .replace("{as_t}", as_t)
            .replace("{n}", &n.to_string())
            .replace("{i}", counter)
    }

    /// Parameter `i` is built by an encoder whose bytes differ by leg
    /// (a decoder's input): its own digest would differ by leg too.
    fn leg_dependent_input(&self, i: usize) -> bool {
        self.recipe_arg(i)
            .is_some_and(|e| super::catalogue::LEG_DEPENDENT_ENCODERS.iter().any(|(enc, _)| e.contains(&format!("{enc}("))))
    }

    /// Parameter `i`'s recipe names the loop counter `{i}`: the direct
    /// program binds it inside the loop, every other program at `{i}` = 0.
    fn per_step(&self, i: usize) -> bool {
        self.recipe_arg(i).is_some_and(|e| e.contains("{i}"))
    }

    /// Parameter `i` is the routed request ([`Routed`]).
    fn is_routed(&self, i: usize) -> bool {
        self.routed.is_some_and(|r| self.params[i].0 == r.param)
    }

    /// `body` (a `main` body, its value the handler's answer) inside the
    /// handler of the one-route router [`Routed`] declares, and the `main`
    /// that routes a size-`n` request to it and, with `print_body`, prints
    /// what it answered (the diamond's handler prints its own lines, and an
    /// extra empty one would break their three-way comparison).
    fn in_handler(&self, r: &Routed, body: &str, n: u64, print_body: bool) -> String {
        let inner: String = body.lines().map(|l| if l.is_empty() { "\n".to_string() } else { format!("  {l}\n") }).collect();
        let target = r.target.replace("{n}", &n.to_string());
        format!(
            "effect fn main() -> Unit = {{\n  let h: HttpHandler = match http.router([http.route(\"{}\", (req) => {{\n{inner}  }})]) {{\n    ok(t) => t,\n    err(e) => panic(e),\n  }}\n  let res: HttpResponse = h(http.new_request(\"GET\", {target}, \"\", map.new()))!\n  if http.status_code(res) == 200 then () else panic(\"{OFF_DOMAIN}: the router answered ${{http.status_code(res)}}\")\n{}}}\n",
            r.pattern,
            if print_body { "  println(http.body(res))\n" } else { "" }
        )
    }

    /// Parameter `i`'s value grows with n.
    fn scales(&self, i: usize) -> bool {
        Some(i) == self.size_param
            || self.recipe_arg(i).is_some_and(|e| e.contains("{n}") || e.contains("{sized"))
            || self.domain_arg(i).is_some_and(Domain::scales)
    }

    fn ty(&self, i: usize) -> Option<GenType> {
        self.params[i].1.instantiate(&self.subst)
    }

    fn arg(&self, i: usize, n: u64) -> String {
        let (pname, t) = &self.params[i];
        // The routed request is the handler's own parameter.
        if self.is_routed(i) {
            return "req".into();
        }
        // A recipe spells its operands in the target's own instantiation:
        // `{sized}` is this parameter's sized value, `{small}` a small one,
        // `{small_elem}` a small element of it (a generic recipe holds at Int
        // and at String alike).
        if self.recipe_arg(i).is_some() {
            return self.recipe_expr(i, n, "0");
        }
        if let Some(d) = self.domain_arg(i) {
            return d.expr(n);
        }
        match t {
            SigType::Fn(ps, r) => self.lambda(ps, r),
            SigType::Int if Some(i) == self.size_param => n.to_string(),
            SigType::Int => small_int(pname).into(),
            _ if SEARCH_PARAMS.contains(&pname.as_str()) => self.ty(i).map(|g| absent(&g)).unwrap_or_else(|| "()".into()),
            _ => match self.ty(i) {
                Some(g) if Some(i) == self.size_param => sized(&g, n),
                Some(g) => small(&g),
                None => "()".into(),
            },
        }
    }

    /// `(p0, ..) => body`: a parameter of the return type passes through,
    /// else a small value of it — the cheapest callback that keeps the types.
    fn lambda(&self, ps: &[SigType], r: &SigType) -> String {
        let names: Vec<String> = (0..ps.len()).map(|k| format!("p{k}")).collect();
        let want = r.instantiate(&self.subst);
        let body = ps
            .iter()
            .zip(&names)
            .find(|(p, _)| p.instantiate(&self.subst) == want)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| match want {
                // The worst case scans everything: a search that never hits,
                // a filter that is evaluated on every element.
                Some(GenType::Bool) if STOPS_ON_TRUE.contains(&self.func()) => "false".into(),
                Some(ref g) => small(g),
                None => "()".into(),
            });
        format!("({}) => {body}", names.join(", "))
    }

    /// Criterion 2, the plain call: `k` calls on one size-`n` input. With
    /// `with_call` false the same loop runs without the call, so the
    /// difference of the two timings is the calls' cost alone.
    pub fn direct_program(&self, n: u64, k: u64, with_call: bool) -> String {
        let mut s = String::from("effect fn main() -> Unit = {\n");
        // The input changes every iteration by an O(1) in-place step, run in
        // BOTH programs, so the call is never loop-invariant (a never-true
        // guard like `i < 0` was provable and hoisted through) and the step's
        // own cost cancels out of the difference. Alternating two inputs was
        // worse: native cloned the chosen list on every iteration (#3532).
        // A string has no O(1) in-place step, and `x = x` is not free on
        // every leg: it is bound once and left alone.
        let slot = self.size_param.map(|i| {
            let varies = !matches!(self.ty(i), Some(GenType::String)) && !self.ty(i).is_some_and(|t| t.refuses_var()) && !self.is_routed(i);
            let kw = if varies || Some(i) == self.mut_param { "var" } else { "let" };
            let _ = writeln!(s, "  {kw} x: {} = {}", self.render_param(i), self.arg(i, n));
            (i, varies)
        });
        // Every other value argument is bound to a typed `let` first: a bare
        // `ok(2)` leaves its error type unconstrained (E025).
        let per_step: Vec<usize> = (0..self.params.len()).filter(|i| self.per_step(*i)).collect();
        s.push_str(&self.arg_lets(n, self.size_param, &per_step));
        let _ = writeln!(s, "  var acc = 0\n  var i = 0\n  while i < {k} {{");
        // A per-step operand is rebuilt every iteration in BOTH programs (its
        // cost cancels): one fixed key measures one probe path, and a hash
        // index's single path is that key's neighbourhood at this n, not the
        // order (map.get's String series moved ±15% between sizes).
        for &p in &per_step {
            let _ = writeln!(s, "    let a{p}: {} = {}", self.render_param(p), self.recipe_expr(p, n, "i"));
        }
        if let Some((i, true)) = slot {
            let _ = writeln!(s, "    x = {}", vary("x", &self.ty(i).unwrap_or(GenType::Int), n));
        }
        if with_call {
            let _ = writeln!(s, "    let r: {} = {}", self.ret.render(), self.bound_call(slot.map(|(i, _)| i)));
            let _ = writeln!(s, "    acc = acc + {}", digest("r", &self.ret));
            s.push_str(&outcome_guard("r", &self.ret, self.in_domain(), "    "));
        }
        let _ = writeln!(s, "    acc = acc + 1\n    i = i + 1\n  }}");
        if let Some(i) = self.size_param.filter(|i| !self.leg_dependent_input(*i)) {
            let _ = writeln!(s, "  acc = acc + {}", digest("x", &self.ty(i).unwrap_or(GenType::Int)));
        }
        let s = match self.routed {
            Some(r) => self.in_handler(r, &format!("{}  http.response(200, \"${{acc}}\")\n", &s["effect fn main() -> Unit = {\n".len()..]), n, true),
            None => s + "  println(\"${acc}\")\n}\n",
        };
        format!("{}{s}", super::catalogue::imports_for(&s))
    }

    /// Criterion 2, the use-shapes: `k` steps that rebind the return-typed
    /// parameter, starting from size `n` — the var's own rebind in a loop,
    /// a self tail call and a fold callback. Each must cost what one step
    /// of the in-place form costs, not a copy of the value.
    pub fn rebind_programs(&self, n: u64, k: u64) -> Vec<(&'static str, String)> {
        let Some(p) = self.rebind_param else { return Vec::new() };
        let ty = self.ret.render();
        let init = if Some(p) == self.size_param || self.recipe_arg(p).is_some() || self.domain_arg(p).is_some() { self.arg(p, n) } else { sized(&self.ret, n) };
        // The other value arguments come from typed top-level fns, which the
        // tail shape's `go` can see as well as `main` (an inline `ok(2)`
        // leaves its error type unconstrained, E025). They are small, except
        // a recipe's operand that must keep its shape against the others.
        let mut consts = String::new();
        for i in (0..self.params.len()).filter(|i| *i != p && !self.is_fn(*i)) {
            // A size operand of another type than `x` does not shape it: a
            // map `get_or` looks its default up in is built at size 1, not
            // rebuilt at n by every step (a matrix operand of `x`'s type
            // must still agree with it).
            let foreign_size = Some(i) == self.size_param && self.ty(i).as_ref() != Some(&self.ret);
            let shaped = self.recipe_arg(i).is_some() && self.scales(i) && !foreign_size;
            let m = if shaped { n } else { 1 };
            let _ = writeln!(consts, "fn a{i}() -> {} = {}\n", self.render_param(i), self.arg(i, m));
        }
        let step = |v: &str| {
            let args: Vec<String> = (0..self.params.len())
                .map(|i| match i {
                    _ if i == p => v.to_string(),
                    _ if self.is_fn(i) => self.arg(i, 1),
                    _ => format!("a{i}()"),
                })
                .collect();
            format!("{}({})", self.name, args.join(", "))
        };
        let d = digest("x", &self.ret);
        // The value each shape ends with must be ok (and, from an in-domain
        // input, not `none` / `false`). Checked once, after the steps: a
        // check inside them would change what they time (the in-place
        // rebind). Every shape starts from an `ok` (`small` and the recipes
        // build no `err`), so a step that took the error path leaves an `err`
        // unless a later step turns it back into an `ok`.
        let end = format!("{}  println(\"${{{d}}}\")\n", outcome_guard("x", &self.ret, self.in_domain(), "  "));
        // A handler-holding value has no `var` form (E008), so no loop.
        let loop_ok = !self.ret.refuses_var();
        let programs = vec![
            (
                "loop",
                format!(
                    "effect fn main() -> Unit = {{\n  var x: {ty} = {init}\n  var i = 0\n  while i < {k} {{\n    x = {}\n    i = i + 1\n  }}\n{end}}}\n",
                    step("x")
                ),
            ),
            (
                "tail",
                format!(
                    "fn go(x: {ty}, i: Int) -> {ty} = if i <= 0 then x else go({}, i - 1)\n\neffect fn main() -> Unit = {{\n  let x0: {ty} = {init}\n  let x: {ty} = go(x0, {k})\n{end}}}\n",
                    step("x")
                ),
            ),
            (
                "fold",
                format!(
                    "effect fn main() -> Unit = {{\n  let x0: {ty} = {init}\n  let x: {ty} = list.range(0, {k}) |> list.fold(x0, (x, i) => {})\n{end}}}\n",
                    step("x")
                ),
            ),
        ];
        programs
            .into_iter()
            .filter(|(label, _)| loop_ok || *label != "loop")
            .map(|(label, src)| {
                let body = format!("{consts}{src}");
                (label, format!("{}{body}", super::catalogue::imports_for(&body)))
            })
            .collect()
    }

    /// Criterion 5: `base` wraps the call, `left` and `right` each import
    /// `base` and wrap it again, and `main` imports both and also calls the
    /// function directly — three lines that must print the same value.
    pub fn diamond_project(&self, n: u64) -> Project {
        let sig: Vec<String> = (0..self.params.len())
            .map(|i| format!("{}a{i}: {}", if Some(i) == self.mut_param { "mut " } else { "" }, self.render_param(i)))
            .collect();
        let names: Vec<String> = (0..self.params.len()).map(|i| format!("a{i}")).collect();
        let ret = self.ret.render();
        let kw = if self.effect { "effect fn" } else { "fn" };
        let wrap = |inner: &str| format!("pub {kw} call({}) -> {ret} = {inner}({})\n", sig.join(", "), names.join(", "));
        let mut main = String::from("import self.left\nimport self.right\n\neffect fn main() -> Unit = {\n");
        for i in (0..self.params.len()).filter(|i| Some(*i) != self.mut_param) {
            let _ = writeln!(main, "  let a{i}: {} = {}", self.render_param(i), self.arg(i, n));
        }
        // Each path mutates its own var, so the three must agree on the
        // mutated value as well as on the return.
        let path_args = |path: &str| -> String {
            (0..self.params.len())
                .map(|i| if Some(i) == self.mut_param { format!("{path}m") } else { format!("a{i}") })
                .collect::<Vec<_>>()
                .join(", ")
        };
        for (v, callee) in [("d", self.name.as_str()), ("l", "left.call"), ("r", "right.call")] {
            if let Some(m) = self.mut_param {
                let _ = writeln!(main, "  var {v}m: {} = {}", self.render_param(m), self.arg(m, n));
            }
            let _ = writeln!(main, "  let {v}: {ret} = {callee}({})", path_args(v));
        }
        // Three paths agreeing on an `err` agree on the error path.
        for v in ["d", "l", "r"] {
            main.push_str(&outcome_guard(v, &self.ret, self.in_domain(), "  "));
        }
        for v in ["d", "l", "r"] {
            match self.observe {
                // Encoded bytes differ by leg by contract: compare what decodes.
                Some(dec) => {
                    let _ = writeln!(main, "  let {v}_o: {ret} = match {v} {{\n    ok(c) => {dec}(c),\n    err(e) => err(e),\n  }}");
                    let _ = writeln!(main, "  {}", render_print(&format!("{v}_o"), &self.ret));
                }
                None if self.ret != GenType::Unit => {
                    let _ = writeln!(main, "  {}", render_print(v, &self.ret));
                }
                // A Unit return has no string form (E089); its one value is
                // printed literally, so each path shows it returned.
                None => {
                    let _ = writeln!(main, "  println(\"{v} = ()\")");
                }
            }
            if let Some(m) = self.mut_param {
                let _ = writeln!(main, "  {}", render_print(&format!("{v}m"), &self.ty(m).unwrap_or(GenType::Unit)));
            }
        }
        let main = match self.routed {
            Some(r) => {
                let (head, body) = main.split_at(main.find("effect fn main").unwrap_or(0));
                let body = &body["effect fn main() -> Unit = {\n".len()..];
                format!("{head}{}", self.in_handler(r, &format!("{body}  http.response(200, \"\")\n"), n, false))
            }
            None => main + "}\n",
        };
        let main = format!("{}{main}", super::catalogue::imports_for(&main));
        let base = wrap(&self.name);
        // A path names the types in its signature, so it imports their
        // modules as well as `base`.
        let path = wrap("base.call");
        let path = format!("{}import self.base\n\n{path}", super::catalogue::imports_for(&path));
        let files = vec![
            ("app/almide.toml".to_string(), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n".to_string()),
            ("app/src/base.almd".to_string(), format!("{}{base}", super::catalogue::imports_for(&base))),
            ("app/src/left.almd".to_string(), path.clone()),
            ("app/src/right.almd".to_string(), path),
            ("app/src/main.almd".to_string(), main),
        ];
        Project { files, entry: "app/src/main.almd".to_string() }
    }

    /// `let a<i>: T = <arg>` for every value parameter other than `skip`
    /// (callbacks stay inline: they are not values a `let` can type here).
    fn arg_lets(&self, n: u64, skip: Option<usize>, later: &[usize]) -> String {
        let mut s = String::new();
        for i in (0..self.params.len()).filter(|i| Some(*i) != skip && !self.is_fn(*i) && !later.contains(i)) {
            let kw = if Some(i) == self.mut_param { "var" } else { "let" };
            let _ = writeln!(s, "  {kw} a{i}: {} = {}", self.render_param(i), self.arg(i, n));
        }
        s
    }

    /// The call over the `let`-bound arguments of [`Self::arg_lets`], `x` in
    /// the `slot` position and callbacks inline.
    fn bound_call(&self, slot: Option<usize>) -> String {
        let args: Vec<String> = (0..self.params.len())
            .map(|i| match i {
                _ if Some(i) == slot => "x".to_string(),
                _ if self.is_fn(i) => self.arg(i, 0),
                _ => format!("a{i}"),
            })
            .collect();
        format!("{}({})", self.name, args.join(", "))
    }

    fn is_fn(&self, i: usize) -> bool {
        matches!(self.params[i].1, SigType::Fn(..))
    }

    fn func(&self) -> &str {
        self.name.rsplit('.').next().unwrap_or("")
    }

    /// The return value can be printed: an observable type, or a top-level
    /// `Bytes` (printed as its byte list, see [`render_print`]).
    pub fn ret_is_observable(&self) -> bool {
        self.ret.observe("v", 0).is_some()
    }

    fn render_param(&self, i: usize) -> String {
        render_sig(&self.params[i].1, &self.subst)
    }
}

fn render_sig(t: &SigType, subst: &HashMap<String, GenType>) -> String {
    match t {
        SigType::Fn(ps, r) => format!(
            "({}) -> {}",
            ps.iter().map(|p| render_sig(p, subst)).collect::<Vec<_>>().join(", "),
            render_sig(r, subst)
        ),
        other => other.instantiate(subst).map(|g| g.render()).unwrap_or_else(|| "Unit".into()),
    }
}

fn small_int(param: &str) -> &'static str {
    match param {
        "i" | "j" | "index" | "start" | "lo" | "row" | "col" => "0",
        _ => "2",
    }
}

/// A small literal of `t`.
fn small(t: &GenType) -> String {
    match t {
        GenType::Int => "2".into(),
        GenType::Float => "1.5".into(),
        GenType::String => "\"aé\"".into(),
        GenType::Bool => "true".into(),
        GenType::Unit => "()".into(),
        GenType::List(e) => format!("[{}, {}]", small(e), small(e)),
        GenType::Option(e) => format!("some({})", small(e)),
        GenType::Result(e) | GenType::ResultNom(e, _) => format!("ok({})", small(e)),
        GenType::Tuple2(a, b) => format!("({}, {})", small(a), small(b)),
        GenType::Map(v) => format!("[\"k\": {}]", small(v)),
        GenType::Sized(name) => GenType::sized_literal(name, "2"),
        GenType::Bytes => "bytes.from_list([1, 2])".into(),
        GenType::Matrix => "matrix.from_lists([[1.5, 2.0], [0.5, 3.0]])".into(),
        GenType::Value => "value.object([(\"a\", value.int(1)), (\"snake_case\", value.str(\"x\"))])".into(),
        GenType::Endian => "BigEndian".into(),
        GenType::Set(e) => format!("set.from_list([{}])", small(e)),
        GenType::Nominal(k) => nominal::get(k).map_or_else(|| "()".into(), |d| d.small.to_string()),
    }
}

/// The element type of a container (a Map's value).
fn elem_of(t: &GenType) -> Option<GenType> {
    match t {
        GenType::List(e) | GenType::Set(e) | GenType::Map(e) | GenType::Option(e) | GenType::Result(e) => Some((**e).clone()),
        _ => None,
    }
}

/// A value of `t` no sized input holds (`elem` makes Ints >= 0, strings
/// `s<i>`): the miss that makes a search scan everything.
fn absent(t: &GenType) -> String {
    match t {
        GenType::Int => "-1".into(),
        GenType::String => "\"absent\"".into(),
        other => small(other),
    }
}

/// An element of `t` that varies with the loop index `i`.
fn elem(t: &GenType) -> String {
    match t {
        GenType::Int => "i".into(),
        GenType::Float => "int.to_float(i) * 0.5".into(),
        GenType::String => "\"s${100000000 + i}\"".into(),
        GenType::Bool => "i % 2 == 0".into(),
        GenType::List(e) => format!("[{}]", elem(e)),
        GenType::Option(e) => format!("some({})", elem(e)),
        GenType::Tuple2(a, b) => format!("({}, {})", elem(a), elem(b)),
        GenType::Sized(name) => GenType::sized_literal(name, "i % 100"),
        GenType::Set(e) => format!("set.from_list([{}])", elem(e)),
        other => small(other),
    }
}

/// A value of `t` whose size is `n`.
fn sized(t: &GenType, n: u64) -> String {
    match t {
        GenType::Int => n.to_string(),
        GenType::String => format!("string.repeat(\"aé\", {n})"),
        GenType::List(e) => format!("list.range(0, {n}) |> list.map((i) => {})", elem(e)),
        GenType::Map(v) => format!("list.range(0, {n}) |> list.fold(map.new(), (m, i) => map.set(m, \"k${{100000000 + i}}\", {}))", elem(v)),
        GenType::Bytes => format!("bytes.new({n})"),
        GenType::Matrix => format!("matrix.zeros({n}, 2)"),
        // An object of n keys: what `field` / `keys` / `pick` / `omit` /
        // `merge` and the case converters walk. Keys and values are fixed
        // width, so its stringified length is a·n + b (#3636).
        GenType::Value => format!("value.object(list.range(0, {n}) |> list.map((i) => (\"k${{100000000 + i}}\", value.int(100000000 + i))))"),
        GenType::Set(e) => format!("list.range(0, {n}) |> list.map((i) => {}) |> set.from_list", elem(e)),
        GenType::Nominal(k) => nominal::get(k).map_or_else(|| "()".into(), |d| nominal::fill(d.sized, "", n)),
        other => small(other),
    }
}

/// An O(1) in-place change of the size-`n` value `v` that depends on the
/// loop index `i` and keeps its size.
fn vary(v: &str, t: &GenType, n: u64) -> String {
    match t {
        GenType::Int => format!("{n} + i % 2"),
        GenType::List(e) => format!("list.set({v}, 0, {})", elem(e)),
        GenType::Map(e) => format!("map.set({v}, \"k0\", {})", elem(e)),
        // A string has no O(1) in-place edit; the same value is rebound.
        _ => v.to_string(),
    }
}

/// The statement that stops the program when `v` (of type `t`) is a
/// refusal: an `err` always ([`ERR_PATH`]); with the input `in_domain`, a
/// `none` or a `false` as well ([`OFF_DOMAIN`]). Without an in-domain
/// input a `none` / `false` is a legitimate answer — a search's miss is its
/// worst case ([`SEARCH_PARAMS`]) — so nothing is checked. The error slot is
/// pinned to String (see the module doc), so `e` interpolates; a nominal
/// error is shown through the nominal's own readers.
fn outcome_guard(v: &str, t: &GenType, in_domain: bool, indent: &str) -> String {
    match t {
        GenType::Result(_) => {
            format!("{indent}match {v} {{\n{indent}  ok(_) => (),\n{indent}  err(e) => panic(\"{ERR_PATH}: ${{e}}\"),\n{indent}}}\n")
        }
        GenType::ResultNom(_, k) => {
            let (seen, shown) = match nominal::get(k).filter(|d| !d.observe_calls_effect) {
                Some(d) => ((d.observed)().render(), nominal::fill(d.observe, "e", 0)),
                None => ("String".to_string(), "\"\"".to_string()),
            };
            format!(
                "{indent}match {v} {{\n{indent}  ok(_) => (),\n{indent}  err(e) => {{\n{indent}    let eo: {seen} = {shown}\n{indent}    panic(\"{ERR_PATH}: ${{eo}}\")\n{indent}  }},\n{indent}}}\n"
            )
        }
        GenType::Option(_) if in_domain => {
            format!("{indent}match {v} {{\n{indent}  some(_) => (),\n{indent}  none => panic(\"{OFF_DOMAIN}: none\"),\n{indent}}}\n")
        }
        GenType::Bool if in_domain => format!("{indent}if {v} then () else panic(\"{OFF_DOMAIN}: false\")\n"),
        _ => String::new(),
    }
}

/// An Int that depends on the whole of `v`, so no call is dead.
fn digest(v: &str, t: &GenType) -> String {
    match t {
        GenType::Int => v.into(),
        GenType::Float => format!("(if {v} > 0.0 then 1 else 0)"),
        GenType::String => format!("string.len({v})"),
        GenType::Bool => format!("(if {v} then 1 else 0)"),
        GenType::Unit => "0".into(),
        GenType::List(_) => format!("list.len({v})"),
        GenType::Map(_) => format!("map.len({v})"),
        GenType::Option(_) => format!("(if option.is_some({v}) then 1 else 0)"),
        GenType::Result(_) | GenType::ResultNom(..) => format!("(if result.is_ok({v}) then 1 else 0)"),
        GenType::Tuple2(..) => "1".into(),
        GenType::Sized(name) if name.starts_with("Float") => {
            format!("(if float.from_{}({v}) > 0.0 then 1 else 0)", name.to_lowercase())
        }
        GenType::Sized(name) => format!("int.from_{}({v})", name.to_lowercase()),
        GenType::Bytes => format!("bytes.len({v})"),
        GenType::Matrix => format!("matrix.rows({v})"),
        GenType::Value => format!("string.len(value.stringify({v}))"),
        GenType::Endian => "1".into(),
        GenType::Set(_) => format!("set.len({v})"),
        GenType::Nominal(k) => nominal::get(k).map_or_else(|| "1".into(), |d| nominal::fill(d.digest, v, 0)),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The catalogued function `name`, instantiated (for tests elsewhere too).
    pub(crate) fn target(name: &str) -> Target {
        let catalogue = crate::generator::catalogue::build();
        let sig = catalogue.iter().find(|s| format!("{}.{}", s.module, s.func) == name).unwrap_or_else(|| panic!("{name} not catalogued"));
        Target::new(sig, &catalogue).expect("target")
    }

    /// Every catalogued function the probes give an in-domain input.
    pub(crate) fn in_domain_targets() -> Vec<Target> {
        let catalogue = crate::generator::catalogue::build();
        catalogue.iter().filter_map(|s| Target::new(s, &catalogue).ok()).filter(Target::in_domain).collect()
    }

    /// #3636: the sized Value's stringified length must be a·n + b. With
    /// `value.int(i)` the digit width grew across the sweep (crossing 99999
    /// between 20k and 320k), so json.stringify / value.stringify /
    /// value.omit fit n at 2.9–3.8% rms against a declared n.
    #[test]
    fn the_sized_value_holds_fixed_width_ints() {
        let s = sized(&GenType::Value, 1000);
        assert!(s.contains("value.int(100000000 + i)"), "{s}");
        assert!(!s.contains("value.int(i)"), "{s}");
    }

    /// #3637: matrix functions with head or kernel geometry got
    /// `matrix.zeros(n, 2)` with every other operand 2×2 and 2 for every
    /// count, so heads × dim = 4 overran the 2 columns and the q/k/v and
    /// conv1d shapes did not line up — every shape exited 1. Each gets
    /// operands that agree, with n on an axis the function accepts, in the
    /// plain call, the rebind shapes and the diamond alike.
    #[test]
    fn head_and_kernel_geometry_functions_get_operands_that_agree() {
        for name in ["matrix.rope_rotate", "matrix.rope_rotate_at", "matrix.rope_rotate_neox_at"] {
            let t = target(name);
            let direct = t.direct_program(1000, 4, true);
            // 2 heads × head_dim 2 = the 4 columns.
            assert!(direct.contains("  var x: Matrix = matrix.zeros(1000, 4)"), "{direct}");
            assert!(direct.contains("  let a1: Int = 2\n  let a2: Int = 2\n"), "{direct}");
            let tail = &t.rebind_programs(1000, 4)[1].1;
            assert!(tail.contains("let x0: Matrix = matrix.zeros(1000, 4)") && tail.contains("fn a1() -> Int = 2"), "{tail}");
        }
        // Causal attention needs rows(q) <= rows(k): n is the key/value
        // length (a KV cache), q a fixed 2 rows, all 4 wide over 2 heads.
        let t = target("matrix.masked_multi_head_attention");
        let direct = t.direct_program(1000, 4, true);
        assert!(direct.contains("  var x: Matrix = matrix.zeros(1000, 4)"), "{direct}");
        assert!(direct.contains("  let a0: Matrix = matrix.zeros(2, 4)"), "{direct}");
        assert!(direct.contains("  let a2: Matrix = matrix.zeros(1000, 4)"), "{direct}");
        assert!(direct.contains("  let a3: Int = 2"), "{direct}");
        assert!(direct.contains("matrix.masked_multi_head_attention(a0, x, a2, a3)"), "{direct}");
        let fold = &t.rebind_programs(1000, 4)[2].1;
        assert!(fold.contains("let x0: Matrix = matrix.zeros(2, 4)"), "{fold}");
        assert!(fold.contains("fn a1() -> Matrix = matrix.zeros(1000, 4)") && fold.contains("fn a2() -> Matrix = matrix.zeros(1000, 4)"), "{fold}");
        // conv1d: (T, in_ch) input, (out_ch, in_ch × kernel) weight, one
        // bias per out_ch; kernel 3, stride 1, padding 1 keep T rows out.
        let t = target("matrix.conv1d");
        let direct = t.direct_program(1000, 4, true);
        for want in [
            "  var x: Matrix = matrix.zeros(1000, 2)",
            "  let a1: Matrix = matrix.zeros(2, 6)",
            "  let a2: List[Float] = [0.5, 1.5]",
            "  let a3: Int = 3",
            "  let a4: Int = 1",
            "  let a5: Int = 1",
        ] {
            assert!(direct.contains(want), "{want}: {direct}");
        }
        let project = t.diamond_project(64);
        let main = &project.files.iter().find(|(p, _)| p.ends_with("main.almd")).unwrap().1;
        assert!(main.contains("let a0: Matrix = matrix.zeros(64, 2)") && main.contains("let a1: Matrix = matrix.zeros(2, 6)"), "{main}");
    }

    /// #3666: the decoders' sized String was `string.repeat("aé", n)`, which
    /// none of them accepts, so every shape timed the early-error exit (flat,
    /// ×1.0 per doubling). Each gets valid input of size n — encoded bytes,
    /// 2n hex digits, a JSON array of n fixed-width ints — in the plain call
    /// and the diamond alike.
    #[test]
    fn decoders_are_sized_with_input_they_accept() {
        for (name, want) in [
            ("base64.decode", "base64.encode(bytes.from_list(list.range(0, 1000) |> list.map((i) => i % 256)))"),
            ("base64.decode_url", "base64.encode_url(bytes.from_list(list.range(0, 1000) |> list.map((i) => i % 256)))"),
            ("hex.decode", "hex.encode(bytes.from_list(list.range(0, 1000) |> list.map((i) => i % 256)))"),
            ("json.parse", "\"[\" + (list.range(0, 1000) |> list.map((i) => int.to_string(100000000 + i)) |> list.join(\",\")) + \"]\""),
        ] {
            let t = target(name);
            let direct = t.direct_program(1000, 4, true);
            assert!(direct.contains(&format!("  let x: String = {want}\n")), "{name}: {direct}");
            assert!(!direct.contains("string.repeat"), "{name}: {direct}");
            let project = t.diamond_project(1000);
            let main = &project.files.iter().find(|(p, _)| p.ends_with("main.almd")).unwrap().1;
            assert!(main.contains(&format!("  let a0: String = {want}\n")), "{name}: {main}");
        }
    }

    /// #3666: a call that returns `err` took the error path, and its cost is
    /// not the function's. Every program of a Result-returning function stops
    /// with [`ERR_PATH`] on an `err` — the plain call on every result, each
    /// rebind shape on its final value, the diamond on all three paths — so a
    /// run that measured the error path fails instead of being timed. The
    /// check comes from the return type, not from a list of names.
    #[test]
    fn a_result_returning_call_stops_on_err_in_every_program() {
        let stop = format!("err(e) => panic(\"{ERR_PATH}: ${{e}}\")");
        let guard = |v: &str| (format!("match {v} {{"), stop.clone());
        let has = |src: &str, v: &str| {
            let (head, arm) = guard(v);
            src.lines().map(str::trim).collect::<Vec<_>>().windows(3).any(|w| w[0] == head && w[1] == "ok(_) => ()," && w[2] == format!("{arm},"))
        };
        let t = target("base64.decode");
        let direct = t.direct_program(1000, 4, true);
        assert!(has(&direct, "r"), "{direct}");
        // The baseline (no call) has no result to check.
        assert!(!t.direct_program(1000, 4, false).contains(ERR_PATH));
        let project = t.diamond_project(64);
        let main = &project.files.iter().find(|(p, _)| p.ends_with("main.almd")).unwrap().1;
        for v in ["d", "l", "r"] {
            assert!(has(main, v), "{v}: {main}");
        }
        // A rebind shape checks the value it ends with.
        let t = target("result.map");
        for (shape, src) in t.rebind_programs(1000, 4) {
            assert!(has(&src, "x"), "{shape}: {src}");
        }
        // A function whose return is not a Result has nothing to check.
        let t = target("list.map");
        assert!(!t.direct_program(1000, 4, true).contains(ERR_PATH));
        assert!(t.rebind_programs(1000, 4).iter().all(|(_, s)| !s.contains(ERR_PATH)));
    }

    /// #3671: `float.to_*_checked` got the constant 1.5, which every one of
    /// them refuses as non-integral, and `int.to_int8_checked` /
    /// `to_uint8_checked` got the size-like `n` (20000..320000), past their
    /// range — every stamped run measured `none`. A narrowing conversion is
    /// recognised by its type (an Int or Float to `Option[<fixed width>]`),
    /// and gets 2, which every fixed-width type holds, as a constant: its
    /// cost has no size.
    #[test]
    fn a_narrowing_conversion_gets_a_value_every_fixed_width_type_holds() {
        for (name, ty, want) in [
            ("int.to_int8_checked", "Int", "2"),
            ("int.to_uint8_checked", "Int", "2"),
            ("int.to_int16_checked", "Int", "2"),
            ("int.to_uint64_checked", "Int", "2"),
            ("float.to_int8_checked", "Float", "2.0"),
            ("float.to_uint8_checked", "Float", "2.0"),
            ("float.to_int64_checked", "Float", "2.0"),
            ("float.to_uint64_checked", "Float", "2.0"),
        ] {
            let t = target(name);
            assert!(t.in_domain() && t.size_param.is_none(), "{name}");
            let direct = t.direct_program(1000, 4, true);
            assert!(direct.contains(&format!("  let a0: {ty} = {want}\n")), "{name}: {direct}");
            assert!(!direct.contains("1000"), "{name}: the value is not a size: {direct}");
        }
    }

    /// #3671: a decoder is given what its module's own producer of its input
    /// makes — the inverse it declares by type — instead of the generic
    /// value it refuses: `int.parse` / `int.from_hex` / `float.parse` got
    /// "aéaé…" and returned `err`; `value.as_*` got an object. A numeral
    /// scales by leading zeros; a payload with a size is sized.
    #[test]
    fn a_decoder_gets_what_its_module_encodes() {
        for (name, want) in [
            ("int.parse", "string.repeat(\"0\", 1000) + int.to_string(2)"),
            ("int.from_hex", "string.repeat(\"0\", 1000) + int.to_hex(2)"),
            ("float.parse", "string.repeat(\"0\", 1000) + float.to_string(1.5)"),
            ("value.as_string", "value.str(string.repeat(\"aé\", 1000))"),
            ("value.as_array", "value.array(list.range(0, 1000) |> list.map((i) => "),
            ("url.decode_component", "url.encode_component(string.repeat(\"aé\", 1000))"),
        ] {
            let t = target(name);
            assert!(t.in_domain() && t.size_param == Some(0), "{name}");
            let direct = t.direct_program(1000, 4, true);
            assert!(direct.contains(&format!(" x: {} = {want}", t.render_param(0))), "{name}: {direct}");
            let project = t.diamond_project(64);
            let main = &project.files.iter().find(|(p, _)| p.ends_with("main.almd")).unwrap().1;
            assert!(main.contains(&format!("  let a0: {} = {}", t.render_param(0), want.replace("1000", "64"))), "{name}: {main}");
        }
        // A payload with no size: a constant input, not a size.
        for (name, want) in [("value.as_float", "value.float(1.5)"), ("value.as_int", "value.int(2)"), ("value.as_bool", "value.bool(true)")] {
            let t = target(name);
            assert!(t.in_domain() && t.size_param.is_none(), "{name}");
            assert!(t.direct_program(1000, 4, true).contains(&format!("  let a0: Value = {want}\n")), "{name}");
        }
        // `string.from_codepoint` makes a String from an Int, but a
        // character: it is not padded like a numeral.
        let t = target("string.codepoint");
        assert!(t.direct_program(1000, 4, true).contains("  let a0: String = string.from_codepoint(2)\n"));
    }

    /// #3671: a function whose input is in-domain (a recipe or a derived
    /// domain) stops every probe on a `none` or `false`, as a Result-typed
    /// one stops on `err`: a run that measured the refusal fails instead of
    /// being counted. Without an in-domain input a `none` / `false` stays a
    /// result — a search's miss is its worst case.
    #[test]
    fn an_in_domain_input_stops_every_probe_on_none_or_false() {
        let none = format!("none => panic(\"{OFF_DOMAIN}: none\")");
        let t = target("int.to_int8_checked");
        assert!(t.direct_program(1000, 4, true).contains(&none));
        let project = t.diamond_project(64);
        let main = &project.files.iter().find(|(p, _)| p.ends_with("main.almd")).unwrap().1;
        assert_eq!(main.matches(&none).count(), 3, "{main}");
        let falsy = format!("if r then () else panic(\"{OFF_DOMAIN}: false\")");
        assert!(target("string.is_digit").direct_program(1000, 4, true).contains(&falsy));
        // A search over generic values: its miss is the measurement.
        for name in ["list.find", "list.contains", "string.is_empty"] {
            let t = target(name);
            assert!(!t.in_domain(), "{name}");
            assert!(!t.direct_program(1000, 4, true).contains(OFF_DOMAIN), "{name}");
        }
    }

    /// The functions whose stamps were voided because the probe measured a
    /// refusal (#3671, and value.as_float / as_string after it), and the
    /// not-stampable ones whose probe refused or truncated its input, all
    /// get an in-domain input now — by recipe or by derivation. A change
    /// that drops one back to the generic values fails here.
    #[test]
    fn every_function_found_measuring_a_refusal_gets_an_in_domain_input() {
        let covered: std::collections::BTreeSet<String> = in_domain_targets().into_iter().map(|t| t.name).collect();
        for name in [
            // voided by #3671
            "int.parse",
            "float.parse",
            "int.from_hex",
            "string.byte_slice",
            "int.to_int8_checked",
            "int.to_uint8_checked",
            "float.to_int8_checked",
            "float.to_int16_checked",
            "float.to_int32_checked",
            "float.to_int64_checked",
            "float.to_uint8_checked",
            "float.to_uint16_checked",
            "float.to_uint32_checked",
            "float.to_uint64_checked",
            "string.is_digit",
            "string.is_whitespace",
            "string.is_upper",
            // voided after it
            "value.as_float",
            "value.as_string",
            // not stampable, the probe refused or truncated its input
            "value.as_array",
            "value.as_bool",
            "value.as_int",
            "value.field",
            "json.get_int",
            "map.delete",
            "matrix.sub",
            "matrix.fma",
            "matrix.fma3",
            "bytes.copy_within",
            "bytes.copy_from",
            "regex.full_match",
            // outside the model until a Result could carry a nominal error,
            // and until a probe could run inside a routed handler
            "http.decode_json",
            "http.param",
        ] {
            assert!(covered.contains(name), "{name} has no in-domain input");
        }
    }

    /// Every in-domain probe takes the success path when it runs: the plain
    /// call of every function [`in_domain_targets`] holds, at n = 64, built
    /// and run natively by the workspace `almide`, exits 0 — the in-domain
    /// guards turn an `err`, `none` or `false` into a failed exit, so a
    /// recipe or a derivation that drifted to the refusal fails here. A
    /// no-op without a built `almide` (as `composition_programs_pass_almide_check`);
    /// fuzz-nightly builds one and runs it.
    #[test]
    fn every_in_domain_probe_takes_the_success_path() {
        let bin = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/release/almide");
        if !bin.exists() {
            eprintln!("skipping: no workspace almide binary");
            return;
        }
        let dir = std::env::temp_dir().join(format!("xtarget-in-domain-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut failed = Vec::new();
        let targets = in_domain_targets();
        assert!(!targets.is_empty());
        for t in &targets {
            let file = dir.join(format!("{}.almd", t.name.replace('.', "_")));
            std::fs::write(&file, t.direct_program(64, 1, true)).unwrap();
            // In the fixed world the harness runs it in (ADR-0027 D2).
            let world = crate::oracle::sandbox::World::new(&dir).expect("sandbox");
            let mut cmd = std::process::Command::new(&bin);
            cmd.arg("run").arg(&file).arg("--").args(crate::oracle::sandbox::ARGS);
            world.enter(&mut cmd);
            let out = cmd.output().expect("spawn almide");
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let why = stderr.lines().find(|l| l.contains("inspect:") || l.contains("error")).unwrap_or("").to_string();
                failed.push(format!("{}: exit {:?} {why}", t.name, out.status.code()));
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert!(failed.is_empty(), "in-domain probes that did not take the success path:\n{}", failed.join("\n"));
    }

    /// `http.decode_json` is sized by a JSON body its decode callback accepts
    /// (n ints, decoded to their count), so the call takes the ok path; its
    /// `Result[T, HttpResponse]` stops every probe on `err` with the
    /// response shown through HttpResponse's readers. At String the callback
    /// returns the count as a String.
    #[test]
    fn decode_json_is_sized_with_a_body_its_callback_accepts() {
        let t = target("http.decode_json");
        assert!(t.in_domain() && t.size_param == Some(0) && t.rebind_param.is_none());
        let direct = t.direct_program(1000, 4, true);
        assert!(direct.contains("  var x: HttpRequest = http.new_request(\"POST\", \"/j\", \"[\" + (list.range(0, 1000) |> list.map((i) => int.to_string(100000000 + i))"), "{direct}");
        assert!(direct.contains("let r: Result[Int, HttpResponse] = http.decode_json(x, (v) => match value.as_array(v) { ok(xs) => ok((list.len(xs))), err(e) => err(e) })"), "{direct}");
        assert!(direct.contains("err(e) => {\n        let eo: (Int, (String, Map[String, String])) = (http.status_code(e), (http.body(e), http.headers(e)))\n        panic(\"inspect: the probed call returned err: ${eo}\")"), "{direct}");
        let catalogue = crate::generator::catalogue::build();
        let sig = catalogue.iter().find(|s| s.module == "http" && s.func == "decode_json").unwrap();
        let heap = Target::new_filled(sig, &catalogue, GenType::String).expect("heap target");
        assert!(heap.direct_program(1000, 4, true).contains("ok(int.to_string(list.len(xs)))"));
        let project = t.diamond_project(64);
        let main = &project.files.iter().find(|(p, _)| p.ends_with("main.almd")).unwrap().1;
        assert!(main.contains("let d_o: Result[Int, String] = match d { ok(o0) => ok(o0), err(e0) => {"), "{main}");
    }

    /// `http.param` answers `some` only for a request the router bound path
    /// parameters into: every probe runs inside the handler of a one-route
    /// router, the routed request filling `req`, and the size is the bound
    /// value's. The plain call and its baseline both route (the routing
    /// cancels out of their difference); a router that answered anything but
    /// 200 stops the probe as off-domain.
    #[test]
    fn param_is_probed_inside_a_routed_handler() {
        let t = target("http.param");
        assert!(t.in_domain() && t.size_param == Some(0));
        for with_call in [true, false] {
            let direct = t.direct_program(1000, 4, with_call);
            assert!(direct.contains("  let h: HttpHandler = match http.router([http.route(\"GET /p/{id}\", (req) => {\n    let x: HttpRequest = req\n    let a1: String = \"id\"\n"), "{direct}");
            assert!(direct.contains("h(http.new_request(\"GET\", \"/p/\" + string.repeat(\"a%C3%A9\", 1000), \"\", map.new()))!"), "{direct}");
            assert!(direct.contains(&format!("if http.status_code(res) == 200 then () else panic(\"{OFF_DOMAIN}: the router answered")), "{direct}");
            assert!(direct.contains("    http.response(200, \"${acc}\")\n  })]) {"), "{direct}");
            assert_eq!(direct.contains("let r: Option[String] = http.param(x, a1)"), with_call, "{direct}");
        }
        let project = t.diamond_project(64);
        let main = &project.files.iter().find(|(p, _)| p.ends_with("main.almd")).unwrap().1;
        assert!(main.starts_with("import http\nimport self.left\nimport self.right\n\neffect fn main() -> Unit = {\n  let h: HttpHandler = "), "{main}");
        assert!(main.contains("    let a0: HttpRequest = req\n") && main.contains("    let l: Option[String] = left.call(a0, a1)\n"), "{main}");
        assert_eq!(main.matches(&format!("none => panic(\"{OFF_DOMAIN}: none\")")).count(), 3, "{main}");
        assert!(!main.contains("println(http.body(res))"), "the diamond prints only its three lines: {main}");
    }

    /// A recipe that drifted from its function's signature is an error, not
    /// a silent fall-back to the shapes that function refuses.
    #[test]
    fn every_shape_recipe_names_a_catalogued_function_and_its_parameters() {
        for r in SHAPE_RECIPES.iter().chain(super::effect_recipes::EFFECT_RECIPES) {
            let t = target(r.func);
            assert!(t.recipe.is_some() && (r.size.is_empty() || t.size_param.is_some()), "{}", r.func);
        }
    }

    /// A `mut`-parameter mutator: its sized input is a `var` the loop calls
    /// it on, and each diamond path mutates its own `var` and prints it.
    #[test]
    fn a_mutator_is_called_on_a_var_and_each_diamond_path_prints_its_own_copy() {
        let t = target("bytes.push");
        let direct = t.direct_program(1000, 4, true);
        assert!(direct.contains("  var x: Bytes = bytes.new(1000)"), "{direct}");
        assert!(direct.contains("let r: Unit = bytes.push(x, "), "{direct}");
        let project = t.diamond_project(1000);
        let file = |suffix: &str| project.files.iter().find(|(p, _)| p.ends_with(suffix)).map(|(_, s)| s.clone()).unwrap();
        assert!(file("base.almd").contains("pub fn call(mut a0: Bytes, a1: Int) -> Unit = bytes.push(a0, a1)"), "{}", file("base.almd"));
        let main = file("main.almd");
        for v in ["d", "l", "r"] {
            assert!(main.contains(&format!("  var {v}m: Bytes = bytes.new(1000)")), "{main}");
            assert!(main.contains(&format!("let {v}m_o: List[Int] = bytes.to_list({v}m)\n  println(\"{v}m = ${{{v}m_o}}\")")), "{main}");
        }
        assert!(main.contains("println(\"d = ()\")"), "a Unit return prints its one value, not its string form (E089): {main}");
        if let Ok(dir) = std::env::var("XF_DUMP") {
            std::fs::write(format!("{dir}/direct.almd"), &direct).unwrap();
            for (p, s) in &project.files {
                let path = std::path::Path::new(&dir).join(p);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, s).unwrap();
            }
        }
    }
}
