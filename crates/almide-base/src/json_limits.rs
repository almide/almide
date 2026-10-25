// json_limits (#4020 / #4209, R-json-04, R-L0-47): the JSON value domain's
// nesting bound — the ONE declaration every leg reads. The native runtime
// splices this file into runtime/rs/src/json.rs (`include!`), the
// structural wasm leg reads it as `almide_base::json_limits`, and the
// self-hosted stdlib/json_parse.almd + stdlib/json_path.almd bodies (the
// wasm parse and the interp oracle) carry it as a literal that
// tests/json_depth_limit_test.rs pins to this constant.
//
// `json.parse` refuses an array or object opened inside JSON_MAX_DEPTH
// others, and `json.set_path` refuses to build a value nested deeper than
// that (its path's steps plus the new value's own nesting), both with the
// one text `json_depth_error()` names, so every value the JSON functions
// build stays parseable and every recursive walk stays bounded.

/// The deepest container nesting a JSON value may have.
pub const JSON_MAX_DEPTH: usize = 1000;

/// The err text for a value nested past [`JSON_MAX_DEPTH`].
pub fn json_depth_error() -> String {
    format!("json: nesting deeper than {}", JSON_MAX_DEPTH)
}
