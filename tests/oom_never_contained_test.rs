// OUT OF MEMORY IS NEVER CONTAINED (row 1 of the security table, C-434)
// ======================================================================
//
// #4203 made a runtime abort inside an `http.serve` handler containable:
// `almide_abort` unwinds while ALMIDE_CONTAIN is set and the serve loop
// answers the fixed 500. An allocation that fails part-way through a handler
// leaves what it was building half made, so it must end the run instead.
// Natively that is a call path: every out-of-memory site calls
// `almide_abort_oom`, which never unwinds. On the embedded lane it is the
// store limiter's refusal (`Contain::oom` in crates/almide-wasm-run/src/host.rs).
//
// tests/http_serve_cross_test.rs
// `out_of_memory_in_the_handler_ends_the_run_on_native_and_the_embedded_lane`
// shows the observable side on both legs (release-only, Linux). These checks
// pin the call path in every build, so a new out-of-memory site written as
// `almide_abort("out of memory")` fails here.
//
// Negative control: put one native site back on `almide_abort("out of
// memory")`, or have `almide_abort_oom` call `almide_contained_unwind`, and
// the matching test below fails.

use std::path::{Path, PathBuf};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn files_under(rel: &str, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root().join(rel)];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
            let p = e.expect("dir entry").path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == ext) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Every place that emits or embeds native runtime code, the generated
/// runtime registry included.
fn native_sources() -> Vec<PathBuf> {
    let mut v = files_under("runtime/rs/src", "rs");
    v.extend(files_under("crates/almide-rt-core/src", "rs"));
    v.extend(files_under("crates/almide-codegen/src", "rs"));
    v.extend(files_under("crates/almide-codegen/templates", "toml"));
    v
}

#[test]
fn no_out_of_memory_site_goes_through_the_containable_abort() {
    // The plain spelling, and the escaped one inside a string literal (the
    // walker's format! strings, the templates, the generated registry).
    let forms = ["almide_abort(\"out of memory\")", "almide_abort(\\\"out of memory\\\")"];
    let mut hits = Vec::new();
    for p in native_sources() {
        let text = std::fs::read_to_string(&p).expect("read source");
        for (i, line) in text.lines().enumerate() {
            if forms.iter().any(|f| line.contains(f)) {
                hits.push(format!("{}:{}", p.strip_prefix(root()).unwrap_or(&p).display(), i + 1));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "out-of-memory sites on the containable `almide_abort` (use `almide_abort_oom`):\n{}",
        hits.join("\n")
    );
}

#[test]
fn the_out_of_memory_abort_never_unwinds() {
    let lib = std::fs::read_to_string(root().join("crates/almide-codegen/src/lib.rs")).expect("lib.rs");
    let line = |name: &str| {
        lib.lines()
            .find(|l| l.contains(&format!("fn {name}(")))
            .unwrap_or_else(|| panic!("the prelude no longer defines {name}"))
            .to_string()
    };
    for name in ["almide_abort_oom", "almide_abort_exit"] {
        let l = line(name);
        assert!(!l.contains("almide_contained_unwind"), "{name} unwinds under ALMIDE_CONTAIN:\n{l}");
        assert!(!l.contains("resume_unwind"), "{name} unwinds:\n{l}");
    }
    assert!(
        line("almide_abort_oom").contains("almide_abort_exit("),
        "almide_abort_oom no longer ends through almide_abort_exit"
    );
}

#[test]
fn the_embedded_serve_loop_never_contains_a_refused_growth() {
    let host = std::fs::read_to_string(root().join("crates/almide-wasm-run/src/host.rs")).expect("host.rs");
    let ended = host
        .lines()
        .find(|l| l.trim_start().starts_with("let ended = "))
        .expect("serve_contained's `ended`");
    assert!(ended.contains("!oom"), "a refused growth is contained again:\n{ended}");
    let limiter = &host[host.find("impl wasmtime::ResourceLimiter for Host").expect("the store limiter")..];
    let limiter = &limiter[..limiter.find("\n}\n").expect("the limiter's end")];
    assert!(
        limiter.matches("mark_oom(self)").count() >= 3,
        "every refusal in the store limiter must mark the call out of memory:\n{limiter}"
    );
}
