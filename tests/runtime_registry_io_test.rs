//! Runtime registry generation must not silently omit unreadable source.
#[path = "../crates/almide-codegen/buildscript/runtime_registry.rs"]
mod registry;

#[test]
fn missing_included_source_reports_its_path() {
    let root = tempfile::tempdir().expect("tempdir");
    let sources = root.path().join("runtime/rs/src");
    let out = root.path().join("out");
    std::fs::create_dir_all(&sources).expect("sources");
    std::fs::create_dir(&out).expect("out");
    std::fs::write(sources.join("sample.rs"), "include!(\"missing.rs\");\n").expect("source");
    let error = registry::generate(root.path(), &out).expect_err("missing include must fail");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(error.to_string().contains("missing.rs"), "{error}");
    assert!(!out.join("rust_runtime.rs").exists());
}

#[test]
fn missing_declared_kernel_module_is_not_silently_skipped() {
    let root = tempfile::tempdir().expect("tempdir");
    let sources = root.path().join("runtime/rs/src");
    let kernel = root.path().join("crates/almide-kernel/src");
    let out = root.path().join("out");
    for path in [&sources, &kernel, &out] { std::fs::create_dir_all(path).expect("mkdir"); }
    std::fs::write(sources.join("sample.rs"), "pub fn sample() {}\n").expect("source");
    std::fs::write(kernel.join("lib.rs"), "pub mod missing_kernel;\n").expect("kernel");
    let error = registry::generate(root.path(), &out).expect_err("missing kernel must fail");
    assert!(error.to_string().contains("missing_kernel.rs"), "{error}");
    assert!(!out.join("rust_runtime.rs").exists());
}
