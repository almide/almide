#[test]
fn effect_loop_inside_a_result_arm_has_a_representable_witness() {
    std::thread::Builder::new().stack_size(256 << 20).spawn(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../spec/stdlib/utf8_http_producers_test.almd");
        let source = std::fs::read_to_string(&path).expect("HTTP producer fixture");
        let super::FrontendOutcome::Ir(ir) = super::source_to_ir(&path, &source) else {
            panic!("prepare HTTP producer tests with resolved bundled modules");
        };
        let mut records = almide_mir::lower::build_record_layouts(&ir.type_decls);
        let mut variants = almide_mir::lower::build_variant_layouts(&ir.type_decls);
        for m in &ir.modules {
            records.extend(almide_mir::lower::build_record_layouts(&m.type_decls));
            variants.extend(almide_mir::lower::build_variant_layouts(&m.type_decls));
        }
        let mut tested = 0;
        for f in &ir.functions {
            if f.name.as_str().contains("http routes and middleware")
                || f.name.as_str().starts_with("__almd_cont_") {
                let mirs = almide_mir::lower::lower_function_all_with_layouts(
                    f, &Default::default(), &records, &variants
                ).expect("lower continuation");
                for mir in mirs {
                    almide_mir::verify_ownership(&mir).expect("ownership balance");
                    let (cert, poisoned) = almide_mir::certificate::ownership_certificate_with_poison(&mir);
                    assert!(!poisoned, "{}: {cert}", mir.name);
                    tested += 1;
                }
            }
        }
        assert!(tested > 0, "the original HTTP test must be checked");
    }).expect("spawn").join().expect("certification");
}
