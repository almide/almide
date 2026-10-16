//! The stdlib's nominal (opaque) types the value grammar builds: each one
//! declared once here — how to make a small value, a size-`n` value and a few
//! random ones from its module's pure constructors, and how to observe it
//! through its module's pure readers (a nominal has no string form of its
//! own). `GenType::Nominal` carries the name; every consumer (the signature
//! parser, the term generator, the inspection programs, the printer) reads
//! this table, so a nominal added here reaches all of them at once.
//!
//! Templates: `{v}` is the value observed, `{n}` the size.

use super::types::GenType;

#[derive(Debug)]
pub struct Nominal {
    /// The type name as signatures spell it.
    pub name: &'static str,
    /// The module that declares it: a file that only names the type (a
    /// diamond path's `pub fn call(a0: HttpResponse)`) still imports it.
    pub module: &'static str,
    /// A small, fixed value.
    pub small: &'static str,
    /// A value of size `{n}`; its size is what the module's readers walk.
    pub sized: &'static str,
    /// Random leaves for the differential generator.
    pub leaves: &'static [&'static str],
    /// An observable expression of `{v}` built from the module's readers.
    pub observe: &'static str,
    /// The type of [`observe`](Self::observe).
    pub observed: fn() -> GenType,
    /// An Int that depends on `{v}` (the inspection's anti-dead-code sink).
    pub digest: &'static str,
    /// [`observe`](Self::observe) and [`digest`](Self::digest) call an
    /// effect closure (`h(req)!`): only an effect body may hold them.
    pub observe_calls_effect: bool,
    /// A value of this type holds a request handler, and a handler may not
    /// reach a `var` (E008: handlers run concurrently): it is never bound
    /// with `var`, so it has no loop-rebind shape.
    pub refuses_var: bool,
}

fn string() -> GenType {
    GenType::String
}

fn pair(a: GenType, b: GenType) -> GenType {
    GenType::Tuple2(Box::new(a), Box::new(b))
}

fn string_map() -> GenType {
    GenType::Map(Box::new(GenType::String))
}

/// Sized values use fixed-width keys (`k100000000`…), so a series is
/// linear in n.
pub const NOMINALS: &[Nominal] = &[
    Nominal {
        name: "HttpResponse",
        module: "http",
        small: "http.with_headers(200, \"aé\", [\"k\": \"v\"])",
        sized: "http.with_headers(200, \"aé\", (list.range(0, {n}) |> list.fold(map.new(), (m, i) => map.set(m, \"k${100000000 + i}\", \"v\"))))",
        leaves: &[
            "http.response(200, \"ok\")",
            "http.with_headers(404, \"aé\", [\"content-type\": \"text/plain\"])",
            "http.with_headers(201, \"\", [\"x-a\": \"1\", \"X-B\": \"two\"])",
            "http.json(500, \"{\\\"e\\\": 1}\")",
            "http.redirect(\"/next?a=1\")",
        ],
        observe: "(http.status_code({v}), (http.body({v}), http.headers({v})))",
        observed: || pair(GenType::Int, pair(string(), string_map())),
        digest: "map.len(http.headers({v}))",
        observe_calls_effect: false,
        refuses_var: false,
    },
    Nominal {
        name: "HttpRequest",
        module: "http",
        small: "http.new_request(\"GET\", \"/p/1?a=1\", \"aé\", [\"k\": \"v\"])",
        sized: "http.new_request(\"GET\", \"/p/1?\" + (list.range(0, {n}) |> list.map((i) => \"q${100000000 + i}=v\") |> list.join(\"&\")), \"aé\", (list.range(0, {n}) |> list.fold(map.new(), (m, i) => map.set(m, \"k${100000000 + i}\", \"v\"))))",
        leaves: &[
            "http.new_request(\"GET\", \"/\", \"\", map.new())",
            "http.new_request(\"POST\", \"/users/42\", \"aé\", [\"content-type\": \"text/plain\"])",
            "http.new_request(\"PUT\", \"/a/b?x=1&y=two\", \"\", [\"x-a\": \"1\", \"X-B\": \"two\"])",
            "http.new_request(\"DELETE\", \"/s?q=a%20b&q=c&e=\", \"z\", map.new())",
            // A JSON body: `http.decode_json` reaches its decode callback.
            "http.new_request(\"POST\", \"/j\", \"[1, \\\"aé\\\", {\\\"k\\\": null}]\", [\"content-type\": \"application/json\"])",
        ],
        observe: "((http.req_method({v}), http.req_path({v})), (http.req_body({v}), http.query_params({v})))",
        observed: || pair(pair(string(), string()), pair(string(), string_map())),
        digest: "map.len(http.query_params({v}))",
        observe_calls_effect: false,
        refuses_var: false,
    },
    Nominal {
        name: "SafeHtml",
        module: "html",
        small: "html.raw(\"<b>aé</b>\")",
        sized: "html.escape(string.repeat(\"<aé>&\", {n}))",
        leaves: &[
            "html.empty()",
            "html.raw(\"<i>x</i>\")",
            "html.escape(\"a<b>&\\\"'\")",
            "html.concat(html.raw(\"<p>\"), html.escape(\"é&\"))",
        ],
        observe: "html.to_string({v})",
        observed: string,
        digest: "string.len(html.to_string({v}))",
        observe_calls_effect: false,
        refuses_var: false,
    },
    Nominal {
        name: "SafePath",
        module: "path",
        small: "path.trusted(\"a/b.txt\")",
        sized: "path.trusted(string.repeat(\"ab/\", {n}) + \"f.txt\")",
        leaves: &["path.trusted(\"/\")", "path.trusted(\"x/../y\")", "path.trusted(\"é.d/f\")", "path.trusted(\"\")"],
        observe: "path.to_string({v})",
        observed: string,
        digest: "string.len(path.to_string({v}))",
        observe_calls_effect: false,
        refuses_var: false,
    },
    // `url.parse` is the only constructor; these inputs parse.
    Nominal {
        name: "Url",
        module: "url",
        small: "(match url.parse(\"https://h.example:8080/a?x=1#t\") { ok(u) => u, err(e) => panic(e) })",
        sized: "(match url.parse(\"https://h.example/\" + string.repeat(\"aé\", {n}) + \"?q=1\") { ok(u) => u, err(e) => panic(e) })",
        leaves: &[
            "(match url.parse(\"http://a.b\") { ok(u) => u, err(e) => panic(e) })",
            "(match url.parse(\"https://h.example:8080/a?x=1#t\") { ok(u) => u, err(e) => panic(e) })",
            "(match url.parse(\"ftp://x-y.z/%41/b?#f\") { ok(u) => u, err(e) => panic(e) })",
        ],
        observe: "url.to_string({v})",
        observed: string,
        digest: "string.len(url.to_string({v}))",
        observe_calls_effect: false,
        refuses_var: false,
    },
    // Observed by where it leads in a fixed value. The sized path is ONE
    // step whose key is n long: a path n steps deep is a structure n deep,
    // and at the judge's 16n (320k) its recursive teardown overflowed the
    // stack on both legs before any call was timed.
    Nominal {
        name: "JsonPath",
        module: "json",
        small: "json.field(json.root(), \"a\")",
        sized: "json.field(json.root(), string.repeat(\"k\", {n}))",
        leaves: &[
            "json.root()",
            "json.field(json.root(), \"a\")",
            "json.index(json.field(json.root(), \"a\"), 1)",
            "json.index(json.root(), 0)",
        ],
        observe: "option.map(json.get_path(value.object([(\"a\", value.array([value.int(1), value.str(\"x\")]))]), {v}), (jp) => value.stringify(jp))",
        observed: || GenType::Option(Box::new(GenType::String)),
        digest: "(if option.is_some(json.get_path(value.object([(\"a\", value.int(1))]), {v})) then 1 else 0)",
        observe_calls_effect: false,
        refuses_var: false,
    },
    // n rows of 2: the operands of the products agree with a 2 × 2 partner.
    Nominal {
        name: "Matrix[Float32]",
        module: "matrix",
        small: "matrix.ones_f32(2, 2)",
        sized: "matrix.zeros_f32({n}, 2)",
        leaves: &["matrix.ones_f32(2, 2)", "matrix.zeros_f32(1, 2)", "matrix.mul_f32_scaled(matrix.ones_f32(2, 2), 0.5, matrix.ones_f32(2, 2))"],
        observe: "matrix.to_lists({v})",
        observed: || GenType::List(Box::new(GenType::List(Box::new(GenType::Float)))),
        digest: "matrix.rows({v})",
        observe_calls_effect: false,
        refuses_var: false,
    },
    // A request handler; observed by what it answers a fixed request. Its
    // values are one-level lambdas, which the checker lifts to the effect
    // closure (a lambda nested in another is not lifted, #3749).
    Nominal {
        name: "HttpHandler",
        module: "http",
        small: "((req) => http.response(200, http.req_path(req) + \"|\" + (http.param(req, \"id\") ?? \"-\")))",
        sized: "((req) => http.response(200, http.req_path(req) + \"|\" + (http.param(req, \"id\") ?? \"-\")))",
        leaves: &[
            "((req) => http.response(200, http.req_path(req) + \"|\" + (http.param(req, \"id\") ?? \"-\")))",
            "((req) => http.with_headers(201, http.req_body(req), [\"x-m\": http.req_method(req)]))",
            "((req) => http.redirect(\"/r\"))",
        ],
        observe: "http.body({v}(http.new_request(\"POST\", \"/p/7?x=1\", \"b\", map.new()))!)",
        observed: string,
        digest: "string.len(http.body({v}(http.new_request(\"POST\", \"/p/7?x=1\", \"b\", map.new()))!))",
        observe_calls_effect: true,
        refuses_var: true,
    },
    // A route row; observed by what a one-row router answers. The sized
    // pattern has n literal segments before its parameter.
    Nominal {
        name: "HttpRoute",
        module: "http",
        small: "http.route(\"GET /p/{id}\", (req) => http.response(200, http.param(req, \"id\") ?? \"-\"))",
        sized: "http.route(\"GET /\" + (list.range(0, {n}) |> list.map((i) => \"s${100000000 + i}\") |> list.join(\"/\")) + \"/{id}\", (req) => http.response(200, http.param(req, \"id\") ?? \"-\"))",
        leaves: &[
            "http.route(\"GET /p/{id}\", (req) => http.response(200, http.param(req, \"id\") ?? \"-\"))",
            "http.route(\"POST /p/{rest...}\", (req) => http.response(201, http.param(req, \"rest\") ?? \"-\"))",
            "http.mount(\"/p\", (req) => http.response(202, http.req_path(req)))",
        ],
        observe: "(match http.router([{v}]) { ok(t) => http.body(t(http.new_request(\"GET\", \"/p/7\", \"\", map.new()))!), err(e) => e })",
        observed: string,
        digest: "string.len(match http.router([{v}]) { ok(t) => http.body(t(http.new_request(\"GET\", \"/p/7\", \"\", map.new()))!), err(e) => e })",
        observe_calls_effect: true,
        refuses_var: true,
    },
    // A middleware; observed by what it makes of a fixed handler. Only
    // spellings that build no closure of their own: one that does is a
    // nested lambda in an effect-closure position, refused or miscompiled
    // (#3749).
    Nominal {
        name: "HttpMiddleware",
        module: "http",
        small: "((next) => next)",
        sized: "((next) => next)",
        leaves: &["((next) => next)", "((next) => http.wrap(next, []))"],
        observe: "http.body(http.wrap((req) => http.response(200, \"base\"), [{v}])(http.new_request(\"GET\", \"/\", \"\", map.new()))!)",
        observed: string,
        digest: "string.len(http.body(http.wrap((req) => http.response(200, \"base\"), [{v}])(http.new_request(\"GET\", \"/\", \"\", map.new()))!))",
        observe_calls_effect: true,
        refuses_var: true,
    },
    // The ADR-0001 clocks: an i64 of nanoseconds behind the checker's
    // firewall, observed through Repr (S7 keeps accessors omitted). A
    // constructor's size is its count; the value never has a size of its own.
    Nominal {
        name: "Duration",
        module: "duration",
        small: "duration.ms(2)",
        sized: "duration.ns({n})",
        leaves: &["duration.ns(0)", "duration.ms(1500)", "duration.s(2) + duration.us(7)", "duration.min(3) * 2"],
        observe: "\"${ {v} }\"",
        observed: string,
        digest: "string.len(\"${ {v} }\")",
        observe_calls_effect: false,
        refuses_var: false,
    },
    Nominal {
        name: "Compute",
        module: "compute",
        small: "compute.ms(2)",
        sized: "compute.ns({n})",
        leaves: &["compute.ns(0)", "compute.ms(1500)", "compute.s(2) + compute.us(7)", "compute.h(1) - compute.min(30)"],
        observe: "\"${ {v} }\"",
        observed: string,
        digest: "string.len(\"${ {v} }\")",
        observe_calls_effect: false,
        refuses_var: false,
    },
];

/// The nominal named `name`, if the grammar builds it.
pub fn get(name: &str) -> Option<&'static Nominal> {
    NOMINALS.iter().find(|n| n.name == name)
}

/// `template` with `{v}` and `{n}` filled.
pub fn fill(template: &str, v: &str, n: u64) -> String {
    template.replace("{v}", v).replace("{n}", &n.to_string())
}
