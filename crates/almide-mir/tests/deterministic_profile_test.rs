//! The Wasm 3.0 deterministic-profile instruction-subset gate (C-210).
//!
//! The profile's determinism holds only if the emitted module stays off the
//! instructions whose results the core spec leaves implementation-defined or
//! host-interleaved: relaxed SIMD, threads/atomics, shared memory. The NaN
//! half of the conformance is behavioral and lives in the C-210 fixtures
//! (spec/wasm_cross/nan_canonical_*); this gate pins the INSTRUCTION half by
//! scanning the WAT of representative renders — a new pass that starts
//! emitting a forbidden family fails here, not in a divergence report.

fn render(source: &str) -> String {
    let modules = almide_mir::pipeline::bundled_self_modules(source);
    almide_mir::pipeline::try_render_wasm_source(source, &modules, false)
        .expect("representative program must render on the wasm leg")
}

/// Instruction families OUTSIDE the deterministic profile. Matched against
/// WAT text: every relaxed-SIMD mnemonic contains "relaxed", every
/// threads-proposal access contains "atomic", and a threads-visible memory
/// declares "shared". (Fixed 128-bit SIMD is IN the profile — when 0.54
/// re-ports the v128 unroll, this gate stays green by design.)
const FORBIDDEN: &[&str] = &["relaxed", "atomic", "shared"];

#[test]
fn emitted_wat_stays_inside_the_deterministic_profile() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Representative coverage: float arithmetic + bit observation (the NaN
    // fixtures), heap-heavy list/string/closure traffic, and the fan fold.
    let fixtures = [
        "spec/wasm_cross/nan_canonical_observation.almd",
        "spec/wasm_cross/nan_canonical_bytes_write.almd",
        "spec/wasm_cross/fan_race_mapper.almd",
        "spec/wasm_cross/fs_relative_path.almd",
    ];
    for rel in fixtures {
        let source = std::fs::read_to_string(root.join(rel)).expect("read fixture");
        // Scan CODE only: the renderer's `;;` prose mentions words like
        // "shared" (the shared-Dup primitive) that are not instructions.
        let wat: String = render(&source)
            .lines()
            .map(|l| l.split(";;").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        for family in FORBIDDEN {
            assert!(
                !wat.contains(family),
                "{rel}: emitted WAT contains `{family}` — outside the Wasm 3.0 \
                 deterministic profile (C-210); the render must not use relaxed \
                 SIMD, atomics, or shared memory"
            );
        }
    }
}
