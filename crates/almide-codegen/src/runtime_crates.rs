//! The crates.io crates the inlined runtime modules are written against (#3346).

/// The TLS crates the `http` runtime module (and `sse`, whose streaming
/// transport is `http`) is written against.
const TLS_CRATES: &[(&str, &str)] = &[
    ("rustls", r#"{ version = "0.23", default-features = false, features = ["ring", "logging", "std", "tls12"] }"#),
    ("webpki-roots", "0.26"),
    ("rustls-native-certs", "0.8"),
];

/// Every runtime module whose source uses a crates.io crate, with those crates
/// as (name, Cargo spec) — THE one table both facts derive from (#3346):
///
/// - which crates a generated project's manifest declares, on every build
///   route (bin, `--cdylib`, `--repr-c`, test, `almide run`):
///   [`runtime_crate_deps`]. The bin route once chose an HTTP template and
///   appended `flate2` while the cdylib route declared neither, so a cdylib
///   using `zlib` failed with E0433 on `flate2`;
/// - which modules stay out of the bare-rustc `almide_rt` rlib
///   ([`crate::emit_runtime_crate`]): programs using them take the cargo path.
///
/// A module is in use when its `almide_rt_<module>_` symbols appear in the
/// generated code (the runtime is inlined as source).
pub const RUNTIME_MODULE_CRATES: &[(&str, &[(&str, &str)])] = &[
    ("http", TLS_CRATES),
    ("sse", TLS_CRATES),
    ("zlib", &[("flate2", "1")]),
];

/// The crates.io dependencies (name, Cargo spec) generated Rust `rs_code`
/// needs for its inlined runtime modules, each crate once, in table order.
pub fn runtime_crate_deps(rs_code: &str) -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = Vec::new();
    for (module, crates) in RUNTIME_MODULE_CRATES {
        if !rs_code.contains(&format!("almide_rt_{module}_")) {
            continue;
        }
        for &(name, spec) in crates.iter() {
            if !out.iter().any(|(n, _)| *n == name) {
                out.push((name, spec));
            }
        }
    }
    out
}
