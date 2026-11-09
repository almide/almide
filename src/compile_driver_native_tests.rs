use super::*;
use std::path::Path;
use std::process::Command;

#[test]
fn checked_native_test_harness_keeps_failures_isolated_and_names_selectable() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("tests.almd");
    write(dir.path(), "tests.almd", r#"
import testing
var state = 0
effect fn main() -> Unit = println("USER MAIN MUST NOT RUN")
fn fail_helper(message: String) -> Never = panic(message)
fn fail_message() -> String = panic("MESSAGE MUST NOT RUN")
test "a pass" { state = state + 1; assert_eq(state, 1) }
test "b fail" { assert_eq(1, 2) }
test "c pass" { state = state + 1; assert_eq(state, 1) }
test "d helper panic" { fail_helper("helper failure") }
test "e closure panic" { let f = () => panic("closure failure"); f() }
test "f value panic" { let x = if state == 0 then fail_helper("value failure") else 1; assert_eq(x, 1) }
test "g abort child" { testing.assert_aborts("PANIC: child failure", () => fail_helper("child failure")) }
test "h lazy message" { assert(true, fail_message()) }
test "日本語の名前" { assert(true) }
"#);
    let (mut pending, _, _) = try_compile_pending(entry.to_str().unwrap(), false,
        &codegen::CodegenOptions::default()).unwrap();
    let (ir, checker) = pending.native_input.take().unwrap();
    let source = pending.source.clone();
    let oracle = pending.generate();
    let (rust, certs) = std::thread::Builder::new().stack_size(512 << 20).spawn(move || {
        almide_mir::pipeline::try_render_rust_checked_tests_certified(&source, ir, checker, None)
    }).unwrap().join().unwrap().expect("native test bodies must render without v0");
    let evidence = tempfile::Builder::new().prefix("almide-native-test-proof-").tempdir().unwrap().keep();
    let artifact = evidence.join("tests.rs");
    let binary = evidence.join("tests");
    std::fs::write(&artifact, &rust).unwrap();
    let bundle = evidence.join("tests.bundle");
    std::fs::write(&bundle, certificate_bundle(&entry, &artifact, &rust, &certs)).unwrap();
    std::fs::copy(&entry, evidence.join("tests.almd")).unwrap();
    println!("NATIVE_TEST_EVIDENCE={}", evidence.display());
    let verifier = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join("almide-verify");
    if verifier.is_file() {
        assert!(Command::new(verifier).arg("bundle").arg(&bundle).status().unwrap().success());
    } else {
        assert_ne!(std::env::var("ALMIDE_EXPECT_TOOLS").as_deref(), Ok("1"));
    }
    let built = Command::new(crate::find_rustc()).args(["--edition=2021", "--test", "-Awarnings"])
        .arg(&artifact).arg("-o").arg(&binary).output().unwrap();
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let oracle_source = evidence.join("oracle.rs");
    let oracle_binary = evidence.join("oracle");
    std::fs::write(&oracle_source, oracle).unwrap();
    let built = Command::new(crate::find_rustc()).args(["--edition=2021", "--test", "-Awarnings"])
        .arg(&oracle_source).arg("-o").arg(&oracle_binary).output().unwrap();
    assert!(built.status.success(), "v0 oracle: {}", String::from_utf8_lossy(&built.stderr));
    let oracle_ledger = evidence.join("oracle-completed.txt");
    let oracle_ran = Command::new(&oracle_binary).arg("--test-threads=1").env("ALMIDE_TEST_LEDGER", &oracle_ledger).output().unwrap();
    std::fs::write(evidence.join("oracle-stdout"), &oracle_ran.stdout).unwrap();
    std::fs::write(evidence.join("oracle-stderr"), &oracle_ran.stderr).unwrap();
    let ledger = evidence.join("completed.txt");
    let ran = Command::new(&binary).arg("--test-threads=1").env("ALMIDE_TEST_LEDGER", &ledger).output().unwrap();
    std::fs::write(evidence.join("stdout"), &ran.stdout).unwrap();
    std::fs::write(evidence.join("stderr"), &ran.stderr).unwrap();
    assert_eq!(ran.status.code(), Some(101), "{}", String::from_utf8_lossy(&ran.stderr));
    let output = String::from_utf8(ran.stdout).unwrap();
    let oracle_output = String::from_utf8(oracle_ran.stdout).unwrap();
    let verdicts = |text: &str| text.lines().filter(|l| l.starts_with("test tests::")).map(str::to_owned).collect::<Vec<_>>();
    assert_eq!(ran.status.code(), oracle_ran.status.code());
    assert_eq!(verdicts(&output), verdicts(&oracle_output), "v1:\n{output}\nv0:\n{oracle_output}");
    assert!(output.contains("5 passed; 4 failed; 1 ignored"), "{output}");
    for message in ["helper failure", "closure failure", "value failure"] {
        assert!(output.contains(message), "{output}");
    }
    assert!(!output.contains("USER MAIN MUST NOT RUN"), "{output}");
    assert!(!output.contains("MESSAGE MUST NOT RUN"), "{output}");
    for diagnostic in ["at line 8", "expected: 2", "found: 1"] {
        assert!(output.contains(diagnostic), "{output}");
        assert!(oracle_output.contains(diagnostic), "{oracle_output}");
    }
    assert_eq!(std::fs::read_to_string(&ledger).unwrap().lines().count(), 5);
    assert_eq!(std::fs::read_to_string(&ledger).unwrap(), std::fs::read_to_string(&oracle_ledger).unwrap());
    let selected = Command::new(&binary).arg("c_pass").output().unwrap();
    assert!(selected.status.success());
    assert!(String::from_utf8_lossy(&selected.stdout).contains("1 passed; 0 failed"));
    let probe = Command::new(&binary).args(["--ignored", "--exact", "tests::__almd_abort_site_entry", "--nocapture"])
        .env("ALMIDE_ABORT_SITE", "0").env("ALMIDE_TEST_ABORT_PROBE", "1").output().unwrap();
    assert_eq!(probe.status.code(), Some(1));
    assert_eq!(probe.stderr, b"PANIC: child failure");
    let oracle_probe = Command::new(&oracle_binary).args(["--ignored", "--exact", "tests::__almd_abort_site_entry", "--nocapture"])
        .env("ALMIDE_ABORT_SITE", "0").env("ALMIDE_TEST_ABORT_PROBE", "1").output().unwrap();
    assert_eq!(probe.status.code(), oracle_probe.status.code());
    assert_eq!(probe.stdout, oracle_probe.stdout);
    assert_eq!(probe.stderr, oracle_probe.stderr);
}

fn write(root: &Path, path: &str, source: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
}

fn render_checked(entry: &Path) -> String {
    render_checked_with_scope(entry, None)
}

fn render_checked_with_scope(entry: &Path, roots: Option<Vec<String>>) -> String {
    let (mut pending, _, _) = try_compile_pending(
        entry.to_str().unwrap(), false, &codegen::CodegenOptions::default(),
    ).expect("entry and imported modules check");
    let (ir, checker) = pending.native_input.take().expect("checked frontend input");
    // The native handoff must consume the checked input, not read or parse
    // the entry again. The retained source is only for optional debug output.
    let original_input = std::fs::read(entry).unwrap();
    std::fs::remove_file(entry).unwrap();
    let (rust, certs) = std::thread::Builder::new().stack_size(512 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_checked_scoped_certified(
            &pending.source, ir, checker, roots,
        )).unwrap().join().unwrap().expect("direct v1 render, without fallback");
    assert!(!certs.is_empty(), "the checked-input route must produce witnesses");
    let verifier = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap()
        .join("almide-verify");
    if verifier.is_file() {
        let evidence = tempfile::Builder::new().prefix("almide-native-module-proof-")
            .tempdir().unwrap().keep();
        std::fs::write(evidence.join(entry.file_name().unwrap()), &original_input).unwrap();
        let artifact = evidence.join("render.rs");
        std::fs::write(&artifact, &rust).unwrap();
        let bundle = evidence.join("render.bundle");
        std::fs::write(&bundle, certificate_bundle(entry, &artifact, &rust, &certs)).unwrap();
        let verdict = Command::new(verifier).arg("bundle").arg(bundle).output().unwrap();
        println!("NATIVE_MODULE_EVIDENCE={}", evidence.display());
        assert!(verdict.status.success(), "{}\n{}", String::from_utf8_lossy(&verdict.stdout),
            String::from_utf8_lossy(&verdict.stderr));
    } else {
        assert_ne!(std::env::var("ALMIDE_EXPECT_TOOLS").as_deref(), Ok("1"),
            "independent verifier is required on the verification VM");
        eprintln!("independent certificate check not run: almide-verify unavailable");
    }
    rust
}

fn certificate_bundle(source: &Path, artifact: &Path, rust: &str, certs: &[(String, String, String)]) -> String {
    use sha2::{Digest, Sha256};
    let digest: String = Sha256::digest(rust.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    let mut bundle = format!("almide-certificate-bundle 5\nproducer checked-native-frontend\nsource {}\nartifact sha256 {digest} {}\n", source.display(), artifact.display());
    for (property, function, witness) in certs {
        bundle.push_str(&format!("witness {property} {} {function}\n{witness}\n", witness.len()));
    }
    bundle
}

fn run_rust(root: &Path, rust: &str, expected: &str) {
    let source = root.join("render.rs");
    let binary = root.join("render");
    std::fs::write(&source, rust).unwrap();
    let built = Command::new(crate::find_rustc()).arg("--edition=2021")
        .args(["-Awarnings", "-C", "opt-level=1"])
        .arg(&source).arg("-o").arg(&binary).output().unwrap();
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let output = Command::new(binary).output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, expected.as_bytes());
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
}

#[test]
fn checked_native_input_preserves_sibling_toplets_and_calls() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "almide.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\n");
    write(dir.path(), "src/helper.almd", "let OFFSET = 40\nfn inc(n: Int) -> Int = n + OFFSET\n");
    write(dir.path(), "src/main.almd", "import self.helper\neffect fn main() -> Unit = println(\"${helper.inc(2)}\")\n");
    let rust = render_checked(&dir.path().join("src/main.almd"));
    assert!(!rust.contains("struct __AlmdClosure"),
        "unreached library locals must not pull a closure runtime into scalar code");
    run_rust(dir.path(), &rust, "42\n");
}

#[test]
fn checked_native_input_preserves_dependency_identity_and_nested_self() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "dep/almide.toml", "[package]\nname = \"deplib\"\nversion = \"0.1.0\"\n");
    write(dir.path(), "dep/src/core.almd", "let OFFSET = 40\nfn value() -> Int = OFFSET + 1\n");
    write(dir.path(), "dep/src/native/leaf.almd", "import self.core\nfn value() -> Int = core.value() + 1\n");
    write(dir.path(), "app/almide.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[dependencies]\ndeplib = { path = \"../dep\" }\n");
    write(dir.path(), "app/main.almd", "import deplib.native.leaf as leaf\neffect fn main() -> Unit = println(\"${leaf.value()}\")\n");
    let rust = render_checked(&dir.path().join("app/main.almd"));
    run_rust(dir.path(), &rust, "42\n");
}

#[test]
fn checked_native_input_accepts_checked_json_ast() {
    let dir = tempfile::tempdir().unwrap();
    let tokens = lexer::Lexer::tokenize(concat!(
        "type Boxed = { value: Int = 1 + 2 }\n",
        "effect fn main() -> Unit = {\n",
        " let b = Boxed {}\n",
        " let n = match 2 { 1 => 10, 2 => 20, _ => 30 }\n",
        " println(\"json-ast ${b.value + n}\")\n}\n",
    ));
    let program = parser::Parser::new(tokens).parse().unwrap();
    write(dir.path(), "main.json", &serde_json::to_string(&program).unwrap());
    let rust = render_checked(&dir.path().join("main.json"));
    run_rust(dir.path(), &rust, "json-ast 23\n");
}

#[test]
fn checked_native_input_rejects_imported_type_errors_before_handoff() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "almide.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\n");
    write(dir.path(), "src/helper.almd", "fn bad() -> Int = \"wrong\"\n");
    write(dir.path(), "src/main.almd", "import self.helper\neffect fn main() -> Unit = println(\"${helper.bad()}\")\n");
    let result = try_compile_pending(dir.path().join("src/main.almd").to_str().unwrap(),
        false, &codegen::CodegenOptions::default());
    assert!(result.is_err(), "an invalid imported body cannot enter the native renderer");
}

#[test]
fn checked_native_input_keeps_an_unreferenced_module_c_export_visible() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "almide.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\n");
    write(dir.path(), "src/helper.almd", "@export(c, \"answer\")\nfn answer() -> Int = 42\nfn used() -> Int = 7\n");
    write(dir.path(), "src/main.almd", "import self.helper\neffect fn main() -> Unit = println(\"${helper.used()}\")\n");
    let (mut pending, _, _) = try_compile_pending(dir.path().join("src/main.almd").to_str().unwrap(),
        false, &codegen::CodegenOptions::default()).expect("valid module export");
    let (ir, checker) = pending.native_input.take().unwrap();
    let error = almide_mir::pipeline::try_render_rust_checked_scoped_certified(&pending.source, ir, checker, None)
        .expect_err("the current v1 renderer must not silently drop a required C symbol");
    assert!(error.to_string().contains("is @export(c)"), "{error}");
}

/// Export the actual CLI frontend's witnesses for the independent verifier.
/// This deliberately does not call the standalone source producer: that
/// producer cannot validate this handoff or its resolved modules.
#[test]
#[ignore = "whole-corpus artifact export for the verification VM"]
fn export_checked_native_corpus_certificates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let paths = std::fs::read_dir(root.join("spec/wasm_cross")).unwrap()
        .map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|s| s == "almd"))
        .collect();
    export_checked_native_paths(paths);
}

#[test]
#[ignore = "additional regression artifact export for the verification VM"]
fn export_checked_native_additional_certificates() {
    let list = std::env::var_os("NATIVE_FRONTEND_CASES_FILE").expect("explicit additional-case list");
    let paths = std::fs::read_to_string(list).unwrap().lines()
        .filter(|p| !p.is_empty()).map(std::path::PathBuf::from).collect();
    export_checked_native_paths(paths);
}

fn export_checked_native_paths(mut paths: Vec<std::path::PathBuf>) {
    use sha2::{Digest, Sha256};
    assert!(!paths.is_empty());
    let evidence = tempfile::Builder::new().prefix("almide-checked-native-")
        .tempdir().unwrap().keep();
    println!("NATIVE_FRONTEND_AUDIT_DIR={}", evidence.display());
    paths.sort();
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let fs_roots: Option<Vec<String>> = std::env::var("NATIVE_FRONTEND_FS_ROOTS_JSON").ok()
        .map(|s| serde_json::from_str(&s).expect("explicit filesystem roots JSON"));
    for path in &paths {
        let input = path.clone();
        let roots = fs_roots.clone();
        let result = std::thread::Builder::new().stack_size(512 << 20).spawn(move || {
            let (mut pending, _, _) = try_compile_pending(input.to_str().unwrap(), false,
                &codegen::CodegenOptions::default())?;
            let (ir, checker) = pending.native_input.take().ok_or("no checked input")?;
            almide_mir::pipeline::try_render_rust_checked_scoped_certified(&pending.source, ir, checker, roots)
                .map_err(|e| e.to_string())
        }).unwrap().join();
        match result {
            Ok(Ok((rust, certs))) => {
                let name = path.file_stem().unwrap().to_str().unwrap();
                let artifact = evidence.join(format!("{name}.rs"));
                std::fs::write(&artifact, &rust).unwrap();
                let digest: String = Sha256::digest(rust.as_bytes()).iter()
                    .map(|b| format!("{b:02x}")).collect();
                let bundle = certificate_bundle(path, &artifact, &rust, &certs);
                std::fs::write(evidence.join(format!("{name}.bundle")), bundle).unwrap();
                rows.push(serde_json::json!({"path": path, "rust_sha256": digest, "witnesses": certs.len()}));
            }
            Ok(Err(error)) => errors.push(serde_json::json!({"path": path, "error": error})),
            Err(_) => errors.push(serde_json::json!({"path": path, "error": "frontend/render panic"})),
        }
        std::fs::write(evidence.join("results.json"), serde_json::to_vec_pretty(
            &serde_json::json!({"total": paths.len(), "rows": rows, "errors": errors, "fs_roots": fs_roots})
        ).unwrap()).unwrap();
    }
    assert!(errors.is_empty(), "{} refused programs; evidence: {}", errors.len(), evidence.display());
    // Success means every artifact was exported, not that it has been
    // independently checked or that its execution is equivalent.
}

#[test]
fn checked_native_scope_bounds_reads_writes_and_predicates_before_artifact_binding() {
    let dir = tempfile::tempdir().unwrap();
    let allowed = dir.path().join("allowed");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(allowed.join("read.txt"), "inside").unwrap();
    std::fs::write(outside.join("secret.txt"), "secret").unwrap();
    let source = format!(r#"import fs
effect fn main() -> Unit = {{
  println(fs.read_text({read:?}) ?? "blocked")
  println(fs.read_text({secret:?}) ?? "blocked")
  println("${{result.is_ok(fs.write_text({write:?}, "new"))}}")
  println("${{result.is_ok(fs.write_text({escape:?}, "forbidden"))}}")
  println("${{fs.exists({secret:?})}}")
}}
"#, read=allowed.join("read.txt").to_str().unwrap(), secret=outside.join("secret.txt").to_str().unwrap(),
        write=allowed.join("written.txt").to_str().unwrap(), escape=outside.join("escape.txt").to_str().unwrap());
    let entry = dir.path().join("main.almd");
    write(dir.path(), "main.almd", &source);
    let rust = render_checked_with_scope(&entry, Some(vec![allowed.to_str().unwrap().into()]));
    assert!(rust.starts_with(almide_codegen::fs_scope::SCOPE_MARKER));
    run_rust(dir.path(), &rust, "inside\nblocked\ntrue\nfalse\nfalse\n");
    assert_eq!(std::fs::read_to_string(allowed.join("written.txt")).unwrap(), "new");
    assert!(!outside.join("escape.txt").exists());
}

#[test]
fn checked_native_empty_fs_scope_denies_write_only_program() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("forbidden.txt");
    write(dir.path(), "main.almd", &format!(
        "import fs\neffect fn main() -> Unit = println(\"${{result.is_ok(fs.write_text({:?}, \"x\"))}}\")\n",
        target.to_str().unwrap()));
    let rust = render_checked_with_scope(&dir.path().join("main.almd"), Some(vec![]));
    run_rust(dir.path(), &rust, "false\n");
    assert!(!target.exists());
}

#[test]
fn checked_native_scope_preserves_host_callback_scheduling() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.almd", include_str!("../tests/fixtures/fs_chunked_closure_parameter.almd"));
    let roots = vec![std::env::temp_dir().to_str().unwrap().to_string()];
    let rust = render_checked_with_scope(&dir.path().join("main.almd"), Some(roots));
    run_rust(dir.path(), &rust, "a\nbb\nccc\n[13, 8]\n");
}
