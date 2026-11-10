//! Explicit Result returns from main must retain entry-wrapper observations.
use std::process::Command;

#[test]
fn result_main_preserves_outputs_and_exit_status() {
    let cases = [
        ("unit_ok", "effect fn main() -> Result[Unit, String] = { println(\"before\")\nok(()) }", "before\n", "", 0),
        ("unit_err", "effect fn main() -> Result[Unit, String] = { println(\"before\")\nerr(\"stop\") }", "before\n", "Error: stop\n", 1),
        ("string_ok", "effect fn main() -> Result[String, String] = { println(\"before\")\nok(\"ignored\") }", "before\n", "", 0),
        ("computed_err", "fn pick(b: Bool) -> Result[Int, String] = if b then ok(7) else err(\"failed\")\neffect fn main() -> Result[Int, String] = { println(\"before\")\npick(false) }", "before\n", "Error: failed\n", 1),
    ];
    let dir = tempfile::tempdir().unwrap();
    let mut failed = Vec::new();
    for (name, source, stdout, stderr, exit) in cases {
        let result = std::panic::catch_unwind(|| {
            let input = dir.path().join(format!("{name}.almd"));
            std::fs::write(&input, source).unwrap();
            let (rust, certs) = almide_mir::pipeline::try_render_rust_source_certified(source)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(certs
                .iter()
                .any(|(p, f, _)| p == "ownership" && f == "main"));
            assert!(certs.iter().any(|(p, _, _)| p == "native-calls"));
            let rs = dir.path().join(format!("{name}.rs"));
            let exe = dir.path().join(name);
            std::fs::write(&rs, rust).unwrap();
            let build = Command::new("rustc")
                .args(["--edition=2021", "-C", "opt-level=1"])
                .arg(&rs)
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(
                build.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&build.stderr)
            );
            let actual = Command::new(&exe).output().unwrap();
            let observed = (&actual.stdout[..], &actual.stderr[..], actual.status.code());
            assert_eq!(
                observed,
                (stdout.as_bytes(), stderr.as_bytes(), Some(exit)),
                "{name}: expected"
            );
            let v0 = dir.path().join(format!("{name}-v0"));
            let build = Command::new(env!("CARGO_BIN_EXE_almide"))
                .args(["build", "--no-verified"])
                .arg(&input)
                .arg("-o")
                .arg(&v0)
                .env("ALMIDE_NO_VERIFIED_OK", "1")
                .output()
                .unwrap();
            assert!(
                build.status.success(),
                "{name}: v0 {}",
                String::from_utf8_lossy(&build.stderr)
            );
            let v0 = Command::new(v0).output().unwrap();
            assert_eq!(
                observed,
                (&v0.stdout[..], &v0.stderr[..], v0.status.code()),
                "{name}: v0"
            );
            let ir = almide::wasm_leg::lower_to_ir(input.to_str().unwrap(), source).unwrap();
            let interp = almide_interp::Interpreter::new(&ir).run_main();
            assert_eq!(
                observed,
                (
                    interp.stdout.as_bytes(),
                    interp.stderr.as_bytes(),
                    Some(interp.exit_code())
                ),
                "{name}: interp {:?}",
                interp.status
            );
            let wasm = almide::wasm_route::route_wasm(
                input.to_str().unwrap(),
                source,
                almide::wasm_route::ModuleSource::Disk { dep_paths: &[] },
                None,
                Default::default(),
                &mut |_| {},
            )
            .unwrap();
            let wasm = almide_wasm_run::run_wasm(&wasm.bytes).unwrap();
            assert_eq!(
                observed,
                (
                    &wasm.stdout_bytes[..],
                    &wasm.stderr_bytes[..],
                    Some(wasm.exit)
                ),
                "{name}: wasm"
            );
        });
        if result.is_err() {
            failed.push(name);
        }
    }
    assert!(failed.is_empty(), "failed main cases: {failed:?}");
}
