use super::*;
use std::path::Path;
use std::process::Command;

fn write(root: &Path, path: &str, source: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
}

fn render_checked(entry: &Path) -> String {
    let (mut pending, _, _) = try_compile_pending(
        entry.to_str().unwrap(), false, &codegen::CodegenOptions::default(),
    ).expect("entry and imported modules check");
    let (ir, checker) = pending.native_input.take().expect("checked frontend input");
    // The native handoff must consume the checked input, not read or parse
    // the entry again. The retained source is only for optional debug output.
    std::fs::remove_file(entry).unwrap();
    let (rust, certs) = std::thread::Builder::new().stack_size(512 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_checked_certified(
            &pending.source, ir, checker,
        )).unwrap().join().unwrap().expect("direct v1 render, without fallback");
    assert!(!certs.is_empty(), "the checked-input route must produce witnesses");
    let verifier = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap()
        .join("almide-verify");
    if verifier.is_file() {
        let evidence = tempfile::Builder::new().prefix("almide-native-module-proof-")
            .tempdir().unwrap().keep();
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
    let error = almide_mir::pipeline::try_render_rust_checked_certified(&pending.source, ir, checker)
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
    for path in &paths {
        let input = path.clone();
        let result = std::thread::Builder::new().stack_size(512 << 20).spawn(move || {
            let (mut pending, _, _) = try_compile_pending(input.to_str().unwrap(), false,
                &codegen::CodegenOptions::default())?;
            let (ir, checker) = pending.native_input.take().ok_or("no checked input")?;
            almide_mir::pipeline::try_render_rust_checked_certified(&pending.source, ir, checker)
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
            &serde_json::json!({"total": paths.len(), "rows": rows, "errors": errors})
        ).unwrap()).unwrap();
    }
    assert!(errors.is_empty(), "{} refused programs; evidence: {}", errors.len(), evidence.display());
    // Success means every artifact was exported, not that it has been
    // independently checked or that its execution is equivalent.
}
