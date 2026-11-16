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

/// The ok payload of a Result-returning constructor, an `err` stopping the
/// program (every constructor here is given input it accepts).
macro_rules! ok_or_panic {
    ($e:literal) => {
        concat!("(match ", $e, " { ok(okv) => okv, err(e) => panic(e) })")
    };
}

/// A stream to the loopback echo fixture with "hello world" written, so
/// 11 bytes are on their way back.
macro_rules! tcp_echo {
    () => {
        "(match net.tcp_connect(\"127.0.0.1\", int.parse(env.get(\"XF_TCP_PORT\") ?? \"0\") ?? 0) { ok(ts) => { let _tw: Result[Unit, String] = io.write(ts, bytes.from_string(\"hello world\"))\n ts }, err(e) => panic(e) })"
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
        // One header past n: a header looked up by name (`http.header`'s last
        // key) is present at n = 0 too, the fixed-cost probe's size.
        sized: "http.with_headers(200, \"aé\", (list.range(0, {n} + 1) |> list.map((i) => (\"k${100000000 + i}\", \"v\"))))",
        leaves: &[
            "http.response(200, \"ok\")",
            "http.with_headers(404, \"aé\", [(\"x-kind\", \"text/plain\")])",
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
        sized: "http.new_request(\"GET\", \"/p/1?\" + (list.range(0, {n}) |> list.map((i) => \"q${100000000 + i}=v\") |> list.join(\"&\")), \"aé\", (list.range(0, {n} + 1) |> list.map((i) => (\"k${100000000 + i}\", \"v\"))))",
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
    // A response body of a declared media type (ADR-0032), from its one
    // constructor `http.typed`, whose media type is a literal (E117). It has
    // no reader of its own: observed as the response it makes, through the
    // Content-Type and body that response carries.
    Nominal {
        name: "TypedBody",
        module: "http",
        small: "http.typed(\"text/csv\", bytes.from_string(\"a,é\"))",
        sized: "http.typed(\"application/octet-stream\", bytes.from_string(string.repeat(\"aé\", {n})))",
        leaves: &[
            "http.typed(\"text/csv\", bytes.new())",
            "http.typed(\"text/csv; charset=utf-8\", bytes.from_string(\"a,b\\n1,é\"))",
            "http.typed(\"image/png\", bytes.from_list([137, 80, 78, 71]))",
            "http.typed(\"application/octet-stream\", bytes.from_list([0, 255, 128]))",
        ],
        observe: "(http.header(http.response(200, {v}), \"Content-Type\") ?? \"-\", http.body(http.response(200, {v})))",
        observed: || pair(GenType::String, GenType::String),
        // No reader leaves the bytes alone: making the response decodes them
        // (`__self_to_text`), so a per-call digest through it would time the
        // decode, not the call (#4337). The digest is constant, as a tuple's.
        digest: "1",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
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
            "(match ffi.slice(ffi.region(bytes.from_list([1, 2, 3, 4])), 1, 2) { ok(r) => r, err(e) => panic(e) })",
            "(match ffi.alloc(8) { ok(w) => match ffi.slice(w, 3, 4) { ok(r) => r, err(e) => panic(e) }, err(e) => panic(e) })",
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
    // An arbitrary-precision integer. The sized value is 2^n + 1234567: n + 1
    // bits, built by one shift (a decimal parse of n digits is quadratic).
    Nominal {
        name: "BigInt",
        module: "bigint",
        small: "bigint.from(1234567)",
        sized: "bigint.bor(bigint.bshl(bigint.from(1), {n}), bigint.from(1234567))",
        leaves: &[
            "bigint.from(0)",
            "bigint.from(-7)",
            "bigint.from(1234567)",
            "bigint.bshl(bigint.from(3), 70)",
            ok_or_panic!("bigint.parse(\"-98765432109876543210987654321\")"),
            ok_or_panic!("bigint.parse_radix(\"ff00ff00ff00ff00ff00\", 16)"),
        ],
        observe: "bigint.to_string({v})",
        observed: string,
        digest: "bigint.bit_length({v})",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    // A UUID from its text or its 16 bytes; it has no size.
    Nominal {
        name: "Uuid",
        module: "uuid",
        small: ok_or_panic!("uuid.parse(\"0190a6e2-7c3b-7d4e-8f00-123456789abc\")"),
        sized: ok_or_panic!("uuid.parse(\"0190a6e2-7c3b-7d4e-8f00-123456789abc\")"),
        leaves: &[
            ok_or_panic!("uuid.parse(\"00000000-0000-0000-0000-000000000000\")"),
            ok_or_panic!("uuid.parse(\"0190a6e2-7c3b-7d4e-8f00-123456789abc\")"),
            ok_or_panic!("uuid.parse(\"F47AC10B-58CC-4372-A567-0E02B2C3D479\")"),
            ok_or_panic!("uuid.from_bytes(bytes.from_list([255, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 254]))"),
        ],
        observe: "uuid.to_string({v})",
        observed: string,
        digest: "bytes.len(uuid.to_bytes({v}))",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    // A compiled pattern, observed through what it finds in a fixed text.
    // The sized pattern is n literal `ab` pairs.
    Nominal {
        name: "Regex",
        module: "regex",
        small: "regex.new(\"a+(b)\")",
        sized: "regex.new(string.repeat(\"ab\", {n}))",
        leaves: &[
            "regex.new(\"a+(b)\")",
            "regex.new(\"(é)\")",
            "regex.new(\"[a-c]+\")",
            "regex.new(\"^\\\\s*$\")",
            "regex.new(\"(\\\\w+)@(\\\\w+)\")",
            "regex.new(\"x|y*\")",
        ],
        observe: "regex.find_all({v}, \"aab ab aé b x@y \")",
        observed: || GenType::List(Box::new(GenType::String)),
        digest: "string.len({v}.pattern)",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    // A seeded generator (pure: the same seed draws the same values on
    // every leg), observed through its first draw.
    Nominal {
        name: "Rng",
        module: "random",
        small: "random.seeded(42)",
        sized: "random.seeded({n})",
        leaves: &["random.seeded(0)", "random.seeded(1)", "random.seeded(-7)", "random.seeded(123456789)"],
        observe: "random.next_int({v}, 0, 1000000).0",
        observed: || GenType::Int,
        digest: "random.next_int({v}, 0, 1000000).0",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    // A path into a Value. The sized path is n key steps; it is observed
    // through what it finds in a fixed value.
    Nominal {
        name: "Path",
        module: "value",
        small: "value.key(value.root(), \"a\")",
        sized: "list.range(0, {n}) |> list.fold(value.root(), (vp, i) => value.key(vp, \"k${100000000 + i}\"))",
        leaves: &[
            "value.root()",
            "value.key(value.root(), \"a\")",
            "value.index(value.key(value.root(), \"a\"), 1)",
            "value.key(value.key(value.root(), \"snake_case\"), \"k\")",
            "value.index(value.root(), -1)",
        ],
        observe: "json.stringify(value.get_path(value.object([(\"a\", value.array([value.int(1), value.int(2)])), (\"snake_case\", value.object([(\"k\", value.null())]))]), {v}) ?? value.string(\"-\"))",
        observed: string,
        digest: "(match value.get_path(value.null(), {v}) { some(_) => 1, none => 0 })",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    // A directory capability over the run's sandbox (its cwd).
    Nominal {
        name: "FsRoot",
        module: "fs",
        small: ok_or_panic!("fs.open_root(\".\")"),
        sized: ok_or_panic!("fs.open_root(\".\")"),
        leaves: &[ok_or_panic!("fs.open_root(\".\")"), ok_or_panic!("fs.open_root(\"sub\")")],
        observe: "fs.root_list_dir({v}, \".\")!",
        observed: || GenType::List(Box::new(GenType::String)),
        digest: "1",
        observe_calls_effect: true,
        refuses_var: false,
        in_flight: false,
    },
    // A child's settings, over the fixed command set (ADR-0027 D4). The
    // sized command passes n / 5 arguments.
    Nominal {
        name: "ProcessCommand",
        module: "process",
        small: "process.ProcessCommand { cmd: \"/bin/echo\", args: [\"aé\"] }",
        sized: "process.ProcessCommand { cmd: \"/bin/echo\", args: list.range(0, {n} / 5) |> list.map((i) => \"aé\") }",
        leaves: &[
            "process.ProcessCommand { cmd: \"/bin/echo\", args: [\"aé\", \"b\"] }",
            "process.ProcessCommand { cmd: \"/usr/bin/true\", args: [] }",
            "process.ProcessCommand { cmd: \"/usr/bin/false\", args: [] }",
            "process.ProcessCommand { cmd: \"/bin/cat\", args: [], stdin: \"in aé\" }",
            "process.ProcessCommand { cmd: \"/bin/echo\", args: [\"d\"], dir: \"sub\" }",
            "process.ProcessCommand { cmd: \"/no/such/command\", args: [] }",
        ],
        observe: "\"${{v}.cmd} ${list.len({v}.args)} ${{v}.dir} ${{v}.stdin}\"",
        observed: string,
        digest: "list.len({v}.args)",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    // A started child, observed by how it ends (`wait`). Only commands that
    // start: a start that fails is the error path.
    Nominal {
        name: "ProcessChild",
        module: "process",
        small: ok_or_panic!("process.start(process.ProcessCommand { cmd: \"/bin/echo\", args: [\"aé\"] })"),
        sized: ok_or_panic!("process.start(process.ProcessCommand { cmd: \"/bin/echo\", args: list.range(0, {n} / 5) |> list.map((i) => \"aé\") })"),
        leaves: &[
            ok_or_panic!("process.start(process.ProcessCommand { cmd: \"/bin/echo\", args: [\"aé\"] })"),
            ok_or_panic!("process.start(process.ProcessCommand { cmd: \"/usr/bin/true\", args: [] })"),
            ok_or_panic!("process.start(process.ProcessCommand { cmd: \"/usr/bin/false\", args: [] })"),
            ok_or_panic!("process.start(process.ProcessCommand { cmd: \"/bin/cat\", args: [], stdin: \"in aé\" })"),
        ],
        observe: "{ let pw = process.wait({v})!\n \"${pw.code}|${pw.stdout}|${pw.stderr}\" }",
        observed: string,
        digest: "1",
        observe_calls_effect: true,
        refuses_var: false,
        in_flight: false,
    },
    // The standard streams.
    Nominal {
        name: "Stdin",
        module: "io",
        small: ok_or_panic!("io.stdin()"),
        sized: ok_or_panic!("io.stdin()"),
        leaves: &[ok_or_panic!("io.stdin()")],
        observe: "\"stdin\"",
        observed: string,
        digest: "1",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    Nominal {
        name: "Stdout",
        module: "io",
        small: ok_or_panic!("io.stdout()"),
        sized: ok_or_panic!("io.stdout()"),
        leaves: &[ok_or_panic!("io.stdout()")],
        observe: "\"stdout\"",
        observed: string,
        digest: "1",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    Nominal {
        name: "Stderr",
        module: "io",
        small: ok_or_panic!("io.stderr()"),
        sized: ok_or_panic!("io.stderr()"),
        leaves: &[ok_or_panic!("io.stderr()")],
        observe: "\"stderr\"",
        observed: string,
        digest: "1",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    // A stream to the loopback echo fixture with "hello world" on its way
    // back, so a read never waits on nothing (ADR-0027 D5).
    Nominal {
        name: "TcpStream",
        module: "net",
        small: tcp_echo!(),
        sized: tcp_echo!(),
        leaves: &[tcp_echo!()],
        observe: "net.tcp_is_open({v})!",
        observed: || GenType::Bool,
        digest: "1",
        observe_calls_effect: true,
        refuses_var: false,
        in_flight: false,
    },
    // A listener on an ephemeral loopback port with one connection pending,
    // so an accept never waits.
    Nominal {
        name: "TcpListener",
        module: "net",
        small: TCP_PENDING_LISTENER,
        sized: TCP_PENDING_LISTENER,
        leaves: &[TCP_PENDING_LISTENER],
        observe: "net.tcp_local_port({v})! > 0",
        observed: || GenType::Bool,
        digest: "1",
        observe_calls_effect: true,
        refuses_var: false,
        in_flight: false,
    },
    // A UDP socket on an ephemeral loopback port.
    Nominal {
        name: "UdpSocket",
        module: "net",
        small: ok_or_panic!("net.udp_bind(\"127.0.0.1\", 0)"),
        sized: ok_or_panic!("net.udp_bind(\"127.0.0.1\", 0)"),
        leaves: &[ok_or_panic!("net.udp_bind(\"127.0.0.1\", 0)")],
        observe: "net.udp_local_port({v})! > 0",
        observed: || GenType::Bool,
        digest: "1",
        observe_calls_effect: true,
        refuses_var: false,
        in_flight: false,
    },
    // A received datagram; its port is the sender's ephemeral one, so it is
    // observed through its data and host.
    Nominal {
        name: "UdpPacket",
        module: "net",
        small: "{ data: bytes.from_list([1, 2]), host: \"127.0.0.1\", port: 9 }",
        sized: "{ data: bytes.zeros({n}), host: \"127.0.0.1\", port: 9 }",
        leaves: &["{ data: bytes.new(), host: \"::1\", port: 0 }"],
        observe: "\"${bytes.to_list({v}.data)} ${{v}.host}\"",
        observed: string,
        digest: "bytes.len({v}.data)",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    Nominal {
        name: "IpAddr",
        module: "net",
        small: ok_or_panic!("net.parse_ip(\"127.0.0.1\")"),
        sized: ok_or_panic!("net.parse_ip(\"2001:db8::ff00:42:8329\")"),
        leaves: &[
            ok_or_panic!("net.parse_ip(\"127.0.0.1\")"),
            ok_or_panic!("net.parse_ip(\"10.0.0.255\")"),
            ok_or_panic!("net.parse_ip(\"::1\")"),
            ok_or_panic!("net.parse_ip(\"2001:db8::ff00:42:8329\")"),
        ],
        observe: "net.ip_to_string({v})",
        observed: string,
        digest: "string.len(net.ip_to_string({v}))",
        observe_calls_effect: false,
        refuses_var: false,
        in_flight: false,
    },
    // A descriptor of the program's own (#3620): a shared-memory file, whose
    // reads end (a socket to the sink would wait forever on a read).
    // Calls that need a socket build their own (`net_unix_args.rs`).
    Nominal {
        name: "Fd",
        module: "unix",
        small: ok_or_panic!("unix.shm_create(8)"),
        sized: ok_or_panic!("unix.shm_create({n})"),
        leaves: &[ok_or_panic!("unix.shm_create(8)"), ok_or_panic!("unix.shm_create(2)"), ok_or_panic!("unix.shm_create(3)")],
        // A socket has no offset (shm_read is ESPIPE on it): it reads as [].
        // Any err reads as [] (`??`): branching on an error's text is E035,
        // which the generated program must not trip.
        observe: "bytes.to_list(unix.shm_read({v}, 0, 2) ?? bytes.new())",
        observed: || GenType::List(Box::new(GenType::Int)),
        digest: "1",
        observe_calls_effect: true,
        refuses_var: false,
        in_flight: false,
    },
];

/// The listener of [`NOMINALS`]' `TcpListener`.
const TCP_PENDING_LISTENER: &str = "(match net.tcp_listen(\"127.0.0.1\", 0) { ok(tl) => { let _tc: Result[TcpStream, String] = net.tcp_connect(\"127.0.0.1\", net.tcp_local_port(tl) ?? 0)\n tl }, err(e) => panic(e) })";

/// Nominals a program names by their module (`value.Path`): the bare name
/// is another type in a program's own scope.
const QUALIFIED: &[(&str, &str)] = &[("Path", "value.Path")];

/// How a program spells nominal `name` in a type annotation.
pub fn spelled(name: &str) -> &str {
    QUALIFIED
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(name, |(_, q)| *q)
}

/// The nominal named `name`, if the grammar builds it.
pub fn get(name: &str) -> Option<&'static Nominal> {
    NOMINALS.iter().find(|n| n.name == name)
}

/// `template` with `{v}` and `{n}` filled.
pub fn fill(template: &str, v: &str, n: u64) -> String {
    template.replace("{v}", v).replace("{n}", &n.to_string())
}
