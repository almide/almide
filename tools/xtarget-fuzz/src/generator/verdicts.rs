//! What the host decides is compared as a verdict, not as a value
//! (ADR-0027 D3). A function whose result the host chooses (an entropy
//! draw, a clock, a pid, a temp name) cannot agree byte for byte across
//! legs or runs, so neither the generator nor the inspection probes ever
//! print it. Each call is checked against the properties its declaration
//! promises (`holds`), then stands for a fixed value of the same type
//! (`canon`): a leg whose result breaks a property panics, the other does
//! not, and the campaign sees a divergence; a leg whose result holds goes on
//! with the same value as every other leg.
//!
//! The templates name the call's parameters `{param}` and its value `v`
//! (the ok payload of a Result-returning effect call). This table is the one
//! place a function is classified as host-decided: the catalogue admits it
//! (past the effect-module filter and the nondeterminism denylist) only
//! here, at weight 0 so only a focused campaign draws it.

/// One host-decided function.
#[derive(Debug)]
pub struct Verdict {
    pub func: &'static str,
    /// A Bool over `v` and the parameters: the promised properties.
    pub holds: &'static str,
    /// An expression of the result's type over the parameters: what the
    /// rest of the program sees.
    pub canon: &'static str,
    /// Generated arguments brought into the function's domain before the
    /// call (`sleep_ms` hangs on a negative ms, by declaration).
    pub domain: &'static [(&'static str, &'static str)],
}

/// 2026-01-01T00:00:00Z: no clock that reads earlier than this is right.
macro_rules! epoch_floor_s {
    () => {
        "1767225600"
    };
}

pub const VERDICTS: &[Verdict] = &[
    Verdict { func: "random.int", holds: "if {max} <= {min} then v == {min} else v >= {min} and v <= {max}", canon: "{min}", domain: &[] },
    Verdict { func: "random.float", holds: "v >= 0.0 and v < 1.0", canon: "0.0", domain: &[] },
    Verdict {
        func: "random.choice",
        holds: "match v { some(e) => list.contains({xs}, e), none => list.is_empty({xs}) }",
        canon: "list.first({xs})",
        domain: &[],
    },
    // A permutation: as long, and holding every element as often.
    Verdict {
        func: "random.shuffle",
        holds: "list.len(v) == list.len({xs}) and list.all({xs}, (e) => list.count(v, (f) => f == e) == list.count({xs}, (f) => f == e))",
        canon: "{xs}",
        domain: &[],
    },
    Verdict {
        func: "list.shuffle",
        holds: "list.len(v) == list.len({xs}) and list.all({xs}, (e) => list.count(v, (f) => f == e) == list.count({xs}, (f) => f == e))",
        canon: "{xs}",
        domain: &[],
    },
    Verdict { func: "datetime.now", holds: concat!("v >= ", epoch_floor_s!()), canon: "0", domain: &[] },
    Verdict { func: "env.unix_timestamp", holds: concat!("v >= ", epoch_floor_s!()), canon: "0", domain: &[] },
    Verdict { func: "env.millis", holds: concat!("v >= ", epoch_floor_s!(), "000"), canon: "0", domain: &[] },
    Verdict { func: "datetime.monotonic_ns", holds: "v >= 0", canon: "0", domain: &[] },
    // argv[0] is the program as the host names it (a native path, the wasm
    // runner's name); the arguments after it are the sandbox's.
    Verdict { func: "process.args", holds: "list.len(v) >= 1", canon: "[\"prog\"] + list.drop(v, 1)", domain: &[] },
    Verdict { func: "fs.stat", holds: concat!("v.modified >= ", epoch_floor_s!()), canon: "v", domain: &[] },
    Verdict { func: "fs.modified_at", holds: concat!("v >= ", epoch_floor_s!()), canon: "0", domain: &[] },
    Verdict { func: "process.pid", holds: "v > 0", canon: "1", domain: &[] },
    Verdict { func: "env.os", holds: "list.contains([\"macos\", \"linux\", \"windows\", \"wasi\", \"unknown\"], v)", canon: "\"os\"", domain: &[] },
    Verdict { func: "env.cwd", holds: "string.len(v) > 0", canon: "\".\"", domain: &[] },
    Verdict { func: "env.temp_dir", holds: "string.len(v) > 0", canon: "\"tmp\"", domain: &[] },
    Verdict { func: "fs.temp_dir", holds: "string.len(v) > 0", canon: "\"tmp\"", domain: &[] },
    Verdict { func: "fs.create_temp_file", holds: "string.contains(v, {prefix})", canon: "{prefix}", domain: &[] },
    Verdict { func: "fs.create_temp_dir", holds: "string.contains(v, {prefix})", canon: "{prefix}", domain: &[] },
    Verdict { func: "process.is_alive", holds: "v or {pid} != process.pid()", canon: "false", domain: &[] },
    // Unit results: nothing to check but that the call returns, with a
    // short, non-negative wait.
    Verdict { func: "env.sleep_ms", holds: "true", canon: "()", domain: &[("ms", "int.abs({ms}) % 5")] },
    Verdict { func: "process.sleep", holds: "true", canon: "()", domain: &[("ms", "int.abs({ms}) % 5")] },
    // A child's pid; the fixed command set exits at once.
    Verdict { func: "process.spawn", holds: "v > 0", canon: "0", domain: &[] },
    // A signal reaches only a child of this program's own: a generated pid
    // could name any process on the host. Past every pid_max, the fallback
    // fails with ESRCH.
    Verdict {
        func: "process.kill",
        holds: "true",
        canon: "()",
        domain: &[
            ("pid", "match process.spawn(\"/bin/sleep\", [\"1\"]) { ok(p) => p, err(_) => 2147483000 }"),
            ("signal", "int.abs({signal}) % 2 * 6 + 9"),
        ],
    },
    // How much of the echo has arrived is the network's timing.
    Verdict { func: "net.tcp_read", holds: "bytes.len(v) <= {len} or {len} < 0", canon: "bytes.from_list([])", domain: &[] },
    Verdict { func: "net.tcp_read_timeout", holds: "bytes.len(v) <= {len} or {len} < 0", canon: "bytes.from_list([])", domain: &[] },
    // A socket's handle counts the sockets opened before it: two paths that
    // each open one see different handles. The value the program goes on
    // with is the handle itself (the legs number them alike).
    Verdict { func: "net.tcp_connect", holds: "v >= 0", canon: "v", domain: &[] },
    Verdict { func: "net.tcp_listen", holds: "v >= 0", canon: "v", domain: &[] },
    Verdict { func: "net.tcp_accept", holds: "v >= 0", canon: "v", domain: &[] },
    // The OS picks a listener's ephemeral port.
    Verdict { func: "net.tcp_local_port", holds: "v > 0 and v < 65536", canon: "0", domain: &[] },
    Verdict { func: "net.tcp_available", holds: "v >= 0", canon: "0", domain: &[] },
    // Exactly len of the 11 echoed bytes: never more than arrive.
    Verdict { func: "net.tcp_read_exact", holds: "bytes.len(v) == {len}", canon: "v", domain: &[("len", "int.abs({len}) % 12")] },
    // A buffer's address reads back the buffer while it lives (checked on
    // its first byte: the probe's check stays O(1)); a generated program goes
    // on with null, so it never holds an address past its buffer.
    Verdict {
        func: "bytes.as_ptr",
        holds: "bytes.from_raw_ptr(v, int.min(1, bytes.len({b}))) == bytes.slice({b}, 0, int.min(1, bytes.len({b})))",
        canon: "bytes.as_ptr(bytes.from_list([]))",
        domain: &[],
    },
    Verdict {
        func: "bytes.as_mut_ptr",
        holds: "bytes.from_raw_ptr(v, int.min(1, bytes.len({b}))) == bytes.slice({b}, 0, int.min(1, bytes.len({b})))",
        canon: "bytes.as_ptr(bytes.from_list([]))",
        domain: &[],
    },
    Verdict { func: "bytes.data_ptr", holds: "v > 0 or bytes.len({b}) == 0", canon: "0", domain: &[] },
    // Through a generated (null) pointer nothing is written or read.
    Verdict { func: "bytes.copy_to_ptr", holds: "v >= 0 and v <= bytes.len({b})", canon: "v", domain: &[] },
    Verdict { func: "mem.save", holds: "true", canon: "0", domain: &[] },
];

/// The O(1) form of a property checked inside a timed loop, where the full
/// check's own cost (a membership scan, a per-element count) would be
/// measured as the call's: the full form still runs on every diamond path
/// and in every generated program.
const PROBES: &[(&str, &str)] = &[
    ("random.choice", "match v { some(_) => not list.is_empty({xs}), none => list.is_empty({xs}) }"),
    ("random.shuffle", "list.len(v) == list.len({xs})"),
    ("list.shuffle", "list.len(v) == list.len({xs})"),
];

impl Verdict {
    /// [`Self::holds`], or its O(1) form when `cheap` and the table has one.
    pub fn property(&self, cheap: bool) -> &'static str {
        PROBES.iter().find(|(f, _)| cheap && *f == self.func).map_or(self.holds, |(_, p)| *p)
    }
}

/// The verdict for `module.func`, if the host decides its result.
pub fn verdict_for(module: &str, func: &str) -> Option<&'static Verdict> {
    VERDICTS.iter().find(|v| v.func.split_once('.') == Some((module, func)))
}

/// `tmpl` with each `{param}` replaced by the expression bound to it.
pub fn fill(tmpl: &str, params: &[(&str, &str)]) -> String {
    params.iter().fold(tmpl.to_string(), |s, (p, e)| s.replace(&format!("{{{p}}}"), e))
}
