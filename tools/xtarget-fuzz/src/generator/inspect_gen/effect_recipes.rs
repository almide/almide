//! Recipes for the effect functions inspected in the fixed world
//! (ADR-0027). Every probe run gets a fresh sandbox (`oracle/sandbox.rs`) as
//! its cwd, with the sandbox's argv, env and stdin, so a recipe builds what
//! its call needs there before the timed calls, and the call always takes
//! the success path (#3671). A call that consumes what it is handed
//! (`remove`, `rename`) gets a fresh one every step (`{i}`): the step's cost
//! is paid by both programs and cancels.

use super::Recipe;

/// `in.txt`, n lines of "aé".
macro_rules! file_of_n_lines {
    () => {
        "{ fs.write_text(\"in.txt\", string.repeat(\"aé\\n\", {n}))!\n  \"in.txt\" }"
    };
}

/// `d/`, holding n / 10 + 3 one-byte files of one name width (a sort over
/// names of mixed widths moves a mixed number of bytes): every file is a host call to
/// create, and 16n of them (320000) took the run past its cap. Not n / 100:
/// wasmtime reads a directory through rustix, whose getdents buffer starts
/// small, grows by 32 dirents per refill up to ~24 KiB and is zeroed on every
/// refill, so below ~1600 entries its zeroing per entry falls as the
/// directory grows — the stock fs.list_dir's bytes moved read 55, 59 and 14
/// per entry at 203, 803 and 3203 entries (GCE, per call site) and fitted no
/// class. From 2003 entries on every size is past that warm-up.
macro_rules! dir_of_n_files {
    () => {
        "{ fs.mkdir_p(\"d\")!\n  for j in list.range(0, {n} / 10 + 3) { fs.write_text(\"d/f${100000 + j}\", \"x\")! }\n  \"d\" }"
    };
}

/// A fresh one-byte file per step.
macro_rules! fresh_file {
    ($stem:literal) => {
        concat!(
            "{ fs.write_text(\"",
            $stem,
            "${{i}}.txt\", \"x\")!\n  \"",
            $stem,
            "${{i}}.txt\" }"
        )
    };
}

const FILE_N: &[(&str, &str)] = &[("path", file_of_n_lines!())];

pub(super) const EFFECT_RECIPES: &[Recipe] = &[
    // Readers of a file of n lines.
    Recipe { func: "fs.read_text", size: "path", args: FILE_N },
    Recipe { func: "fs.read_text_if_exists", size: "path", args: FILE_N },
    Recipe { func: "fs.read_lines", size: "path", args: FILE_N },
    Recipe { func: "fs.read_lines_if_exists", size: "path", args: FILE_N },
    Recipe { func: "fs.read_bytes", size: "path", args: FILE_N },
    Recipe { func: "fs.read_bytes_if_exists", size: "path", args: FILE_N },
    Recipe { func: "fs.fold_lines", size: "path", args: FILE_N },
    Recipe { func: "fs.fold_lines_chunked", size: "path", args: &[("path", file_of_n_lines!()), ("workers", "2")] },
    Recipe { func: "fs.fold_lines_range", size: "path", args: &[("path", file_of_n_lines!()), ("start", "0"), ("end", "{n}")] },
    Recipe { func: "fs.for_each_line", size: "path", args: FILE_N },
    Recipe { func: "fs.copy", size: "src", args: &[("src", file_of_n_lines!()), ("dst", "\"copy.txt\"")] },
    // Metadata of a file of n lines: the work is the host's, not the size's.
    Recipe { func: "fs.exists", size: "path", args: FILE_N },
    Recipe { func: "fs.is_file", size: "path", args: FILE_N },
    // The seeded link, reached from a diamond's project directory too.
    Recipe { func: "fs.is_symlink", size: "", args: &[("path", "(env.get(\"XF_SANDBOX\") ?? \".\") + \"/link.txt\"")] },
    // A directory of n files.
    Recipe { func: "fs.is_dir", size: "path", args: &[("path", dir_of_n_files!())] },
    Recipe { func: "fs.list_dir", size: "path", args: &[("path", dir_of_n_files!())] },
    Recipe { func: "fs.walk", size: "dir", args: &[("dir", dir_of_n_files!())] },
    Recipe { func: "fs.glob", size: "pattern", args: &[("pattern", concat!("{ let d = ", dir_of_n_files!(), "\n  d + \"/*\" }"))] },
    // Writers of n bytes of content.
    Recipe { func: "fs.write_text", size: "content", args: &[("path", "\"out.txt\""), ("content", "string.repeat(\"aé\\n\", {n})")] },
    // A whole number of the stock service's 64 KiB write slices at every
    // measured size (65536 bytes at n = 20000): one fd_write per slice, so
    // 4n bytes cost a + b·⌈4n / 65536⌉ — 2, 3, 5, 10, 20 host calls — and that
    // staircase read nlogn (GCE: 5.185e6 / 6.576e6 / 9.355e6 / 1.631e7 /
    // 3.021e7, exactly 9.5e3 + 5.4e3 per slice per call). Aligned, the series
    // is the per-slice (per-byte) cost.
    Recipe { func: "fs.append_text", size: "content", args: &[("path", "\"out.txt\""), ("content", "string.repeat(\"aé\\n\", {n} * 4096 / 5000)")] },
    Recipe {
        func: "fs.append_bytes",
        size: "data",
        args: &[("path", "\"out.bin\""), ("data", "bytes.from_list(list.range(0, {n} * 4096 / 5000) |> list.map((i) => i % 256))")],
    },
    Recipe {
        func: "fs.write_bytes",
        size: "data",
        args: &[("path", "\"out.bin\""), ("data", "bytes.from_list(list.range(0, {n}) |> list.map((i) => i % 256))")],
    },
    // Calls that consume or create one thing.
    Recipe { func: "fs.mkdir_p", size: "", args: &[("path", "\"m/a/b\"")] },
    Recipe { func: "fs.remove", size: "", args: &[("path", fresh_file!("r"))] },
    Recipe { func: "fs.rename", size: "", args: &[("src", fresh_file!("s")), ("dst", "\"moved.txt\"")] },
    Recipe {
        func: "fs.remove_all",
        size: "",
        args: &[("path", "{ fs.mkdir_p(\"t${{i}}/a\")!\n  fs.write_text(\"t${{i}}/a/f\", \"x\")!\n  \"t${{i}}\" }")],
    },
    // argv and env (oracle/sandbox.rs ARGS / ENV).
    Recipe { func: "args.has_flag", size: "", args: &[("name", "\"flag\"")] },
    Recipe { func: "args.option", size: "", args: &[("name", "\"opt\"")] },
    Recipe { func: "args.positional", size: "", args: &[("options", "[]")] },
    Recipe { func: "env.get", size: "", args: &[("name", "\"XF_GREETING\"")] },
    Recipe { func: "env.set", size: "value", args: &[("name", "\"XF_SET\""), ("value", "string.repeat(\"aé\", {n})")] },
    // Past the native 64 KiB stdout buffer at every size, so every size
    // writes in one regime (as io.write's recipe did).
    Recipe { func: "io.print", size: "s", args: &[("s", "string.repeat(\"aé\", 4 * {n})")] },
    // Host-decided results (`verdicts.rs`): each probe checks the declared
    // properties instead of reading the value.
    Recipe { func: "random.int", size: "", args: &[("min", "0"), ("max", "9")] },
    Recipe { func: "random.choice", size: "xs", args: &[("xs", "{sized}")] },
    Recipe { func: "random.shuffle", size: "xs", args: &[("xs", "{sized}")] },
    Recipe { func: "fs.stat", size: "path", args: FILE_N },
    Recipe { func: "fs.create_temp_file", size: "", args: &[("prefix", "\"p\"")] },
    Recipe { func: "fs.create_temp_dir", size: "", args: &[("prefix", "\"p\"")] },
    Recipe { func: "env.sleep_ms", size: "", args: &[("ms", "0")] },
    // Processes from the fixed command set (ADR-0027 D4), on the embedded
    // host: n / 5 arguments echoed back.
    Recipe { func: "process.exec", size: "args", args: &[("cmd", ECHO), ("args", ECHO_ARGS_WIDE)] },
    // A fresh child per step to signal.
    Recipe {
        func: "process.kill",
        size: "",
        args: &[
            (
                "pid",
                "match process.start(process.ProcessCommand { cmd: \"/bin/sleep\", args: [\"${{i} * 0 + 5}\"] }) { ok(c) => process.child_pid(c) ?? 2147483000, err(_) => 2147483000 }",
            ),
            ("signal", "15"),
        ],
    },
    // Judged by the exit it makes (ADR-0027 D4): a code no clean end of a
    // program makes, inside C-350's 0..=125. The literal is what every
    // probe must exit with (`Target::expected_exit`).
    Recipe { func: "process.exit", size: "", args: &[("code", "3")] },
    // A ProcessCommand of n / 5 arguments to a child that prints nothing:
    // `run` attaches the child to this terminal.
    Recipe { func: "process.run", size: "c", args: &[("c", "process.ProcessCommand { cmd: \"/usr/bin/true\", args: list.range(0, {n} / 5) |> list.map((i) => \"aé\") }")] },
    // A child of its own per step, echoing n / 5 arguments: `wait` reaps it,
    // and what `read_new_*` / `poll` see of it is the child's own.
    Recipe { func: "process.wait", size: "c", args: &[("c", FRESH_ECHO_CHILD)] },
    Recipe { func: "process.read_new_stdout", size: "c", args: &[("c", FRESH_ECHO_CHILD)] },
    Recipe { func: "process.read_new_stderr", size: "c", args: &[("c", FRESH_ECHO_CHILD)] },
    Recipe { func: "process.poll", size: "c", args: &[("c", FRESH_ECHO_CHILD)] },
    // A running child of its own per step, ended at once (no grace).
    Recipe {
        func: "process.stop",
        size: "",
        args: &[
            ("c", "match process.start(process.ProcessCommand { cmd: \"/bin/sleep\", args: [\"${{i} * 0 + 5}\"] }) { ok(c) => c, err(e) => panic(e) }"),
            ("grace_ms", "0"),
        ],
    },
    // Directory capabilities (`fs.open_root`) over the sandbox: the same
    // files the plain fs readers and writers get, reached through a root.
    Recipe { func: "fs.open_root", size: "", args: &[("dir", "\".\"")] },
    Recipe { func: "fs.root_read_text", size: "path", args: &[("root", ROOT), ("path", file_of_n_lines!())] },
    Recipe { func: "fs.root_read_bytes", size: "path", args: &[("root", ROOT), ("path", file_of_n_lines!())] },
    Recipe { func: "fs.root_exists", size: "path", args: &[("root", ROOT), ("path", file_of_n_lines!())] },
    Recipe { func: "fs.root_list_dir", size: "path", args: &[("root", ROOT), ("path", dir_of_n_files!())] },
    Recipe { func: "fs.root_write_text", size: "content", args: &[("root", ROOT), ("path", "\"out.txt\""), ("content", "string.repeat(\"aé\\n\", {n})")] },
    Recipe { func: "fs.root_remove", size: "", args: &[("root", ROOT), ("path", fresh_file!("r"))] },
    // The standard streams. A reader reads the probe's stdin (`stdin_for`);
    // `read_to_end` takes the whole k-fold input at its first call (as
    // `read_all`), so its size is the input's, named in the handle's operand.
    Recipe { func: "io.read", size: "max", args: &[("h", STDIN), ("max", "{n}")] },
    Recipe { func: "io.read_exact", size: "n", args: &[("h", STDIN), ("n", "{n}")] },
    Recipe { func: "io.read_to_end", size: "h", args: &[("h", "{ let _input_line = {n}\n  io.stdin()! }")] },
    // Past the native 64 KiB stdout buffer at every size (as io.print's).
    Recipe { func: "io.write", size: "data", args: &[("h", "io.stdout()!"), ("data", "bytes.from_string(string.repeat(\"aé\", 4 * {n}))")] },
    // A stream of its own per step to the echo fixture.
    Recipe { func: "io.close", size: "", args: &[("h", "{ let _step = {i}\n  net.tcp_connect(\"127.0.0.1\", int.parse(env.get(\"XF_TCP_PORT\") ?? \"0\") ?? 0)! }")] },
    // TCP against the loopback fixtures (ADR-0027 D5): the echo, a source
    // that writes until the peer closes, and listeners of the probe's own.
    Recipe { func: "net.tcp_connect", size: "", args: &[("host", "\"127.0.0.1\""), ("port", TCP_PORT)] },
    Recipe { func: "net.tcp_listen", size: "", args: &[("host", "\"127.0.0.1\""), ("port", "0")] },
    Recipe {
        func: "net.tcp_accept",
        size: "",
        args: &[(
            "listener",
            "{ let tl = net.tcp_listen(\"127.0.0.1\", 0 * {i})!\n  let _tc: Result[TcpStream, String] = net.tcp_connect(\"127.0.0.1\", net.tcp_local_port(tl)!)\n  tl }",
        )],
    },
    Recipe { func: "net.tcp_is_open", size: "", args: &[("stream", ECHO_STREAM)] },
    Recipe { func: "net.tcp_set_timeout", size: "", args: &[("stream", ECHO_STREAM), ("timeout_ms", "1000")] },
    Recipe { func: "net.tcp_available", size: "", args: &[("stream", ECHO_STREAM)] },
    Recipe { func: "net.tcp_local_port", size: "", args: &[("listener", "net.tcp_listen(\"127.0.0.1\", 0)!")] },
    Recipe {
        func: "net.tcp_read_timeout",
        size: "len",
        args: &[("stream", "net.tcp_connect(\"127.0.0.1\", int.parse(env.get(\"XF_SOURCE_PORT\") ?? \"0\") ?? 0)!"), ("len", "{n}"), ("timeout_ms", "5000")],
    },
    // UDP on loopback: a socket of the probe's own, sending to itself (a
    // datagram of n / 8 bytes stays under the 64 KiB datagram limit at every
    // measured size); a receive takes a datagram its own socket sent.
    Recipe { func: "net.udp_bind", size: "", args: &[("host", "\"127.0.0.1\""), ("port", "0")] },
    Recipe { func: "net.udp_local_port", size: "", args: &[("sock", "net.udp_bind(\"127.0.0.1\", 0)!")] },
    Recipe {
        func: "net.udp_send_to",
        size: "data",
        args: &[("sock", "net.udp_bind(\"127.0.0.1\", 0)!"), ("data", "bytes.zeros({n} / 8)"), ("host", "\"127.0.0.1\""), ("port", TCP_PORT)],
    },
    Recipe {
        func: "net.udp_recv_from",
        size: "max",
        args: &[
            (
                "sock",
                "{ let us = net.udp_bind(\"127.0.0.1\", 0 * {i})!\n  net.udp_send_to(us, bytes.zeros({n} / 8), \"127.0.0.1\", net.udp_local_port(us)!)!\n  us }",
            ),
            ("max", "{n}"),
        ],
    },
    Recipe { func: "net.lookup_host", size: "", args: &[("host", "\"127.0.0.1\"")] },
    // unix's sockets over the fixture's sink and sockets of the probe's own,
    // and its shared-memory files (#3620). A call that consumes what it is
    // handed gets a fresh one every step.
    Recipe { func: "unix.connect", size: "", args: &[("path", UNIX_SINK)] },
    Recipe { func: "unix.listen", size: "", args: &[("path", "\"l${{i}}.sock\"")] },
    Recipe {
        func: "unix.accept",
        size: "",
        args: &[("listener", "{ let ul = unix.listen(\"a${{i}}.sock\")!\n  let _uc: Result[Fd, String] = unix.connect(\"a${{i}}.sock\")\n  ul }")],
    },
    Recipe {
        func: "unix.write_fds",
        size: "data",
        args: &[("sock", concat!("unix.connect(", "(env.get(\"XF_UNIX_SINK\") ?? \"\")", ")!")), ("data", SIZED_BYTES), ("fds", "[]")],
    },
    Recipe {
        func: "unix.take_fds",
        size: "",
        args: &[(
            "sock",
            "{ let ul = unix.listen(\"t${{i}}.sock\")!\n  let uc = unix.connect(\"t${{i}}.sock\")!\n  let us = unix.accept(ul)!\n  unix.write_fds(uc, bytes.from_string(\"hello world\"), [unix.shm_create(8)!])!\n  io.close(uc)!\n  io.close(ul)!\n  us }",
        )],
    },
    Recipe {
        func: "unix.select",
        size: "",
        args: &[(
            "xs",
            "{ let ul = unix.listen(\"s.sock\")!\n  let uc = unix.connect(\"s.sock\")!\n  let us = unix.accept(ul)!\n  unix.write_fds(uc, bytes.from_string(\"hello world\"), [])!\n  [us] }",
        ), ("timeout_ms", "1000")],
    },
    Recipe { func: "unix.shm_create", size: "size", args: &[("size", "{n}")] },
    Recipe { func: "unix.shm_resize", size: "size", args: &[("fd", "unix.shm_create(8)!"), ("size", "{n}")] },
    Recipe { func: "unix.shm_write", size: "data", args: &[("fd", "unix.shm_create({n})!"), ("offset", "0"), ("data", SIZED_BYTES)] },
    Recipe {
        func: "unix.shm_read",
        size: "len",
        args: &[("fd", "{ let sf = unix.shm_create({n})!\n  unix.shm_write(sf, 0, bytes.from_list(list.range(0, {n}) |> list.map((i) => i % 251)))!\n  sf }"), ("offset", "0"), ("len", "{n}")],
    },
    // HTTP against the fixture: a response of n bytes, or n bytes echoed.
    Recipe { func: "http.get", size: "url", args: &[("url", BYTES_URL)] },
    Recipe { func: "http.get_response", size: "url", args: &[("url", BYTES_URL)] },
    Recipe { func: "http.request", size: "body", args: &[("method", "\"POST\""), ("url", ECHO_URL), ("body", BODY), ("headers", "[(\"x-k\", \"v\")]")] },
    Recipe { func: "http.send", size: "req", args: &[("req", ECHO_REQUEST)] },
    // The call handle (#2631): start's cost is the exchange it begins,
    // which the probe's digest waits out (the echo of n bytes); poll answers
    // a call that has ended (its recipe waits first) with a copy of the n
    // bytes; read_new's first read of an ended call is its whole body, so
    // every step reads a fresh one; cancel ends a fresh call of its own.
    Recipe { func: "http.start", size: "req", args: &[("req", ECHO_REQUEST), ("limits", NO_LIMITS)] },
    Recipe { func: "http.poll", size: "c", args: &[("c", ENDED_CALL)] },
    Recipe { func: "http.read_new", size: "c", args: &[("c", FRESH_ENDED_CALL)] },
    Recipe { func: "http.wait", size: "c", args: &[("c", CALL)] },
    Recipe { func: "http.cancel", size: "", args: &[("c", FRESH_CALL)] },
];

/// Statements a probe's `main` runs first, for operands that must outlive
/// their own `let`. None today: the raw-address recipes that bound a live
/// buffer here left with the address surface (ADR-0031); the hook stays for
/// the next operand that needs one.
pub(super) fn prelude_for(_func: &str, _n: u64) -> Option<String> {
    None
}

/// What a stdin reader's probe gets on stdin: for size n and k calls, the
/// line length and the line count of an `a…a\n` stream with exactly what k
/// calls consume (`read_all` takes the whole k-fold input at its first call:
/// the total is still k·n, so the marginal over k reads n per call).
pub(super) fn stdin_for(func: &str, n: u64, k: u64) -> Option<(u64, u64)> {
    match func {
        "io.read_line" | "io.read_all" | "io.read" | "io.read_exact" | "io.read_to_end" => {
            Some((n, k))
        }
        _ => None,
    }
}

/// A call that holds a socket (or a short-lived port) per call stops at this
/// k: a probe runs 2k calls, and an fd per call past the soft limit (1024)
/// or a TIME_WAIT per call past the ephemeral range is an err, not a cost.
/// `effect` is the declaration's: a plain `fn` does no I/O per call, so a
/// module-wide cap below never applies to it.
pub fn max_k(func: &str, effect: bool) -> Option<u64> {
    match func.split_once('.') {
        Some(("net", f))
            if matches!(
                f,
                "tcp_connect" | "tcp_listen" | "tcp_accept" | "tcp_close" | "tcp_close_listener"
            ) =>
        {
            Some(64)
        }
        Some(("net", f))
            if matches!(f, "tcp_read_timeout" | "udp_bind" | "udp_local_port" | "udp_send_to" | "udp_recv_from") =>
        {
            Some(64)
        }
        // A descriptor (or three) held per call (#3620).
        Some(("unix", f))
            if matches!(
                f,
                "connect" | "listen" | "accept" | "take_fds" | "shm_create" | "shm_resize" | "shm_write" | "select"
            ) =>
        {
            Some(64)
        }
        // A stream (or a child) per call.
        Some(("io", "close")) => Some(64),
        Some(("process", f))
            if matches!(f, "start" | "wait" | "poll" | "stop" | "read_new_stdout" | "read_new_stderr" | "child_pid") =>
        {
            Some(64)
        }
        Some(("fs", "open_root")) => Some(64),
        // n bytes landed in the guest per call: the canonical allocator
        // never reuses a block, so 2k calls at 16n grow the heap to gigabytes
        // and the host's memory growth enters the count (shm_read at k=4096
        // read x2.24 at 4n, rms 8%; 256 keeps 2k calls at 16n near 160 MB,
        // where 64 left the fixed per-call cost visible at n: rms 3.2%), and a
        // send of 16n bytes per call at a
        // calibrated k of 2^20 overran the 60 s cap.
        Some(("unix", "shm_read")) => Some(256),
        Some(("unix", "write_fds")) => Some(256),
        // A plain accessor (http.body, req_method) reads a value in hand: at
        // k=256 its ~400 instructions per call sank under the embedded leg's
        // run-to-run jitter and the marginal read 0 at three of five sizes.
        Some(("http", _)) if effect => Some(256),
        // Each append grows the one file: 2k calls of n bytes at 16n.
        Some(("fs", "append_text" | "append_bytes")) => Some(256),
        _ => None,
    }
}

/// The sandbox (the probe's cwd) as a directory capability.
const ROOT: &str = "fs.open_root(\".\")!";
const STDIN: &str = "io.stdin()!";
const TCP_PORT: &str = "int.parse(env.get(\"XF_TCP_PORT\") ?? \"0\") ?? 0";
const UNIX_SINK: &str = "env.get(\"XF_UNIX_SINK\") ?? \"\"";
/// A stream to the echo fixture with "hello world" on its way back.
const ECHO_STREAM: &str = "{ let ts = net.tcp_connect(\"127.0.0.1\", int.parse(env.get(\"XF_TCP_PORT\") ?? \"0\") ?? 0)!\n  io.write(ts, bytes.from_string(\"hello world\"))!\n  ts }";
/// A child of the step's own echoing n / 5 arguments.
const FRESH_ECHO_CHILD: &str = "match process.start(process.ProcessCommand { cmd: \"/bin/echo\", args: list.range(0, {n} / 5 + {i} * 0) |> list.map((i) => \"aé\") }) { ok(c) => c, err(e) => panic(e) }";

const SIZED_BYTES: &str ="bytes.from_list(list.range(0, {n}) |> list.map((i) => i % 251))";
const BYTES_URL: &str = "\"http://\" + (env.get(\"XF_HTTP\") ?? \"\") + \"/bytes/{n}\"";
const ECHO_URL: &str = "\"http://\" + (env.get(\"XF_HTTP\") ?? \"\") + \"/echo\"";
const BODY: &str = "string.repeat(\"a\", {n})";
/// A POST of n bytes to the fixture's echo, with one header.
const ECHO_REQUEST: &str = "http.new_request(\"POST\", \"http://\" + (env.get(\"XF_HTTP\") ?? \"\") + \"/echo\", string.repeat(\"a\", {n}), [(\"x-k\", \"v\")])";
const NO_LIMITS: &str = "{ total_ms: 0, idle_ms: 0 }";

/// `http.start(http.new_request("GET", <fixture>path, "", headers))` with no
/// limits, an `err` stopping the probe (the fixture is always up).
macro_rules! call_of {
    ($path:literal, $headers:literal) => {
        concat!(
            "match http.start(http.new_request(\"GET\", \"http://\" + (env.get(\"XF_HTTP\") ?? \"\") + \"",
            $path,
            "\", \"\", ",
            $headers,
            "), { total_ms: 0, idle_ms: 0 }) { ok(hc) => hc, err(he) => panic(he) }"
        )
    };
}

/// A call answering n bytes, in flight.
const CALL: &str = call_of!("/bytes/{n}", "[]");
/// A call answering n bytes, ended before the probe reads it.
const ENDED_CALL: &str = concat!(
    "{ let hc = ",
    call_of!("/bytes/{n}", "[]"),
    "\n  let _w: Result[HttpResponse, String] = http.wait(hc)\n  hc }"
);
/// [`ENDED_CALL`], a new one every step (`{i}` names the step).
const FRESH_ENDED_CALL: &str = concat!(
    "{ let hc = ",
    call_of!("/bytes/{n}", "[(\"x-step\", \"${{i}}\")]"),
    "\n  let _w: Result[HttpResponse, String] = http.wait(hc)\n  hc }"
);
/// A call answering 16 bytes, a new one every step.
const FRESH_CALL: &str = call_of!("/bytes/16", "[(\"x-step\", \"${{i}}\")]");

const ECHO: &str = "\"/bin/echo\"";
/// n / 5 arguments: a captured run's pipe reads vary with the fork, and the
/// call's own work must stand above that noise (the rms bound is 3%). Not
/// n / 10: native `Command` holds one small CString per argument, and once
/// its argument Vec reaches 64 KiB (4096 arguments) freeing it crosses
/// glibc's fastbin-consolidation threshold, which then walks every freed
/// CString on each call — per argument 670-720 instructions up to 3000
/// arguments, 765-800 from 4000 (GCE, native process.exec, k = 256..512), so
/// n / 10 (2000..32000) straddled the step and fit no class (rms 6.0%); with
/// fastbins off (GLIBC_TUNABLES=glibc.malloc.mxfast=0) the step is gone. From
/// 4000 arguments every size is in the one allocator regime (fit n, 0.9%).
const ECHO_ARGS_WIDE: &str = "list.range(0, {n} / 5) |> list.map((i) => \"aé\")";
