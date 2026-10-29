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
    /// A value of this type is work under way (a call in flight): a probe's
    /// digest of a Result holding one reads it through [`Self::digest`],
    /// which waits the work out, so the work the call began is measured
    /// whole instead of racing the drop that cancels it.
    pub in_flight: bool,
}

fn string() -> GenType {
    GenType::String
}

fn pair(a: GenType, b: GenType) -> GenType {
    GenType::Tuple2(Box::new(a), Box::new(b))
}

/// `http.start(http.new_request(method, <fixture>path, body, headers), no limits)`, an `err`
/// stopping the program (the fixture is always up).
macro_rules! http_call {
    ($method:literal, $path:literal, $body:literal, $headers:literal) => {
        concat!(
            "(match http.start(http.new_request(\"",
            $method,
            "\", \"http://\" + (env.get(\"XF_HTTP\") ?? \"\") + \"",
            $path,
            "\", \"",
            $body,
            "\", ",
            $headers,
            "), { total_ms: 0, idle_ms: 0 }) { ok(hc) => hc, err(he) => panic(he) })"
        )
    };
}

/// Sized values use fixed-width keys (`k100000000`…), so a series is
/// linear in n.
pub const NOMINALS: &[Nominal] = &[
    // `fs.stat`'s record. Its mtime is the host's clock (`verdicts.rs`
    // checks it), so it is observed through the other three fields.
    Nominal {
        name: "FileStat",
        module: "fs",
        small: "{ size: 0, is_dir: false, is_file: true, modified: 0 }",
        sized: "{ size: {n}, is_dir: false, is_file: true, modified: 0 }",
        leaves: &["{ size: 0, is_dir: true, is_file: false, modified: 0 }"],
        observe: "\"${{v}.size} ${{v}.is_dir} ${{v}.is_file}\"",
        observed: string,
        digest: "{v}.size",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    // `process.output`'s record, from the fixed command set (ADR-0027 D4).
    Nominal {
        name: "ProcessStatus",
        module: "process",
        small: "{ code: 0, stdout: \"\", stderr: \"\" }",
        sized: "{ code: 0, stdout: string.repeat(\"a\", {n}), stderr: \"\" }",
        leaves: &["{ code: 1, stdout: \"\", stderr: \"e\" }"],
        observe: "\"${{v}.code}|${{v}.stdout}|${{v}.stderr}\"",
        observed: string,
        digest: "{v}.code + string.len({v}.stdout)",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    Nominal {
        name: "HttpResponse",
        module: "http",
        small: "http.with_headers(200, \"aé\", [(\"k\", \"v\")])",
        sized: "http.with_headers(200, \"aé\", (list.range(0, {n}) |> list.map((i) => (\"k${100000000 + i}\", \"v\"))))",
        leaves: &[
            "http.response(200, \"ok\")",
            "http.with_headers(404, \"aé\", [(\"content-type\", \"text/plain\")])",
            "http.with_headers(201, \"\", [(\"x-a\", \"1\"), (\"X-B\", \"two\")])",
            "http.json(500, \"{\\\"e\\\": 1}\")",
            "http.redirect(\"/next?a=1\")",
        ],
        observe: "(http.status({v}), (http.body({v}), http.headers({v})))",
        observed: || pair(GenType::Int, pair(string(), GenType::List(Box::new(pair(string(), string()))))),
        digest: "list.len(http.headers({v}))",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    Nominal {
        name: "HttpRequest",
        module: "http",
        small: "http.new_request(\"GET\", \"/p/1?a=1\", \"aé\", [(\"k\", \"v\")])",
        sized: "http.new_request(\"GET\", \"/p/1?\" + (list.range(0, {n}) |> list.map((i) => \"q${100000000 + i}=v\") |> list.join(\"&\")), \"aé\", (list.range(0, {n}) |> list.map((i) => (\"k${100000000 + i}\", \"v\"))))",
        leaves: &[
            "http.new_request(\"GET\", \"/\", \"\", [])",
            "http.new_request(\"POST\", \"/users/42\", \"aé\", [(\"content-type\", \"text/plain\")])",
            "http.new_request(\"PUT\", \"/a/b?x=1&y=two\", \"\", [(\"x-a\", \"1\"), (\"X-B\", \"two\")])",
            "http.new_request(\"DELETE\", \"/s?q=a%20b&q=c&e=\", \"z\", [])",
            // A JSON body: `http.decode_json` reaches its decode callback.
            "http.new_request(\"POST\", \"/j\", \"[1, \\\"aé\\\", {\\\"k\\\": null}]\", [(\"content-type\", \"application/json\")])",
        ],
        observe: "((http.method({v}), http.path({v})), (http.body({v}), http.query_params({v})))",
        observed: || pair(pair(string(), string()), pair(string(), GenType::Result(Box::new(GenType::List(Box::new(pair(string(), string()))))))),
        digest: "list.len(http.query_params({v}) ?? [])",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
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
        in_flight: false,
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
        in_flight: false,
    },
    // A request handler; observed by what it answers a fixed request. Its
    // values are one-level lambdas, which the checker lifts to the effect
    // closure (a lambda nested in another is not lifted, #3749).
    Nominal {
        name: "HttpHandler",
        module: "http",
        small: "((req) => http.response(200, http.path(req) + \"|\" + (http.param(req, \"id\") ?? \"-\")))",
        sized: "((req) => http.response(200, http.path(req) + \"|\" + (http.param(req, \"id\") ?? \"-\")))",
        leaves: &[
            "((req) => http.response(200, http.path(req) + \"|\" + (http.param(req, \"id\") ?? \"-\")))",
            "((req) => http.with_headers(201, http.body(req), [(\"x-m\", http.method(req))]))",
            "((req) => http.redirect(\"/r\"))",
        ],
        observe: "http.body({v}(http.new_request(\"POST\", \"/p/7?x=1\", \"b\", []))!)",
        observed: string,
        digest: "string.len(http.body({v}(http.new_request(\"POST\", \"/p/7?x=1\", \"b\", []))!))",
        observe_calls_effect: true,
        refuses_var: true,
        in_flight: false,
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
            "http.mount(\"/p\", (req) => http.response(202, http.path(req)))",
        ],
        observe: "(match http.router([{v}]) { ok(t) => http.body(t(http.new_request(\"GET\", \"/p/7\", \"\", []))!), err(e) => e })",
        observed: string,
        digest: "string.len(match http.router([{v}]) { ok(t) => http.body(t(http.new_request(\"GET\", \"/p/7\", \"\", []))!), err(e) => e })",
        observe_calls_effect: true,
        refuses_var: true,
        in_flight: false,
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
        observe: "http.body(http.wrap((req) => http.response(200, \"base\"), [{v}])(http.new_request(\"GET\", \"/\", \"\", []))!)",
        observed: string,
        digest: "string.len(http.body(http.wrap((req) => http.response(200, \"base\"), [{v}])(http.new_request(\"GET\", \"/\", \"\", []))!))",
        observe_calls_effect: true,
        refuses_var: true,
        in_flight: false,
    },
    // A call's limits. Only limits no fixture exchange reaches (0 = none, or
    // far past what the loopback answers in): a limit that fires is the
    // host's timing, and a call that took it is the error path.
    Nominal {
        name: "HttpLimits",
        module: "http",
        small: "{ total_ms: 0, idle_ms: 0 }",
        sized: "{ total_ms: 0, idle_ms: 0 }",
        leaves: &[
            "{ total_ms: 0, idle_ms: 0 }",
            "{ total_ms: 60000, idle_ms: 0 }",
            "{ total_ms: 0, idle_ms: 30000 }",
            "{ total_ms: 60000, idle_ms: 30000 }",
        ],
        observe: "\"${{v}.total_ms} ${{v}.idle_ms}\"",
        observed: string,
        digest: "{v}.total_ms + {v}.idle_ms",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    // A call in flight against the loopback fixture (ADR-0027 D5). It has
    // no value of its own: it is observed by how it ends (`http.wait`),
    // which no read and no poll changes (the whole body stays the
    // response's), and which no generated program can cancel — `cancel`'s
    // operand is a fresh call of its own (`verdicts.rs`). The sized call
    // answers n bytes.
    Nominal {
        name: "HttpCall",
        module: "http",
        small: http_call!("GET", "/chunked/3", "", "[]"),
        sized: http_call!("GET", "/bytes/{n}", "", "[]"),
        leaves: &[
            http_call!("GET", "/chunked/3", "", "[]"),
            http_call!("GET", "/bytes/16", "", "[]"),
            http_call!("POST", "/echo", "aé", "[(\"x-k\", \"v\")]"),
            http_call!("GET", "/status/404", "", "[]"),
            http_call!("GET", "/sse/2", "", "[]"),
            http_call!("DELETE", "/missing", "", "[]"),
        ],
        observe: "(match http.wait({v}) { ok(hr) => \"${http.status(hr)}|${http.body(hr)}\", err(he) => he })",
        observed: string,
        digest: "(match http.wait({v}) { ok(hr) => string.len(http.body(hr)), err(_) => 0 })",
        observe_calls_effect: true,
        refuses_var: false,
        in_flight: true,
    },
    // A bounds-checked view of memory (ADR-0031), from the two mints the
    // language has (an `@extern` with `@region(len = …)` is the third, and
    // needs a foreign library): `ffi.region(b)`, read-only and the length of
    // `b`, and `ffi.alloc(n)`, writable and n zero bytes — each at length 0
    // as well. The sized value is writable, so a probe of `ffi.write` takes
    // the success path. Observed through the module's readers, its bytes
    // included (`to_bytes` is an effect fn: an error there is an `err`).
    Nominal {
        name: "Region",
        module: "ffi",
        small: "ffi.region(bytes.from_list([1, 2, 3, 4]))",
        sized: "(match ffi.alloc({n}) { ok(r) => r, err(e) => panic(e) })",
        leaves: &[
            "ffi.region(bytes.new())",
            "ffi.region(bytes.from_list([1, 2, 3, 4]))",
            "ffi.region(bytes.from_string(\"aé\"))",
            "(match ffi.alloc(0) { ok(r) => r, err(e) => panic(e) })",
            "(match ffi.alloc(1) { ok(r) => r, err(e) => panic(e) })",
            "(match ffi.alloc(4) { ok(r) => r, err(e) => panic(e) })",
            "(match ffi.alloc(16) { ok(r) => r, err(e) => panic(e) })",
        ],
        observe: "(ffi.len({v}), (ffi.is_writable({v}), bytes.to_list(ffi.to_bytes({v})!)))",
        observed: || pair(GenType::Int, pair(GenType::Bool, GenType::List(Box::new(GenType::Int)))),
        digest: "ffi.len({v})",
        observe_calls_effect: true,
        refuses_var: false,
        in_flight: false,
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
        in_flight: false,
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
        in_flight: false,
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
