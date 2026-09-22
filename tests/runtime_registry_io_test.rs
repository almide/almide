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

/// #2511: a runtime module whose test block never closes must FAIL the build, not be
/// embedded with its tail deleted. The runtime text is embedded unstripped and stripped
/// at emit, so the build script is the earliest place the defect can be caught.
#[test]
fn a_runtime_module_with_an_unterminated_test_block_fails_the_build() {
    let root = tempfile::tempdir().expect("tempdir");
    let sources = root.path().join("runtime/rs/src");
    let out = root.path().join("out");
    std::fs::create_dir_all(&sources).expect("sources");
    std::fs::create_dir(&out).expect("out");
    std::fs::write(
        sources.join("sample.rs"),
        "pub fn sample() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() { assert!(m(r\"a{2\")); }\n",
    )
    .expect("source");
    let error = registry::generate(root.path(), &out).expect_err("an unterminated block must fail");
    assert!(error.to_string().contains("sample.rs"), "{error}");
    assert!(error.to_string().contains("unterminated"), "{error}");
    assert!(!out.join("rust_runtime.rs").exists(), "a truncated registry was written anyway");
}

/// The kernel half of the same rule: `build_kernel_inline` strips the kernel's test
/// blocks at build time, and losing one's end must fail rather than truncate the module.
#[test]
fn a_kernel_module_with_an_unterminated_test_block_fails_the_build() {
    let root = tempfile::tempdir().expect("tempdir");
    let sources = root.path().join("runtime/rs/src");
    let kernel = root.path().join("crates/almide-kernel/src");
    let out = root.path().join("out");
    for path in [&sources, &kernel, &out] { std::fs::create_dir_all(path).expect("mkdir"); }
    std::fs::write(sources.join("sample.rs"), "pub fn sample() {}\n").expect("source");
    std::fs::write(kernel.join("lib.rs"), "pub mod k;\n").expect("kernel lib");
    std::fs::write(
        kernel.join("k.rs"),
        "pub fn k() {}\n#[cfg(test)]\nmod tests {\n    fn t() { let s = \"{\"; }\n",
    )
    .expect("kernel module");
    let error = registry::generate(root.path(), &out).expect_err("an unterminated block must fail");
    assert!(error.to_string().contains("k.rs"), "{error}");
    assert!(error.to_string().contains("unterminated"), "{error}");
    assert!(!out.join("rust_runtime.rs").exists(), "a truncated registry was written anyway");
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
