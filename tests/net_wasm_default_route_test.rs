//! `almide build --target wasm` routes a program whose op set names a socket
//! op (the tcp and udp families, `almide_wasm::host_ops::SOCKET_FAMILIES`) to
//! the WASI 0.3 component without a flag — WASI preview 1 has no sockets — and
//! prints the network grant it needs, as an `http.serve` program builds as the
//! serve export. A program with no socket op builds the preview 1 module it
//! built before; `ALMIDE_BASELINE_BIN` (a compiler from before the route)
//! makes that a byte comparison. A socket program that also names an op the
//! p3 shim cannot serve is refused at build time and at `check` time alike.
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn build(bin: &str, src: &Path, out: &Path) -> Output {
    Command::new(bin)
        .args(["build", src.to_str().expect("utf8"), "--target", "wasm", "-o", out.to_str().expect("utf8")])
        .env_remove("ALMIDE_COMPONENT_P3")
        .env_remove("ALMIDE_COMPONENT_ADAPTER")
        .stdin(Stdio::null())
        .output()
        .expect("almide build")
}

/// The wasm binary header: a core module is version 1, a component is the
/// component-model layer (version 0x0d, layer 1).
fn is_component(bytes: &[u8]) -> bool {
    bytes.get(4..8) == Some(&[0x0d, 0x00, 0x01, 0x00][..])
}

fn is_core_module(bytes: &[u8]) -> bool {
    bytes.get(..8) == Some(&[0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00][..])
}

const SOCKET_PROGRAMS: &[(&str, &str)] = &[
    ("udp", "import net\n\neffect fn main() -> Unit = {\n  let s = net.udp_bind(\"127.0.0.1\", 0)!\n  println(\"${net.udp_local_port(s)! > 0}\")\n}\n"),
    ("tcp", "import net\n\neffect fn main() -> Unit = {\n  let l = net.tcp_listen(\"127.0.0.1\", 0)!\n  println(\"${net.tcp_local_port(l)! > 0}\")\n}\n"),
    (
        "lookup",
        "import net\n\neffect fn main() -> Unit = {\n  match net.lookup_host(\"127.0.0.1\") {\n    ok(xs) => println(\"${list.len(xs)}\"),\n    err(e) => println(e),\n  }\n}\n",
    ),
];

#[test]
fn a_socket_program_builds_as_the_p3_component_by_default_and_names_its_grant() {
    let scratch = tempfile::tempdir().expect("tempdir");
    for (name, src) in SOCKET_PROGRAMS {
        let path = scratch.path().join(format!("{name}.almd"));
        std::fs::write(&path, src).expect("write");
        let out = scratch.path().join(format!("{name}.wasm"));
        let built = build(&almide_bin(), &path, &out);
        let stderr = String::from_utf8_lossy(&built.stderr);
        assert!(built.status.success(), "{name}: the default build failed:\n{stderr}");
        let bytes = std::fs::read(&out).expect("read");
        assert!(is_component(&bytes), "{name}: the default build is not a component: {:02x?}", &bytes[..8]);
        assert!(bytes.windows(12).any(|w| w == b"wasi:sockets"), "{name}: the component imports no wasi:sockets");
        assert!(
            stderr.contains("WASI 0.3 component") && stderr.contains("-S inherit-network"),
            "{name}: the build did not name the grant:\n{stderr}"
        );
    }
}

const PLAIN_PROGRAMS: &[(&str, &str)] = &[
    ("echo", "effect fn main() -> Unit = {\n  println(\"hi\")\n}\n"),
    ("pure_net", "import net\n\neffect fn main() -> Unit = {\n  match net.parse_ip(\"::1\") {\n    ok(ip) => println(\"${net.is_loopback(ip)}\"),\n    err(e) => println(e),\n  }\n}\n"),
    ("fs", "import fs\n\neffect fn main() -> Unit = {\n  match fs.read_text(\"/nonexistent\") {\n    ok(t) => println(t),\n    err(e) => println(e),\n  }\n}\n"),
    ("env", "import env\n\neffect fn main() -> Unit = {\n  println(\"${env.millis() >= 0}\")\n}\n"),
];

/// A program with no socket op builds the preview 1 core module, prints no
/// sockets note, and (with `ALMIDE_BASELINE_BIN`) is byte-identical to what
/// the compiler before the route built.
#[test]
fn a_program_with_no_socket_op_builds_the_artifact_it_built_before() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let baseline = std::env::var("ALMIDE_BASELINE_BIN").ok();
    if baseline.is_none() {
        assert!(std::env::var_os("ALMIDE_EXPECT_BASELINE").is_none(), "ALMIDE_EXPECT_BASELINE: ALMIDE_BASELINE_BIN is unset");
    }
    for (name, src) in PLAIN_PROGRAMS {
        let path = scratch.path().join(format!("{name}.almd"));
        std::fs::write(&path, src).expect("write");
        let out = scratch.path().join(format!("{name}.wasm"));
        let built = build(&almide_bin(), &path, &out);
        let stderr = String::from_utf8_lossy(&built.stderr);
        assert!(built.status.success(), "{name}: the build failed:\n{stderr}");
        assert!(!stderr.contains("wasi:sockets"), "{name}: a sockets note on a program with no socket op:\n{stderr}");
        let bytes = std::fs::read(&out).expect("read");
        assert!(is_core_module(&bytes), "{name}: not a preview 1 core module: {:02x?}", &bytes[..8]);
        if let Some(base) = &baseline {
            let base_out = scratch.path().join(format!("{name}.base.wasm"));
            let b = build(base, &path, &base_out);
            assert!(b.status.success(), "{name}: the baseline build failed:\n{}", String::from_utf8_lossy(&b.stderr));
            assert!(std::fs::read(&base_out).expect("read") == bytes, "{name}: the artifact differs from the baseline compiler's");
        }
    }
}

const SOCKET_AND_UNIX: &str = r#"import net
import unix

effect fn main() -> Unit = {
  let s = net.udp_bind("127.0.0.1", 0)!
  let _fd = unix.connect("/tmp/almide-no-such.sock")!
  println("${net.udp_local_port(s)!}")
}
"#;

/// The negative control: a socket op does not carry an op the p3 shim cannot
/// serve (the unix family: WASI has no Unix sockets) into an artifact — the
/// build refuses it, and so does `check --target wasm`, as before the route.
#[test]
fn a_socket_program_with_an_unserved_op_is_refused_at_build_and_check() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let path = scratch.path().join("mixed.almd");
    std::fs::write(&path, SOCKET_AND_UNIX).expect("write");
    let out = scratch.path().join("mixed.wasm");
    let built = build(&almide_bin(), &path, &out);
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(!built.status.success(), "a socket + unix program built:\n{stderr}");
    assert!(!out.exists(), "a refused build left an artifact");
    assert!(stderr.contains("WASI 0.3 component"), "the refusal does not name the component it audited:\n{stderr}");
    let checked = Command::new(almide_bin())
        .args(["check", path.to_str().expect("utf8"), "--target", "wasm"])
        .stdin(Stdio::null())
        .output()
        .expect("almide check");
    assert!(!checked.status.success(), "check --target wasm accepted what the build refuses");
}
