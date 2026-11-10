//! These whole programs regressed when product-path preparation was added to
//! the native proof pipeline. A v0 fallback cannot satisfy this check.
mod common;

fn certifies(source: &'static str) -> String {
    std::thread::Builder::new().stack_size(256 << 20).spawn(move || {
        let (rust, certs) = almide_mir::pipeline::try_render_rust_source_certified(source)
            .expect("the corpus program must render with its native certificate");
        let mut ownership = 0;
        for (property, name, cert) in certs {
            if property != "ownership" { continue; }
            ownership += 1;
            for line in cert.lines() {
                assert_eq!(common::reject_reason(line), None, "{name}: {line}");
            }
        }
        assert!(ownership > 0, "the ownership witness set must not be empty");
        rust
    }).expect("spawn").join().expect("certification")
}

#[test]
fn nested_list_patterns_keep_their_guard_fallthrough() {
    certifies(include_str!("../../../spec/wasm_cross/list_pattern_in_option_payload.almd"));
}

#[test]
fn runtime_values_keep_their_declared_repr_in_variants() {
    compiles("runtime_variants", include_str!("../../../spec/wasm_cross/runtime_backed_variant_repr.almd"));
}

#[test]
fn generic_list_insert_preserves_float_bits() {
    certifies(include_str!("../../../spec/wasm_cross/stdlib_manufactured_nan_sign.almd"));
}

fn compiles(name: &str, source: &'static str) {
    let rust = certifies(source);
    let dir = std::env::temp_dir().join(format!("almide-native-corpus-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let src = dir.join("program.rs");
    std::fs::write(&src, rust).expect("write certified source");
    let result = std::process::Command::new("rustc").args(["--edition=2021", "--crate-name", "corpus"])
        .arg(&src).arg("-o").arg(dir.join("program")).output().expect("run rustc");
    std::fs::remove_dir_all(dir).expect("remove temp dir");
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
}

#[test]
fn literal_map_entries_convert_structural_record_keys() {
    compiles("record_keys", include_str!("../../../spec/wasm_cross/compound_eq.almd"));
}

#[test]
fn literal_map_entries_convert_float32_values() {
    compiles("float32_values", include_str!("../../../spec/wasm_cross/sized_display_matrix.almd"));
}
