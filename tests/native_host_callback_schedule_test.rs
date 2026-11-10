//! Host callbacks must retain their existing observable scheduling behavior.
use std::process::Command;

#[test]
fn chunked_host_callback_parameter_keeps_sequential_output() {
    std::thread::Builder::new().stack_size(512 << 20).spawn(|| {
        let dir = std::env::temp_dir().join(format!("almide-native-host-schedule-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        check_compiled_routes(&dir, "chunked_callback", include_str!("fixtures/fs_chunked_closure_parameter.almd"),
            "a\nbb\nccc\n[13, 8]\n", "", 0);
        for _ in 0..20 {
            let output = Command::new(dir.join("chunked_callback")).output().unwrap();
            assert!(output.status.success());
            assert!(output.stderr.is_empty());
            assert_eq!(output.stdout, b"a\nbb\nccc\n[13, 8]\n");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }).unwrap().join().unwrap();
}

fn check_compiled_routes(dir: &std::path::Path, name: &str, source: &str,
    expected_stdout: &str, expected_stderr: &str, expected_exit: i32) -> std::process::Output {
    let input = dir.join(format!("{name}.almd"));
    let rs = dir.join(format!("{name}.rs"));
    let binary = dir.join(name);
    std::fs::write(&input, source).unwrap();
    let (rust, certs) = almide_mir::pipeline::try_render_rust_source_certified(source)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    for prop in ["ownership", "native-calls", "bounds"] {
        assert!(certs.iter().any(|(p, _, _)| p == prop), "{name}: missing {prop}");
    }
    for (property, function, witness) in &certs {
        let property = almide_verify::Property::from_name(property).expect("known certificate property");
        assert!(almide_verify::check(property, witness.as_bytes()), "{name}: rejected {property:?} for {function}");
    }
    std::fs::write(&rs, rust).unwrap();
    let compile = Command::new("rustc").args(["--edition=2021", "-C", "opt-level=1"])
        .arg(&rs).arg("-o").arg(&binary).output().unwrap();
    assert!(compile.status.success(), "{name}: {}", String::from_utf8_lossy(&compile.stderr));
    let actual = Command::new(&binary).output().unwrap();
    assert_eq!(actual.status.code(), Some(expected_exit), "{name}: native execution");
    assert_eq!(actual.stdout, expected_stdout.as_bytes(), "{name}: expected bytes");
    assert_eq!(actual.stderr, expected_stderr.as_bytes(), "{name}: stderr");
    let oracle = dir.join(format!("{name}-v0"));
    let built = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["build", "--no-verified"]).arg(&input).arg("-o").arg(&oracle)
        .env("ALMIDE_NO_VERIFIED_OK", "1").output().unwrap();
    assert!(built.status.success(), "{name}: {}", String::from_utf8_lossy(&built.stderr));
    let v0 = Command::new(oracle).output().unwrap();
    assert_eq!((&actual.stdout, &actual.stderr, actual.status.code()),
        (&v0.stdout, &v0.stderr, v0.status.code()), "{name}: v0");
    let wasm = almide::wasm_route::route_wasm(input.to_str().unwrap(), source,
        almide::wasm_route::ModuleSource::Disk { dep_paths: &[] }, None,
        Default::default(), &mut |_| {}).unwrap_or_else(|e| panic!("{name}: wasm route {e:?}"));
    let wasm = almide_wasm_run::run_wasm(&wasm.bytes).unwrap();
    assert_eq!((&actual.stdout, &actual.stderr, actual.status.code()),
        (&wasm.stdout_bytes, &wasm.stderr_bytes, Some(wasm.exit)), "{name}: wasm");
    actual
}
