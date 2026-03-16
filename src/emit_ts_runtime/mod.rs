/// Runtime modules for Almide TypeScript/JavaScript code generation.
///
/// Each stdlib module (`__almd_*`) is loaded from `runtime/ts/` (Deno) and
/// `runtime/js/` (Node.js). The `full_runtime()` function composes them
/// into the monolithic runtime string used by `--target ts` and `--target js`.
/// Individual modules can be retrieved via `get_module_source()` for the
/// `--target npm` output.

// ──────────────────────────────── Module Sources ────────────────────────────────

// TS (Deno) modules
const MOD_STRING_TS: &str = include_str!("../../runtime/ts/string.ts");
const MOD_LIST_TS: &str = include_str!("../../runtime/ts/list.ts");
const MOD_MAP_TS: &str = include_str!("../../runtime/ts/map.ts");
const MOD_INT_TS: &str = include_str!("../../runtime/ts/int.ts");
const MOD_FLOAT_TS: &str = include_str!("../../runtime/ts/float.ts");
const MOD_MATH_TS: &str = include_str!("../../runtime/ts/math.ts");
const MOD_RANDOM_TS: &str = include_str!("../../runtime/ts/random.ts");
const MOD_JSON_TS: &str = include_str!("../../runtime/ts/json.ts");
const MOD_RESULT_TS: &str = include_str!("../../runtime/ts/result.ts");
const MOD_ERROR_TS: &str = include_str!("../../runtime/ts/error.ts");
const MOD_DATETIME_TS: &str = include_str!("../../runtime/ts/datetime.ts");
const MOD_TESTING_TS: &str = include_str!("../../runtime/ts/testing.ts");
const MOD_FS_TS: &str = include_str!("../../runtime/ts/fs.ts");
const MOD_IO_TS: &str = include_str!("../../runtime/ts/io.ts");
const MOD_PATH_TS: &str = include_str!("../../runtime/ts/path.ts");
const MOD_ENV_TS: &str = include_str!("../../runtime/ts/env.ts");
const MOD_PROCESS_TS: &str = include_str!("../../runtime/ts/process.ts");
const MOD_HTTP_TS: &str = include_str!("../../runtime/ts/http.ts");
const MOD_REGEX_TS: &str = include_str!("../../runtime/ts/regex.ts");
const MOD_TIME_TS: &str = include_str!("../../runtime/ts/time.ts");
const MOD_CRYPTO_TS: &str = include_str!("../../runtime/ts/crypto.ts");
const MOD_UUID_TS: &str = include_str!("../../runtime/ts/uuid.ts");
const MOD_LOG_TS: &str = include_str!("../../runtime/ts/log.ts");
const HELPERS_TS: &str = include_str!("../../runtime/ts/helpers.ts");

// JS (Node.js) modules
const MOD_STRING_JS: &str = include_str!("../../runtime/js/string.js");
const MOD_LIST_JS: &str = include_str!("../../runtime/js/list.js");
const MOD_MAP_JS: &str = include_str!("../../runtime/js/map.js");
const MOD_INT_JS: &str = include_str!("../../runtime/js/int.js");
const MOD_FLOAT_JS: &str = include_str!("../../runtime/js/float.js");
const MOD_MATH_JS: &str = include_str!("../../runtime/js/math.js");
const MOD_RANDOM_JS: &str = include_str!("../../runtime/js/random.js");
const MOD_JSON_JS: &str = include_str!("../../runtime/js/json.js");
const MOD_RESULT_JS: &str = include_str!("../../runtime/js/result.js");
const MOD_ERROR_JS: &str = include_str!("../../runtime/js/error.js");
const MOD_DATETIME_JS: &str = include_str!("../../runtime/js/datetime.js");
const MOD_TESTING_JS: &str = include_str!("../../runtime/js/testing.js");
const MOD_FS_JS: &str = include_str!("../../runtime/js/fs.js");
const MOD_IO_JS: &str = include_str!("../../runtime/js/io.js");
const MOD_PATH_JS: &str = include_str!("../../runtime/js/path.js");
const MOD_ENV_JS: &str = include_str!("../../runtime/js/env.js");
const MOD_PROCESS_JS: &str = include_str!("../../runtime/js/process.js");
const MOD_HTTP_JS: &str = include_str!("../../runtime/js/http.js");
const MOD_REGEX_JS: &str = include_str!("../../runtime/js/regex.js");
const MOD_TIME_JS: &str = include_str!("../../runtime/js/time.js");
const MOD_CRYPTO_JS: &str = include_str!("../../runtime/js/crypto.js");
const MOD_UUID_JS: &str = include_str!("../../runtime/js/uuid.js");
const MOD_LOG_JS: &str = include_str!("../../runtime/js/log.js");
const HELPERS_JS: &str = include_str!("../../runtime/js/helpers.js");

// ──────────────────────────────── Preambles ────────────────────────────────

const PREAMBLE_TS: &str = "// ---- Almide Runtime ----\n";
const PREAMBLE_JS: &str = "\
// ---- Almide Runtime (JS) ----
const __node_process = globalThis.process || {};
";

const EPILOGUE: &str = "// ---- End Runtime ----\n";

// ──────────────────────────────── Registry ────────────────────────────────

/// A runtime module with TS (Deno) and JS (Node.js) source variants.
pub struct RuntimeModule {
    pub name: &'static str,
    pub ts_source: &'static str,
    pub js_source: &'static str,
}

/// All stdlib runtime modules in emit order.
pub static ALL_MODULES: &[RuntimeModule] = &[
    RuntimeModule { name: "fs",      ts_source: MOD_FS_TS,      js_source: MOD_FS_JS },
    RuntimeModule { name: "string",  ts_source: MOD_STRING_TS,  js_source: MOD_STRING_JS },
    RuntimeModule { name: "list",    ts_source: MOD_LIST_TS,    js_source: MOD_LIST_JS },
    RuntimeModule { name: "map",     ts_source: MOD_MAP_TS,     js_source: MOD_MAP_JS },
    RuntimeModule { name: "int",     ts_source: MOD_INT_TS,     js_source: MOD_INT_JS },
    RuntimeModule { name: "float",   ts_source: MOD_FLOAT_TS,   js_source: MOD_FLOAT_JS },
    RuntimeModule { name: "path",    ts_source: MOD_PATH_TS,    js_source: MOD_PATH_JS },
    RuntimeModule { name: "json",    ts_source: MOD_JSON_TS,    js_source: MOD_JSON_JS },
    RuntimeModule { name: "env",     ts_source: MOD_ENV_TS,     js_source: MOD_ENV_JS },
    RuntimeModule { name: "process", ts_source: MOD_PROCESS_TS, js_source: MOD_PROCESS_JS },
    RuntimeModule { name: "math",    ts_source: MOD_MATH_TS,    js_source: MOD_MATH_JS },
    RuntimeModule { name: "random",  ts_source: MOD_RANDOM_TS,  js_source: MOD_RANDOM_JS },
    RuntimeModule { name: "regex",   ts_source: MOD_REGEX_TS,   js_source: MOD_REGEX_JS },
    RuntimeModule { name: "io",      ts_source: MOD_IO_TS,      js_source: MOD_IO_JS },
    RuntimeModule { name: "time",    ts_source: MOD_TIME_TS,    js_source: MOD_TIME_JS },
    RuntimeModule { name: "http",    ts_source: MOD_HTTP_TS,    js_source: MOD_HTTP_JS },
    RuntimeModule { name: "result",  ts_source: MOD_RESULT_TS,  js_source: MOD_RESULT_JS },
    RuntimeModule { name: "error",    ts_source: MOD_ERROR_TS,    js_source: MOD_ERROR_JS },
    RuntimeModule { name: "datetime", ts_source: MOD_DATETIME_TS, js_source: MOD_DATETIME_JS },
    RuntimeModule { name: "testing",  ts_source: MOD_TESTING_TS,  js_source: MOD_TESTING_JS },
    RuntimeModule { name: "crypto",   ts_source: MOD_CRYPTO_TS,   js_source: MOD_CRYPTO_JS },
    RuntimeModule { name: "uuid",     ts_source: MOD_UUID_TS,     js_source: MOD_UUID_JS },
    RuntimeModule { name: "log",      ts_source: MOD_LOG_TS,      js_source: MOD_LOG_JS },
];

/// Compose the full runtime string (backwards compatible with --target ts/js).
pub fn full_runtime(js_mode: bool) -> String {
    let mut out = String::with_capacity(if js_mode { 16384 } else { 20480 });
    out.push_str(if js_mode { PREAMBLE_JS } else { PREAMBLE_TS });
    for m in ALL_MODULES {
        out.push_str(if js_mode { m.js_source } else { m.ts_source });
    }
    out.push_str(if js_mode { HELPERS_JS } else { HELPERS_TS });
    out.push_str(EPILOGUE);
    out
}

/// Get the source for a single stdlib module.
pub fn get_module_source(name: &str, js_mode: bool) -> Option<&'static str> {
    ALL_MODULES.iter().find(|m| m.name == name).map(|m| {
        if js_mode { m.js_source } else { m.ts_source }
    })
}

/// Get the helpers source (always needed).
pub fn get_helpers_source(js_mode: bool) -> &'static str {
    if js_mode { HELPERS_JS } else { HELPERS_TS }
}

/// Get the preamble (platform-specific setup code).
pub fn get_preamble(js_mode: bool) -> &'static str {
    if js_mode { PREAMBLE_JS } else { PREAMBLE_TS }
}
