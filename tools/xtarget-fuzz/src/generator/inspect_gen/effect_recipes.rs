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
        concat!("{ fs.write_text(\"", $stem, "${{i}}.txt\", \"x\")!\n  \"", $stem, "${{i}}.txt\" }")
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
    Recipe { func: "fs.file_size", size: "path", args: FILE_N },
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
    Recipe { func: "args.flag", size: "", args: &[("name", "\"flag\"")] },
    Recipe { func: "args.option", size: "", args: &[("name", "\"opt\"")] },
    Recipe { func: "args.option_or", size: "", args: &[("name", "\"opt\""), ("fallback", "\"none\"")] },
    Recipe { func: "args.positional_at", size: "", args: &[("i", "0"), ("options", "[]")] },
    Recipe { func: "env.get", size: "", args: &[("name", "\"XF_GREETING\"")] },
    Recipe { func: "env.set", size: "value", args: &[("name", "\"XF_SET\""), ("value", "string.repeat(\"aé\", {n})")] },
    // Past the native 64 KiB stdout buffer at every size, so every size
    // writes in one regime (as io.write's recipe did).
    Recipe { func: "io.print", size: "s", args: &[("s", "string.repeat(\"aé\", 4 * {n})")] },
    Recipe { func: "process.env", size: "", args: &[("key", "\"XF_GREETING\"")] },
    // Host-decided results (`verdicts.rs`): each probe checks the declared
    // properties instead of reading the value.
    Recipe { func: "random.int", size: "", args: &[("min", "0"), ("max", "9")] },
    Recipe { func: "random.choice", size: "xs", args: &[("xs", "{sized}")] },
    Recipe { func: "random.shuffle", size: "xs", args: &[("xs", "{sized}")] },
    Recipe { func: "list.shuffle", size: "xs", args: &[("xs", "{sized}")] },
    Recipe { func: "fs.modified_at", size: "path", args: FILE_N },
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
    Recipe { func: "process.sleep", size: "", args: &[("ms", "0")] },
    // Judged by the exit it makes (ADR-0027 D4): a code no clean end of a
    // program makes, inside C-350's 0..=125. The literal is what every
    // probe must exit with (`Target::expected_exit`).
    Recipe { func: "process.exit", size: "", args: &[("code", "3")] },
    // Host addresses over a live buffer: the probe binds it before the loop.
    Recipe { func: "bytes.as_ptr", size: "b", args: &[("b", SIZED_BYTES)] },
    Recipe { func: "bytes.as_mut_ptr", size: "b", args: &[("b", SIZED_BYTES)] },
    Recipe { func: "bytes.data_ptr", size: "b", args: &[("b", SIZED_BYTES)] },
    Recipe { func: "bytes.copy_to_ptr", size: "b", args: &[("b", SIZED_BYTES), ("ptr", "bytes.as_ptr(live_buf)"), ("cap", "{n}")] },
    Recipe { func: "bytes.from_raw_ptr", size: "len", args: &[("ptr", "bytes.as_ptr(live_buf)"), ("len", "{n}")] },
    Recipe { func: "ffi.address", size: "b", args: &[("b", SIZED_BYTES)] },
    Recipe { func: "ffi.to_int", size: "", args: &[("p", "ffi.address(live_buf)")] },
    Recipe { func: "ffi.read", size: "len", args: &[("ptr", "ffi.address(live_buf)"), ("len", "{n}")] },
    Recipe { func: "ffi.write", size: "b", args: &[("ptr", "ffi.address(live_buf)"), ("b", SIZED_BYTES), ("cap", "{n}")] },
    // net's TCP calls and unix's sockets and shared-memory files take and
    // return opaque handles (`net.TcpStream`, `unix.Fd`), and the stream
    // operations on them are io's generic `read` / `write` / `close`: neither
    // is in the generator's type universe yet, so none is catalogued here.
    // HTTP against the fixture: a response of n bytes, or n bytes echoed.
    Recipe { func: "http.get", size: "url", args: &[("url", BYTES_URL)] },
    Recipe { func: "http.get_bytes", size: "url", args: &[("url", BYTES_URL)] },
    Recipe { func: "http.get_response", size: "url", args: &[("url", BYTES_URL)] },
    Recipe { func: "http.request", size: "body", args: &[("method", "\"POST\""), ("url", ECHO_URL), ("body", BODY), ("headers", "[(\"x-k\", \"v\")]")] },
    Recipe { func: "http.send", size: "req", args: &[("req", ECHO_REQUEST)] },
    Recipe { func: "http.request_bytes", size: "body", args: &[("method", "\"POST\""), ("url", ECHO_URL), ("body", BODY), ("headers", "[(\"x-k\", \"v\")]")] },
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
/// their own `let`: an address is only valid while its buffer lives, so the
/// buffer is bound here and the address taken from it in the recipe.
pub(super) fn prelude_for(func: &str, n: u64) -> Option<String> {
    match func {
        "bytes.copy_to_ptr" | "bytes.from_raw_ptr" | "ffi.to_int" | "ffi.read" | "ffi.write" => {
            Some(format!("  let live_buf: Bytes = bytes.from_list(list.range(0, {n}) |> list.map((i) => i % 251))\n"))
        }
        _ => None,
    }
}

/// What a stdin reader's probe gets on stdin: for size n and k calls, the
/// line length and the line count of an `a…a\n` stream with exactly what k
/// calls consume (`read_all` takes the whole k-fold input at its first call:
/// the total is still k·n, so the marginal over k reads n per call).
pub(super) fn stdin_for(func: &str, n: u64, k: u64) -> Option<(u64, u64)> {
    match func {
        "io.read_line" | "io.read_all" | "process.stdin_lines" => Some((n, k)),
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
        Some(("net", f)) if matches!(f, "tcp_connect" | "tcp_listen" | "tcp_accept" | "tcp_close" | "tcp_close_listener") => Some(64),
        // A descriptor (or three) held per call (#3620).
        Some(("net", f))
            if matches!(f, "unix_connect" | "unix_listen" | "unix_accept" | "unix_recv" | "unix_take_fds" | "unix_close" | "shm_create") =>
        {
            Some(64)
        }
        // n bytes landed in the guest per call: the canonical allocator
        // never reuses a block, so 2k calls at 16n grow the heap to gigabytes
        // and the host's memory growth enters the count (shm_read at k=4096
        // read x2.24 at 4n, rms 8%; 256 keeps 2k calls at 16n near 160 MB,
        // where 64 left the fixed per-call cost visible at n: rms 3.2%), and a
        // send of 16n bytes per call at a
        // calibrated k of 2^20 overran the 60 s cap.
        Some(("net", "shm_read")) => Some(256),
        Some(("net", "unix_send")) => Some(256),
        // A plain accessor (http.body, req_method) reads a value in hand: at
        // k=256 its ~400 instructions per call sank under the embedded leg's
        // run-to-run jitter and the marginal read 0 at three of five sizes.
        Some(("http", _)) if effect => Some(256),
        // Each append grows the one file: 2k calls of n bytes at 16n.
        Some(("fs", "append_text" | "append_bytes")) => Some(256),
        _ => None,
    }
}

const SIZED_BYTES: &str = "bytes.from_list(list.range(0, {n}) |> list.map((i) => i % 251))";
const BYTES_URL: &str = "\"http://\" + (env.get(\"XF_HTTP\") ?? \"\") + \"/bytes/{n}\"";
const ECHO_URL: &str ="\"http://\" + (env.get(\"XF_HTTP\") ?? \"\") + \"/echo\"";
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
const ENDED_CALL: &str = concat!("{ let hc = ", call_of!("/bytes/{n}", "[]"), "\n  let _w: Result[HttpResponse, String] = http.wait(hc)\n  hc }");
/// [`ENDED_CALL`], a new one every step (`{i}` names the step).
const FRESH_ENDED_CALL: &str =
    concat!("{ let hc = ", call_of!("/bytes/{n}", "[(\"x-step\", \"${{i}}\")]"), "\n  let _w: Result[HttpResponse, String] = http.wait(hc)\n  hc }");
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
