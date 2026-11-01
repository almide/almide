//! UDP and name lookup on the WASI 0.3 component (#4306): the p3 shim over
//! wasi:sockets@0.3 `udp-socket` and `ip-name-lookup` answers what native
//! answers. C-442 and C-443's fixtures (spec/embedded_cross, the judge's
//! own) are run natively and as the component on a stock wasmtime with the
//! network granted (`-S inherit-network=y -S allow-ip-name-lookup=y`); stdout
//! and the exit code must be identical, and each program must reach its
//! closing `done` line, so two legs that fail alike cannot pass.
//!
//! Also here:
//! - a component that names no udp op imports no wasi:sockets (the world
//!   lists them; wit-component keeps only what the module calls), so an
//!   echo program still runs with no network grant;
//! - with no grant, binding reads as the classified permission text — the
//!   same `<fn>(<host>:<port>): <class>` shape as every other failure (the
//!   net/tcp design's Q7);
//! - the capability bound: the udp ops reach `cap::NET` and nothing else, a
//!   udp program's inferred effects grant it, and — the control — the same
//!   op set under the effects minus the Net leaves is refused.
//!
//! The runtime floor is the pin policy's (wasmtime 46); below it the
//! component tests skip, and under ALMIDE_EXPECT_TOOLS (CI) that is a
//! failure.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[path = "support/capped_run.rs"]
mod capped_run;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// The installed wasmtime runs p3 components (the pin policy's floor, 46).
fn wasmtime_runs_p3() -> bool {
    let Ok(o) = Command::new("wasmtime").arg("--version").output() else {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH");
        return false;
    };
    let v = String::from_utf8_lossy(&o.stdout).to_string();
    let major = v.split_whitespace().nth(1).and_then(|ver| ver.split('.').next()).and_then(|m| m.parse::<u32>().ok()).unwrap_or(0);
    if major < 46 {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: {} is below the p3 floor (46)", v.trim());
        eprintln!("skipping: {} is below the p3 floor (46)", v.trim());
        return false;
    }
    true
}

/// stdout and the exit code, under a 120 s cap (a datagram that never
/// arrives becomes a named failure, not a cancelled job).
fn observe(mut cmd: Command, what: &str) -> (String, Option<i32>, String) {
    cmd.stdin(Stdio::null());
    let out = capped_run::output_capped(cmd, Duration::from_secs(120)).unwrap_or_else(|e| panic!("{what}: {e}"));
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/embedded_cross").join(format!("{name}.almd"))
}

/// `src` as a WASI 0.3 component at `out`, or the build's stderr.
fn build_p3(src: &Path, out: &Path) -> Result<(), String> {
    let built = Command::new(almide_bin())
        .args(["build", src.to_str().expect("utf8"), "--target", "wasm", "--component", "-o", out.to_str().expect("utf8")])
        .env("ALMIDE_COMPONENT_P3", "1")
        .stdin(Stdio::null())
        .output()
        .expect("almide build");
    if built.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&built.stderr).into_owned()) }
}

/// `wasmtime run` with the network grant (`granted`) or without.
fn run_p3(wasm: &Path, granted: bool) -> (String, Option<i32>, String) {
    let mut cmd = Command::new("wasmtime");
    cmd.arg("run");
    if granted {
        cmd.args(["-S", "inherit-network=y", "-S", "allow-ip-name-lookup=y", "-S", "udp=y"]);
    }
    cmd.arg(wasm);
    observe(cmd, "wasmtime run")
}

#[test]
fn udp_and_lookup_fixtures_run_alike_on_native_and_the_p3_component() {
    if !wasmtime_runs_p3() {
        return;
    }
    let scratch = tempfile::tempdir().expect("tempdir");
    for name in ["net_udp_loopback", "net_udp_errors"] {
        let src = fixture(name);
        let mut native = Command::new(almide_bin());
        native.arg("run").arg(&src);
        let native = observe(native, "almide run");
        assert!(native.0.ends_with("done\n"), "{name}: native did not finish:\n{}\n{}", native.0, native.2);
        let wasm = scratch.path().join(format!("{name}.wasm"));
        build_p3(&src, &wasm).unwrap_or_else(|e| panic!("{name}: the p3 build failed:\n{e}"));
        let p3 = run_p3(&wasm, true);
        assert!(p3.0.ends_with("done\n"), "{name}: the p3 component did not finish:\n{}\n{}", p3.0, p3.2);
        assert_eq!((&p3.0, p3.1), (&native.0, native.1), "{name}: the p3 component diverged from native\nstderr: {}", p3.2);
    }
}

const ECHO: &str = "effect fn main() -> Unit = {\n  println(\"hi\")\n}\n";

const BIND: &str = r#"import net

effect fn main() -> Unit = {
  match net.udp_bind("127.0.0.1", 0) {
    ok(_) => println("bound"),
    err(e) => println(e),
  }
}
"#;

/// A program without a udp op carries no wasi:sockets import; one with
/// them, run with no network grant, reads the classified permission text.
#[test]
fn sockets_ship_only_with_udp_ops_and_a_missing_grant_is_the_permission_text() {
    if !wasmtime_runs_p3() {
        return;
    }
    let scratch = tempfile::tempdir().expect("tempdir");
    for (name, src, net) in [("echo", ECHO, false), ("bind", BIND, true)] {
        let path = scratch.path().join(format!("{name}.almd"));
        std::fs::write(&path, src).expect("write");
        let wasm = scratch.path().join(format!("{name}.wasm"));
        build_p3(&path, &wasm).unwrap_or_else(|e| panic!("{name}: the p3 build failed:\n{e}"));
        let bytes = std::fs::read(&wasm).expect("read");
        let imports_sockets = bytes.windows(b"wasi:sockets".len()).any(|w| w == b"wasi:sockets");
        assert_eq!(imports_sockets, net, "{name}: wasi:sockets in the component = {imports_sockets}");
        let ungranted = run_p3(&wasm, false);
        let want = if net { "udp_bind(127.0.0.1:0): permission denied\n" } else { "hi\n" };
        assert_eq!((ungranted.0.as_str(), ungranted.1), (want, Some(0)), "{name} with no grant: {ungranted:?}");
    }
}

/// The effect names the checker reports for any function of `src`.
fn effects(src: &Path) -> BTreeSet<String> {
    let o = Command::new(almide_bin()).args(["check", "--effects", src.to_str().expect("utf8")]).stdin(Stdio::null()).output().expect("almide");
    let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert!(o.status.success() && text.contains("functions:"), "no effect report for {}:\n{text}", src.display());
    let mut out = BTreeSet::new();
    for line in text.lines() {
        let Some((_, rest)) = line.split_once("→ {") else { continue };
        let Some((set, _)) = rest.split_once('}') else { continue };
        out.extend(set.split(',').map(str::trim).filter(|e| !e.is_empty()).map(String::from));
    }
    out
}

/// The capabilities a set of effect names grants the udp family: the
/// network, through any Net leaf (tests/wasm_imports_within_effects_test.rs's
/// `CAPS_BY_EFFECT` rows for `Net.fetch` / `Net.listen`).
fn net_grant(effects: &BTreeSet<String>) -> BTreeSet<u32> {
    effects.iter().filter(|e| e.starts_with("Net.")).map(|_| almide_wasm::cert_project::cap::NET).collect()
}

#[test]
fn the_udp_ops_reach_the_network_only_and_a_grant_without_net_refuses_them() {
    use almide::wasm_route::{route_wasm, ModuleSource, RouteOptions};
    use almide_wasm::cert_project::{cap, fs_call_ops, op_caps};
    for op in almide_wasm::host_ops::UDP {
        assert_eq!(op_caps(op), &[cap::NET][..], "udp op {op}");
    }
    for name in ["net_udp_loopback", "net_udp_errors"] {
        let src = fixture(name);
        let text = std::fs::read_to_string(&src).expect("read");
        let path = src.to_str().expect("utf8");
        let raw = route_wasm(path, &text, ModuleSource::Disk { dep_paths: &[] }, None, RouteOptions::default(), &mut |_| {})
            .unwrap_or_else(|e| panic!("{name}: the in-process route failed: {e:?}"));
        let ops = fs_call_ops(&raw.bytes).unwrap_or_else(|e| panic!("{name}: {e}")).expect("every op a visible constant");
        let udp: BTreeSet<i32> = ops.iter().copied().filter(|op| almide_wasm::host_ops::UDP.contains(op)).collect();
        assert!(!udp.is_empty(), "{name} names no udp op: {ops:?}");
        let reach: BTreeSet<u32> = udp.iter().flat_map(|op| op_caps(*op).iter().copied()).collect();
        let eff = effects(&src);
        assert!(reach.is_subset(&net_grant(&eff)), "{name}: the udp ops reach {reach:?}, the effects {eff:?} grant {:?}", net_grant(&eff));
        // The control: the same op set under the effects without their Net
        // leaves is outside the grant.
        let without: BTreeSet<String> = eff.iter().filter(|e| !e.starts_with("Net.")).cloned().collect();
        assert!(!reach.is_subset(&net_grant(&without)), "{name}: a grant without Net still admits {reach:?}");
    }
}
