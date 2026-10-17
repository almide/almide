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
        "{ fs.write(\"in.txt\", string.repeat(\"aé\\n\", {n}))!\n  \"in.txt\" }"
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
        "{ fs.mkdir_p(\"d\")!\n  for j in list.range(0, {n} / 10 + 3) { fs.write(\"d/f${100000 + j}\", \"x\")! }\n  \"d\" }"
    };
}

/// A fresh one-byte file per step.
macro_rules! fresh_file {
    ($stem:literal) => {
        concat!("{ fs.write(\"", $stem, "${{i}}.txt\", \"x\")!\n  \"", $stem, "${{i}}.txt\" }")
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
    Recipe { func: "fs.read_bytes_raw", size: "path", args: FILE_N },
    Recipe { func: "fs.read_bytes_raw_if_exists", size: "path", args: FILE_N },
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
    Recipe { func: "fs.write", size: "content", args: &[("path", "\"out.txt\""), ("content", "string.repeat(\"aé\\n\", {n})")] },
    // A whole number of the stock service's 64 KiB write slices at every
    // measured size (65536 bytes at n = 20000): one fd_write per slice, so
    // 4n bytes cost a + b·⌈4n / 65536⌉ — 2, 3, 5, 10, 20 host calls — and that
    // staircase read nlogn (GCE: 5.185e6 / 6.576e6 / 9.355e6 / 1.631e7 /
    // 3.021e7, exactly 9.5e3 + 5.4e3 per slice per call). Aligned, the series
    // is the per-slice (per-byte) cost.
    Recipe { func: "fs.append", size: "content", args: &[("path", "\"out.txt\""), ("content", "string.repeat(\"aé\\n\", {n} * 4096 / 5000)")] },
    Recipe { func: "fs.write_bytes", size: "bytes", args: &[("path", "\"out.bin\""), ("bytes", "list.range(0, {n}) |> list.map((i) => i % 256)")] },
    Recipe {
        func: "fs.write_bytes_raw",
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
        args: &[("path", "{ fs.mkdir_p(\"t${{i}}/a\")!\n  fs.write(\"t${{i}}/a/f\", \"x\")!\n  \"t${{i}}\" }")],
    },
    // argv and env (oracle/sandbox.rs ARGS / ENV).
    Recipe { func: "args.flag", size: "", args: &[("name", "\"flag\"")] },
    Recipe { func: "args.option", size: "", args: &[("name", "\"opt\"")] },
    Recipe { func: "args.option_or", size: "", args: &[("name", "\"opt\""), ("fallback", "\"none\"")] },
    Recipe { func: "args.positional_at", size: "", args: &[("i", "0")] },
    Recipe { func: "env.get", size: "", args: &[("name", "\"XF_GREETING\"")] },
    Recipe { func: "env.set", size: "value", args: &[("name", "\"XF_SET\""), ("value", "string.repeat(\"aé\", {n})")] },
    // Past the native 64 KiB stdout buffer at every size, so every size
    // writes in one regime (as io.write's recipe).
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
    Recipe { func: "process.exec_status", size: "args", args: &[("cmd", ECHO), ("args", ECHO_ARGS_WIDE)] },
    Recipe { func: "process.exec_status_timeout", size: "args", args: &[("cmd", ECHO), ("args", ECHO_ARGS_WIDE), ("timeout_ms", "10000")] },
    Recipe { func: "fs.create_temp_file", size: "", args: &[("prefix", "\"p\"")] },
    Recipe { func: "fs.create_temp_dir", size: "", args: &[("prefix", "\"p\"")] },
    Recipe { func: "env.sleep_ms", size: "", args: &[("ms", "0")] },
    // Processes from the fixed command set (ADR-0027 D4), on the embedded
    // host: n / 10 arguments echoed back, or n / 10 lines through cat.
    Recipe { func: "process.exec", size: "args", args: &[("cmd", ECHO), ("args", ECHO_ARGS_WIDE)] },
    Recipe { func: "process.exec_in", size: "args", args: &[("dir", MADE_DIR), ("cmd", ECHO), ("args", ECHO_ARGS_WIDE)] },
    Recipe { func: "process.exec_attached", size: "args", args: &[("cmd", ECHO), ("args", ECHO_ARGS)] },
    Recipe { func: "process.run", size: "args", args: &[("cmd", ECHO), ("args", ECHO_ARGS)] },
    Recipe { func: "process.run_in", size: "args", args: &[("dir", MADE_DIR), ("cmd", ECHO), ("args", ECHO_ARGS)] },
    Recipe {
        func: "process.exec_with_stdin",
        size: "input",
        args: &[("cmd", "\"/bin/cat\""), ("args", "[]"), ("input", "string.repeat(\"aé\\n\", {n} / 10)")],
    },
    Recipe { func: "process.spawn", size: "", args: &[("cmd", "\"/usr/bin/true\""), ("args", "[]")] },
    // A fresh child per step to signal.
    Recipe {
        func: "process.kill",
        size: "",
        args: &[("pid", "match process.spawn(\"/bin/sleep\", [\"${{i} * 0 + 5}\"]) { ok(p) => p, err(_) => 2147483000 }"), ("signal", "15")],
    },
    Recipe { func: "process.is_alive", size: "", args: &[("pid", "process.pid()")] },
    Recipe { func: "process.sleep", size: "", args: &[("ms", "0")] },
    // Judged by the exit it makes (ADR-0027 D4): a code no clean end of a
    // program makes, inside C-350's 0..=125. The literal is what every
    // probe must exit with (`Target::expected_exit`).
    Recipe { func: "process.exit", size: "", args: &[("code", "3")] },
    Recipe { func: "io.read_n_bytes", size: "n", args: &[("n", "{n}")] },
    // Host addresses over a live buffer: the probe binds it before the loop.
    Recipe { func: "bytes.as_ptr", size: "b", args: &[("b", SIZED_BYTES)] },
    Recipe { func: "bytes.as_mut_ptr", size: "b", args: &[("b", SIZED_BYTES)] },
    Recipe { func: "bytes.data_ptr", size: "b", args: &[("b", SIZED_BYTES)] },
    Recipe { func: "bytes.copy_to_ptr", size: "b", args: &[("b", SIZED_BYTES), ("ptr", "bytes.as_ptr(live_buf)"), ("cap", "{n}")] },
    Recipe { func: "bytes.from_raw_ptr", size: "len", args: &[("ptr", "bytes.as_ptr(live_buf)"), ("len", "{n}")] },
    // TCP against the loopback fixtures (ADR-0027 D5): a fresh socket per
    // step where the call consumes or holds one, the sink for writes and the
    // source for reads so neither side waits.
    Recipe { func: "net.tcp_connect", size: "", args: &[("host", LOCALHOST), ("port", ECHO_PORT)] },
    Recipe { func: "net.tcp_listen", size: "", args: &[("host", LOCALHOST), ("port", "0")] },
    Recipe { func: "net.tcp_accept", size: "", args: &[("listener", PENDING_LISTENER)] },
    Recipe { func: "net.tcp_close", size: "", args: &[("stream", FRESH_STREAM)] },
    Recipe { func: "net.tcp_close_listener", size: "", args: &[("listener", FRESH_LISTENER)] },
    Recipe { func: "net.tcp_local_port", size: "", args: &[("listener", LISTENER)] },
    Recipe { func: "net.tcp_is_open", size: "", args: &[("stream", SINK_STREAM)] },
    Recipe { func: "net.tcp_set_timeout", size: "", args: &[("stream", SINK_STREAM), ("timeout_ms", "1000")] },
    Recipe { func: "net.tcp_write", size: "data", args: &[("stream", SINK_STREAM), ("data", "bytes.from_list(list.range(0, {n}) |> list.map((i) => i % 251))")] },
    // How much of the source has arrived bounds what a read returns, and
    // that is the network's timing: a read no longer than one of the
    // source's writes always fills, so its cost is fixed.
    Recipe { func: "net.tcp_read", size: "", args: &[("stream", SOURCE_STREAM), ("len", "1024")] },
    Recipe { func: "net.tcp_read_exact", size: "len", args: &[("stream", SOURCE_STREAM), ("len", "{n}")] },
    Recipe { func: "net.tcp_read_timeout", size: "", args: &[("stream", SOURCE_STREAM), ("len", "1024"), ("timeout_ms", "1000")] },
    Recipe { func: "net.tcp_available", size: "", args: &[("stream", SOURCE_STREAM)] },
    // Unix sockets and shared-memory files (#3620): sockets in the sandbox
    // (short relative names, removed first: a diamond's legs run in one
    // project directory), the fixture's sink for sends. A fresh socket
    // per step where the call takes or holds one; a socket a read takes from
    // holds its bytes and its descriptor before the call, its peer closed.
    Recipe { func: "net.unix_connect", size: "", args: &[("path", UNIX_SINK)] },
    Recipe { func: "net.unix_listen", size: "", args: &[("path", "{ let _r: Result[Unit, String] = fs.remove(\"l${{i}}.sock\")\n  \"l${{i}}.sock\" }")] },
    Recipe { func: "net.unix_accept", size: "", args: &[("listener", UNIX_PENDING_LISTENER)] },
    Recipe { func: "net.unix_send", size: "data", args: &[("sock", UNIX_SINK_STREAM), ("data", SIZED_BYTES), ("fds", "[]")] },
    Recipe { func: "net.unix_recv", size: "", args: &[("sock", UNIX_PENDING_SOCKET), ("max", "64")] },
    Recipe { func: "net.unix_take_fds", size: "", args: &[("sock", UNIX_RECEIVED_SOCKET)] },
    Recipe { func: "net.unix_poll", size: "", args: &[("fd", UNIX_READY_SOCKET), ("timeout_ms", "0")] },
    Recipe { func: "net.unix_wait", size: "", args: &[("fds", "[net.unix_connect(env.get(\"XF_UNIX_SINK\") ?? \"\")!, net.shm_create(8)!]"), ("timeout_ms", "0")] },
    Recipe { func: "net.unix_close", size: "", args: &[("fd", "net.shm_create({i} * 0 + 8)!")] },
    Recipe { func: "net.shm_create", size: "", args: &[("size", "4096")] },
    Recipe { func: "net.shm_resize", size: "", args: &[("fd", "net.shm_create(64)!"), ("size", "4096 + {i} % 2")] },
    Recipe { func: "net.shm_write", size: "data", args: &[("fd", "net.shm_create(64)!"), ("offset", "0"), ("data", SIZED_BYTES)] },
    Recipe {
        func: "net.shm_read",
        size: "len",
        args: &[("fd", "{ let f = net.shm_create({n})!\n  net.shm_write(f, 0, bytes.from_list(list.range(0, {n}) |> list.map((i) => i % 251)))!\n  f }"), ("offset", "0"), ("len", "{n}")],
    },
    // HTTP against the fixture: a response of n bytes, or n bytes echoed.
    Recipe { func: "http.get", size: "url", args: &[("url", BYTES_URL)] },
    Recipe { func: "http.get_bytes", size: "url", args: &[("url", BYTES_URL)] },
    Recipe { func: "http.get_status", size: "url", args: &[("url", BYTES_URL)] },
    Recipe { func: "http.get_response", size: "url", args: &[("url", BYTES_URL)] },
    Recipe { func: "http.delete", size: "url", args: &[("url", BYTES_URL)] },
    Recipe { func: "http.delete_response", size: "url", args: &[("url", BYTES_URL)] },
    Recipe { func: "http.post", size: "body", args: &[("url", ECHO_URL), ("body", BODY)] },
    Recipe { func: "http.post_response", size: "body", args: &[("url", ECHO_URL), ("body", BODY)] },
    Recipe { func: "http.put", size: "body", args: &[("url", ECHO_URL), ("body", BODY)] },
    Recipe { func: "http.put_response", size: "body", args: &[("url", ECHO_URL), ("body", BODY)] },
    Recipe { func: "http.patch", size: "body", args: &[("url", ECHO_URL), ("body", BODY)] },
    Recipe { func: "http.patch_response", size: "body", args: &[("url", ECHO_URL), ("body", BODY)] },
    Recipe { func: "http.request", size: "body", args: &[("method", "\"POST\""), ("url", ECHO_URL), ("body", BODY), ("headers", "[\"x-k\": \"v\"]")] },
    Recipe { func: "http.request_bytes", size: "body", args: &[("method", "\"POST\""), ("url", ECHO_URL), ("body", BODY), ("headers", "[\"x-k\": \"v\"]")] },
    Recipe { func: "http.request_status", size: "body", args: &[("method", "\"POST\""), ("url", ECHO_URL), ("body", BODY), ("headers", "[\"x-k\": \"v\"]")] },
    Recipe { func: "http.request_response", size: "body", args: &[("method", "\"POST\""), ("url", ECHO_URL), ("body", BODY), ("headers", "[\"x-k\": \"v\"]")] },
    // The call handle (#2631): start's cost is the exchange it begins,
    // which the probe's digest waits out (the echo of n bytes); poll answers
    // a call that has ended (its recipe waits first) with a copy of the n
    // bytes; read_new's first read of an ended call is its whole body, so
    // every step reads a fresh one; cancel ends a fresh call of its own.
    Recipe {
        func: "http.start",
        size: "body",
        args: &[("method", "\"POST\""), ("url", ECHO_URL), ("body", BODY), ("headers", "[\"x-k\": \"v\"]"), ("limits", NO_LIMITS)],
    },
    Recipe { func: "http.poll", size: "c", args: &[("c", ENDED_CALL)] },
    Recipe { func: "http.read_new", size: "c", args: &[("c", FRESH_ENDED_CALL)] },
    Recipe { func: "http.wait", size: "c", args: &[("c", CALL)] },
    Recipe { func: "http.cancel", size: "", args: &[("c", FRESH_CALL)] },
    // The streaming clients over the fixture's `/chunked/{n}`: n chunks
    // `c<i>\n`, each delivered to the generated `on_chunk` — plain
    // request_stream (the call core's classic call) and its limited twin.
    Recipe {
        func: "http.request_stream",
        size: "url",
        args: &[("method", "\"GET\""), ("url", CHUNKED_URL), ("body", "\"\""), ("headers", "map.new()")],
    },
    Recipe {
        func: "http.request_stream_with_limits",
        size: "url",
        args: &[("method", "\"GET\""), ("url", CHUNKED_URL), ("body", "\"\""), ("headers", "map.new()"), ("limits", NO_LIMITS)],
    },
];

/// Statements a probe's `main` runs first, for operands that must outlive
/// their own `let`: an address is only valid while its buffer lives, so the
/// buffer is bound here and the address taken from it in the recipe.
pub(super) fn prelude_for(func: &str, n: u64) -> Option<String> {
    match func {
        "bytes.copy_to_ptr" | "bytes.from_raw_ptr" => {
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
        "io.read_line" | "io.read_line_opt" | "io.read_all" | "process.stdin_lines" => Some((n, k)),
        "io.read_byte" => Some((0, k)),
        "io.read_n_bytes" => Some((n.saturating_sub(1), k)),
        _ => None,
    }
}

/// A call that holds a socket (or a short-lived port) per call stops at this
/// k: a probe runs 2k calls, and an fd per call past the soft limit (1024)
/// or a TIME_WAIT per call past the ephemeral range is an err, not a cost.
pub fn max_k(func: &str) -> Option<u64> {
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
        Some(("http", _)) => Some(256),
        // Each append grows the one file: 2k calls of n bytes at 16n.
        Some(("fs", "append")) => Some(256),
        _ => None,
    }
}

const SIZED_BYTES: &str = "bytes.from_list(list.range(0, {n}) |> list.map((i) => i % 251))";
const LOCALHOST: &str = "\"127.0.0.1\"";
const ECHO_PORT: &str = "(int.parse(env.get(\"XF_TCP_PORT\") ?? \"0\") ?? 0)";
const SINK_STREAM: &str = "net.tcp_connect(\"127.0.0.1\", int.parse(env.get(\"XF_SINK_PORT\") ?? \"0\") ?? 0)!";
const SOURCE_STREAM: &str = "net.tcp_connect(\"127.0.0.1\", int.parse(env.get(\"XF_SOURCE_PORT\") ?? \"0\") ?? 0)!";
const FRESH_STREAM: &str = "net.tcp_connect(\"127.0.0.1\", int.parse(env.get(\"XF_SINK_PORT\") ?? \"${{i} * 0}\") ?? 0)!";
const LISTENER: &str = "net.tcp_listen(\"127.0.0.1\", 0)!";
const FRESH_LISTENER: &str = "net.tcp_listen(\"127.0.0.1\", {i} * 0)!";
const PENDING_LISTENER: &str = "{ let l = net.tcp_listen(\"127.0.0.1\", {i} * 0)!\n  let _c = net.tcp_connect(\"127.0.0.1\", net.tcp_local_port(l)!)!\n  l }";
const UNIX_SINK: &str = "(env.get(\"XF_UNIX_SINK\") ?? \"\")";
const UNIX_SINK_STREAM: &str = "net.unix_connect(env.get(\"XF_UNIX_SINK\") ?? \"\")!";
/// A listener with one connection pending, a fresh one per step.
const UNIX_PENDING_LISTENER: &str = "{ let _r: Result[Unit, String] = fs.remove(\"a${{i}}.sock\")\n  let l = net.unix_listen(\"a${{i}}.sock\")!\n  let _c = net.unix_connect(\"a${{i}}.sock\")!\n  l }";
/// A socket holding 11 bytes, its peer closed, a fresh one per step.
const UNIX_PENDING_SOCKET: &str = "{ let _r: Result[Unit, String] = fs.remove(\"r${{i}}.sock\")\n  let l = net.unix_listen(\"r${{i}}.sock\")!\n  let c = net.unix_connect(\"r${{i}}.sock\")!\n  let s = net.unix_accept(l)!\n  net.unix_send(c, bytes.from_string(\"hello world\"), [])!\n  net.unix_close(c)!\n  net.unix_close(l)!\n  s }";
/// A socket whose one descriptor has arrived and not been taken, a fresh one
/// per step.
const UNIX_RECEIVED_SOCKET: &str = "{ let _r: Result[Unit, String] = fs.remove(\"t${{i}}.sock\")\n  let l = net.unix_listen(\"t${{i}}.sock\")!\n  let c = net.unix_connect(\"t${{i}}.sock\")!\n  let s = net.unix_accept(l)!\n  let f = net.shm_create(8)!\n  net.unix_send(c, bytes.from_string(\"x\"), [f])!\n  let _got = net.unix_recv(s, 8)!\n  net.unix_close(c)!\n  net.unix_close(l)!\n  net.unix_close(f)!\n  s }";
/// A socket with bytes waiting, so a poll answers at once.
const UNIX_READY_SOCKET: &str = "{ let _r: Result[Unit, String] = fs.remove(\"p.sock\")\n  let l = net.unix_listen(\"p.sock\")!\n  let c = net.unix_connect(\"p.sock\")!\n  let s = net.unix_accept(l)!\n  net.unix_send(c, bytes.from_string(\"x\"), [])!\n  s }";
const BYTES_URL: &str = "\"http://\" + (env.get(\"XF_HTTP\") ?? \"\") + \"/bytes/{n}\"";
const CHUNKED_URL: &str = "\"http://\" + (env.get(\"XF_HTTP\") ?? \"\") + \"/chunked/{n}\"";
const ECHO_URL: &str ="\"http://\" + (env.get(\"XF_HTTP\") ?? \"\") + \"/echo\"";
const BODY: &str = "string.repeat(\"a\", {n})";
const NO_LIMITS: &str = "{ total_ms: 0, idle_ms: 0 }";

/// `http.start("GET", <fixture>path, "", headers)` with no limits, an `err`
/// stopping the probe (the fixture is always up).
macro_rules! call_of {
    ($path:literal, $headers:literal) => {
        concat!(
            "match http.start(\"GET\", \"http://\" + (env.get(\"XF_HTTP\") ?? \"\") + \"",
            $path,
            "\", \"\", ",
            $headers,
            ", { total_ms: 0, idle_ms: 0 }) { ok(hc) => hc, err(he) => panic(he) }"
        )
    };
}

/// A call answering n bytes, in flight.
const CALL: &str = call_of!("/bytes/{n}", "map.new()");
/// A call answering n bytes, ended before the probe reads it.
const ENDED_CALL: &str = concat!("{ let hc = ", call_of!("/bytes/{n}", "map.new()"), "\n  let _w: Result[HttpResponse, String] = http.wait(hc)\n  hc }");
/// [`ENDED_CALL`], a new one every step (`{i}` names the step).
const FRESH_ENDED_CALL: &str =
    concat!("{ let hc = ", call_of!("/bytes/{n}", "[\"x-step\": \"${{i}}\"]"), "\n  let _w: Result[HttpResponse, String] = http.wait(hc)\n  hc }");
/// A call answering 16 bytes, a new one every step.
const FRESH_CALL: &str = call_of!("/bytes/16", "[\"x-step\": \"${{i}}\"]");

/// A directory the probe makes, so it exists in a diamond's project too.
const MADE_DIR: &str = "{ fs.mkdir_p(\"pdir\")!\n  \"pdir\" }";

const ECHO: &str = "\"/bin/echo\"";
const ECHO_ARGS: &str = "list.range(0, {n} / 100) |> list.map((i) => \"aé\")";
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
