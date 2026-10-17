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
];

const ECHO: &str = "\"/bin/echo\"";
const ECHO_ARGS: &str = "list.range(0, {n} / 10) |> list.map((i) => \"aé\")";
