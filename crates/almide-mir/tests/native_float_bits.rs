//! The raw FloatBits primitive is a bit reinterpretation, including signed
//! zero and NaN payloads. Canonical NaN observation belongs to float.to_bits.

use almide_mir::render_native::{render_native_items, NativeSigKind as S, NativeSigs};
use almide_mir::{MirFunction, MirParam, MirProgram, Op, PrimKind, Repr, ScalarWidth, ValueId, PLACEHOLDER_LAYOUT};
use std::process::Command;

fn reinterpret(name: &str) -> MirFunction {
    MirFunction {
        name: name.into(),
        params: vec![MirParam { value: ValueId(0), repr: Repr::Scalar { width: ScalarWidth::Double } }],
        ops: vec![Op::Prim {
            kind: PrimKind::FloatBits,
            dst: Some(ValueId(1)),
            args: vec![ValueId(0)],
        }],
        ret: Some(ValueId(1)),
        ..Default::default()
    }
}

#[test]
fn raw_reinterpretations_preserve_ieee_edge_bits_when_executed() {
    let mut passthrough = reinterpret("passthrough");
    passthrough.ops.clear();
    passthrough.ret = Some(ValueId(0));
    let program = MirProgram {
        functions: vec![reinterpret("bits"), reinterpret("from_bits"), passthrough],
        ..Default::default()
    };
    let sigs: NativeSigs = [
        ("bits".into(), (vec![S::F64], Some(S::I64))),
        ("from_bits".into(), (vec![S::I64], Some(S::F64))),
        ("passthrough".into(), (vec![S::I64], Some(S::F64))),
    ].into();
    let mut source = render_native_items(&program, &sigs).expect("certified float reinterpret");
    source.push_str(r#"
fn main() {
    let patterns: [u64; 12] = [
        0, 1, 0x8000000000000000, 0x8000000000000001,
        1.5f64.to_bits(), (-1.5f64).to_bits(),
        f64::MAX.to_bits(), f64::MIN_POSITIVE.to_bits(),
        0x7ff0000000000000, 0xfff0000000000000,
        0x7ff0000000000001, 0xfff8123456789abc,
    ];
    for bits in patterns {
        let x = std::hint::black_box(f64::from_bits(bits));
        assert_eq!(almd_bits(x) as u64, bits);
        assert_eq!(almd_from_bits(std::hint::black_box(bits as i64)).to_bits(), bits);
        assert_eq!(almd_passthrough(std::hint::black_box(bits as i64)).to_bits(), bits);
    }
}
"#);
    let dir = std::env::temp_dir().join(format!("almide-native-float-bits-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("bits.rs");
    std::fs::write(&path, source).expect("write Rust");
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    for opt in ["0", "3"] {
        let binary = dir.join(format!("bits-{opt}"));
        let compiled = Command::new(&rustc)
            .args(["--edition=2021", "-C", &format!("opt-level={opt}")])
            .arg(&path).arg("-o").arg(&binary).output().expect("rustc");
        assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
        let output = Command::new(&binary).output().expect("run reinterpretations");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(output.stdout.is_empty());
    }
    std::fs::remove_dir_all(dir).expect("remove temp dir");
}

#[test]
fn raw_reinterpretation_refuses_heap_operands() {
    let mut f = reinterpret("invalid");
    f.params[0].repr = Repr::Ptr { layout: PLACEHOLDER_LAYOUT };
    let program = MirProgram { functions: vec![f], ..Default::default() };
    let sigs = [("invalid".into(), (vec![S::Str], Some(S::I64)))].into();
    let error = render_native_items(&program, &sigs).expect_err("heap is not a float slot");
    assert!(error.to_string().contains("float bits of a non-scalar"), "{error}");
}
