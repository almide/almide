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

/// `d/`, holding n / 100 + 3 one-byte files: every file is a host call to
/// create, and 16n of them (320000) took the run past its cap.
macro_rules! dir_of_n_files {
    () => {
        "{ fs.mkdir_p(\"d\")!\n  for j in list.range(0, {n} / 100 + 3) { fs.write(\"d/f${j}\", \"x\")! }\n  \"d\" }"
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
    Recipe { func: "fs.is_symlink", size: "", args: &[("path", "\"link.txt\"")] },
    // A directory of n files.
    Recipe { func: "fs.is_dir", size: "path", args: &[("path", dir_of_n_files!())] },
    Recipe { func: "fs.list_dir", size: "path", args: &[("path", dir_of_n_files!())] },
    Recipe { func: "fs.walk", size: "dir", args: &[("dir", dir_of_n_files!())] },
    Recipe { func: "fs.glob", size: "pattern", args: &[("pattern", concat!("{ let d = ", dir_of_n_files!(), "\n  d + \"/*\" }"))] },
    // Writers of n bytes of content.
    Recipe { func: "fs.write", size: "content", args: &[("path", "\"out.txt\""), ("content", "string.repeat(\"aé\\n\", {n})")] },
    Recipe { func: "fs.append", size: "content", args: &[("path", "\"out.txt\""), ("content", "string.repeat(\"aé\\n\", {n})")] },
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
    Recipe { func: "process.exec_status", size: "args", args: &[("cmd", ECHO), ("args", ECHO_ARGS)] },
    Recipe { func: "process.exec_status_timeout", size: "args", args: &[("cmd", ECHO), ("args", ECHO_ARGS), ("timeout_ms", "10000")] },
    Recipe { func: "fs.create_temp_file", size: "", args: &[("prefix", "\"p\"")] },
    Recipe { func: "fs.create_temp_dir", size: "", args: &[("prefix", "\"p\"")] },
    Recipe { func: "env.sleep_ms", size: "", args: &[("ms", "0")] },
    // Processes from the fixed command set (ADR-0027 D4), on the embedded
    // host: n / 10 arguments echoed back, or n / 10 lines through cat.
    Recipe { func: "process.exec", size: "args", args: &[("cmd", ECHO), ("args", ECHO_ARGS)] },
    Recipe { func: "process.exec_in", size: "args", args: &[("dir", "\"sub\""), ("cmd", ECHO), ("args", ECHO_ARGS)] },
    Recipe { func: "process.exec_attached", size: "args", args: &[("cmd", ECHO), ("args", ECHO_ARGS)] },
    Recipe { func: "process.run", size: "args", args: &[("cmd", ECHO), ("args", ECHO_ARGS)] },
    Recipe { func: "process.run_in", size: "args", args: &[("dir", "\"sub\""), ("cmd", ECHO), ("args", ECHO_ARGS)] },
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
    Recipe { func: "io.read_n_bytes", size: "n", args: &[("n", "{n}")] },
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
    Recipe { func: "net.tcp_read", size: "len", args: &[("stream", SOURCE_STREAM), ("len", "{n}")] },
    Recipe { func: "net.tcp_read_exact", size: "len", args: &[("stream", SOURCE_STREAM), ("len", "{n}")] },
    Recipe { func: "net.tcp_read_timeout", size: "len", args: &[("stream", SOURCE_STREAM), ("len", "{n}"), ("timeout_ms", "1000")] },
    Recipe { func: "net.tcp_available", size: "", args: &[("stream", SOURCE_STREAM)] },
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
];

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
        Some(("http", _)) => Some(256),
        _ => None,
    }
}

const LOCALHOST: &str = "\"127.0.0.1\"";
const ECHO_PORT: &str = "(int.parse(env.get(\"XF_TCP_PORT\") ?? \"0\") ?? 0)";
const SINK_STREAM: &str = "net.tcp_connect(\"127.0.0.1\", int.parse(env.get(\"XF_SINK_PORT\") ?? \"0\") ?? 0)!";
const SOURCE_STREAM: &str = "net.tcp_connect(\"127.0.0.1\", int.parse(env.get(\"XF_SOURCE_PORT\") ?? \"0\") ?? 0)!";
const FRESH_STREAM: &str = "net.tcp_connect(\"127.0.0.1\", int.parse(env.get(\"XF_SINK_PORT\") ?? \"${{i} * 0}\") ?? 0)!";
const LISTENER: &str = "net.tcp_listen(\"127.0.0.1\", 0)!";
const FRESH_LISTENER: &str = "net.tcp_listen(\"127.0.0.1\", {i} * 0)!";
const PENDING_LISTENER: &str = "{ let l = net.tcp_listen(\"127.0.0.1\", {i} * 0)!\n  let _c = net.tcp_connect(\"127.0.0.1\", net.tcp_local_port(l)!)!\n  l }";
const BYTES_URL: &str = "\"http://\" + (env.get(\"XF_HTTP\") ?? \"\") + \"/bytes/{n}\"";
const ECHO_URL: &str = "\"http://\" + (env.get(\"XF_HTTP\") ?? \"\") + \"/echo\"";
const BODY: &str = "string.repeat(\"a\", {n})";

const ECHO: &str = "\"/bin/echo\"";
const ECHO_ARGS: &str = "list.range(0, {n} / 10) |> list.map((i) => \"aé\")";
