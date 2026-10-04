//! Host operations — the stdlib calls that reach the host through a native
//! runtime symbol, certified as ORDINARY calls (#2739, ruling A, 2026-10-04).
//!
//! # What the certificate covers
//! The ownership certificate covers how Almide hands a host op its arguments
//! and takes its result, not what the host does with them. A host op is
//! therefore lowered exactly like any other `Op::CallFn`: every heap argument
//! is BORROWED (a `CallArg::Handle` — live-checked, refcount unchanged, the
//! caller keeps and later drops its own reference) and a heap result is a
//! FRESH OWNED value the caller binds, moves out or drops. That is the
//! ordinary-call contract `proofs/CallModes.v` already assumes for a dotted
//! callee (`pipeline::program_witnesses`); no new mode exists for host ops.
//!
//! # Where the contract is written, and the gate that holds it
//! The table below is the one place a host op is admitted. Each row names the
//! stdlib function whose `@intrinsic` carries the native runtime symbol, and
//! that symbol. `tests/host_ops_contract.rs` reads `stdlib/<module>.almd` and
//! `runtime/rs/src/*.rs` and fails unless, for every row: the `@intrinsic` is
//! where the row says; the runtime signature takes no `&mut` parameter and no
//! owned heap value (each is `&T`, a scalar, or a shared `Rc` closure handle
//! the callee counts itself); and it does not return a reference. Those are
//! exactly the facts that make the borrow-args / owned-result contract true of
//! the native body. A runtime signature change that breaks them fails the gate
//! before it can make a certificate lie.
//!
//! # Capabilities
//! A host op reaches the host, so it can never ride the purity admission
//! ([`crate::purity`]): a dotted `CallFn` is otherwise treated as reaching no
//! capability. Each row carries the capability the op reaches, and
//! [`crate::certificate::cap_witness`] counts it at every call site. The
//! mapping is the structural wasm leg's (`almide_wasm::cert_project::op_caps`,
//! same ids): `env.set` reaches the environment (`CliArgs`, the manifest's
//! `Env`), `env.sleep_ms` the clock (`Clock`, `Time`), the http client the
//! network (`Net`, `Net`). A function that reaches a capability it does not
//! declare stays caps-unverified; it is never accepted under a bound that
//! omits the host op.

use crate::Capability;

/// One admitted host op.
#[derive(Clone, Copy, Debug)]
pub struct HostOp {
    /// The stdlib module (`env`, `http`).
    pub module: &'static str,
    /// The function the program calls (`set`, `get`, `start`).
    pub func: &'static str,
    /// The stdlib function whose `@intrinsic` names [`Self::symbol`]: the op
    /// itself, or the private function a self-hosted wrapper calls
    /// (`http.start` → `__call_start`).
    pub intrinsic_fn: &'static str,
    /// The native runtime symbol (`runtime/rs/src/*.rs`).
    pub symbol: &'static str,
    /// The capability a call reaches.
    pub cap: Capability,
}

const fn op(module: &'static str, func: &'static str, symbol: &'static str, cap: Capability) -> HostOp {
    HostOp { module, func, intrinsic_fn: func, symbol, cap }
}

const fn wrapper(
    module: &'static str,
    func: &'static str,
    intrinsic_fn: &'static str,
    symbol: &'static str,
    cap: Capability,
) -> HostOp {
    HostOp { module, func, intrinsic_fn, symbol, cap }
}

use Capability::{CliArgs, Clock, Net};

/// Every admitted host op. Adding one is adding a row; the contract gate then
/// checks it against the stdlib declaration and the runtime signature.
pub const HOST_OPS: &[HostOp] = &[
    op("env", "set", "almide_rt_env_set", CliArgs),
    op("env", "sleep_ms", "almide_rt_env_sleep_ms", Clock),
    // The String client, one row per verb.
    op("http", "get", "almide_http_get", Net),
    op("http", "post", "almide_http_post", Net),
    op("http", "put", "almide_http_put", Net),
    op("http", "patch", "almide_http_patch", Net),
    op("http", "delete", "almide_http_delete", Net),
    op("http", "request", "almide_http_request", Net),
    // The result-shape variants: (status, body) and the raw body bytes.
    op("http", "get_status", "almide_http_get_status", Net),
    op("http", "request_status", "almide_http_request_status", Net),
    op("http", "get_bytes", "almide_http_get_bytes", Net),
    op("http", "request_bytes", "almide_http_request_bytes", Net),
    // The whole-response twins (#1791).
    op("http", "get_response", "almide_http_get_response", Net),
    op("http", "post_response", "almide_http_post_response", Net),
    op("http", "put_response", "almide_http_put_response", Net),
    op("http", "patch_response", "almide_http_patch_response", Net),
    op("http", "delete_response", "almide_http_delete_response", Net),
    op("http", "request_response", "almide_http_request_response", Net),
    // The call handle (#2631): start, the four verbs over it, and the
    // limited streaming twin built on it.
    wrapper("http", "start", "__call_start", "almide_rt_http___call_start", Net),
    op("http", "poll", "almide_http_call_poll", Net),
    op("http", "read_new", "almide_http_call_read_new", Net),
    op("http", "wait", "almide_http_call_wait", Net),
    op("http", "cancel", "almide_http_call_cancel", Net),
    wrapper(
        "http",
        "request_stream_with_limits",
        "__request_stream_limited",
        "almide_rt_http___request_stream_limited",
        Net,
    ),
];

/// The row for `<module>.<func>`, if it is an admitted host op.
pub fn host_op(module: &str, func: &str) -> Option<&'static HostOp> {
    HOST_OPS.iter().find(|o| o.module == module && o.func == func)
}

/// The capability a `CallFn` to the dotted `name` reaches as a host op.
pub fn host_op_capability(name: &str) -> Option<Capability> {
    let (module, func) = name.split_once('.')?;
    host_op(module, func).map(|o| o.cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_unique_and_never_pure() {
        for (i, a) in HOST_OPS.iter().enumerate() {
            assert!(
                !crate::purity::is_pure(a.module, a.func),
                "{}.{} is a host op and must not also be admitted as pure",
                a.module,
                a.func
            );
            for b in &HOST_OPS[i + 1..] {
                assert!(!(a.module == b.module && a.func == b.func), "{}.{} listed twice", a.module, a.func);
            }
        }
    }

    /// The capability follows the op, as the structural leg's `op_caps` has it.
    #[test]
    fn each_op_reaches_the_structural_legs_capability() {
        for o in HOST_OPS {
            let want = match (o.module, o.func) {
                ("env", "set") => CliArgs,
                ("env", "sleep_ms") => Clock,
                ("http", _) => Net,
                (m, f) => panic!("host op {m}.{f} has no capability here"),
            };
            assert_eq!(o.cap, want, "{}.{}", o.module, o.func);
        }
    }

    /// The call site counts the op's capability: an effect fn that reaches `Net`
    /// without declaring it is not caps-verified, one that declares it is.
    #[test]
    fn the_capability_witness_counts_a_host_call() {
        let call = |name: &str| crate::Op::CallFn { dst: None, name: name.into(), args: Vec::new(), result: None };
        let f = crate::MirFunction {
            name: "f".into(),
            ops: vec![call("http.get"), call("env.set"), call("list.len")],
            declared_caps: vec![Capability::Stdout, Capability::CliArgs],
            ..Default::default()
        };
        let w = crate::certificate::cap_witness(&f);
        assert_eq!(w.used, vec![Capability::Net, Capability::CliArgs]);
        assert!(!w.used.iter().all(|c| w.allowed.contains(c)), "Net is undeclared: the bound must not hold");
    }

    #[test]
    fn a_dotted_host_call_names_its_capability() {
        assert_eq!(host_op_capability("env.set"), Some(Capability::CliArgs));
        assert_eq!(host_op_capability("env.sleep_ms"), Some(Capability::Clock));
        assert_eq!(host_op_capability("http.get"), Some(Capability::Net));
        assert_eq!(host_op_capability("list.map"), None);
        assert_eq!(host_op_capability("print_str"), None);
    }
}
