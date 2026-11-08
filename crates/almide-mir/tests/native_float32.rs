//! Execute the Float32 primitive family at both optimization levels. Raw
//! bit observations preserve NaN payloads; arithmetic compares NaNs by class
//! because their payload is not specified by the floating-point operations.

use almide_mir::render_native::{render_native_items, NativeSigKind as S, NativeSigs};
use almide_mir::{FBinOp as B, FCmpOp as C, FUnOp as U, MirFunction, MirParam, MirProgram, Op, PrimKind as P, Repr, ScalarWidth, ValueId};
use std::process::Command;

fn primitive(name: &str, kind: P, arity: usize) -> MirFunction {
    MirFunction {
        name: name.into(),
        params: (0..arity).map(|n| MirParam {
            value: ValueId(n as u32), repr: Repr::Scalar { width: ScalarWidth::Double },
        }).collect(),
        ops: vec![Op::Prim {
            kind, dst: Some(ValueId(arity as u32)),
            args: (0..arity).map(|n| ValueId(n as u32)).collect(),
        }],
        ret: Some(ValueId(arity as u32)),
        ..Default::default()
    }
}

#[test]
fn float32_primitives_preserve_bits_rounding_and_comparisons() {
    let cases = [
        ("demote", P::F32Demote, vec![S::F64], S::I64),
        ("demote_bits", P::F32Demote, vec![S::I64], S::I64),
        ("promote", P::F32Promote, vec![S::I64], S::F64),
        ("from_int", P::IntToF32, vec![S::I64], S::I64),
        ("int_to_float", P::IntToFloat, vec![S::I64], S::F64),
        ("bits", P::F32Bits, vec![S::I64], S::I64),
        ("add", P::F32Bin(B::Add), vec![S::I64; 2], S::I64),
        ("sub", P::F32Bin(B::Sub), vec![S::I64; 2], S::I64),
        ("mul", P::F32Bin(B::Mul), vec![S::I64; 2], S::I64),
        ("div", P::F32Bin(B::Div), vec![S::I64; 2], S::I64),
        ("copysign", P::F32Bin(B::CopySign), vec![S::I64; 2], S::I64),
        ("neg", P::F32Un(U::Neg), vec![S::I64], S::I64),
        ("abs", P::F32Un(U::Abs), vec![S::I64], S::I64),
        ("sqrt", P::F32Un(U::Sqrt), vec![S::I64], S::I64),
        ("floor", P::F32Un(U::Floor), vec![S::I64], S::I64),
        ("ceil", P::F32Un(U::Ceil), vec![S::I64], S::I64),
        ("nearest", P::F32Un(U::Nearest), vec![S::I64], S::I64),
        ("lt", P::F32Cmp(C::Lt), vec![S::I64; 2], S::I64),
        ("le", P::F32Cmp(C::Le), vec![S::I64; 2], S::I64),
        ("gt", P::F32Cmp(C::Gt), vec![S::I64; 2], S::I64),
        ("ge", P::F32Cmp(C::Ge), vec![S::I64; 2], S::I64),
        ("eq", P::F32Cmp(C::Eq), vec![S::I64; 2], S::I64),
        ("ne", P::F32Cmp(C::Ne), vec![S::I64; 2], S::I64),
    ];
    let program = MirProgram {
        functions: cases.iter().map(|(name, kind, ps, _)| primitive(name, kind.clone(), ps.len())).collect(),
        ..Default::default()
    };
    let sigs: NativeSigs = cases.into_iter().map(|(n, _, ps, r)| (n.into(), (ps, Some(r)))).collect();
    let mut source = render_native_items(&program, &sigs).expect("Float32 family renders");
    source.push_str(r#"
fn same(bits: i64, want: f32) {
    assert!((0..=u32::MAX as i64).contains(&bits), "not zero extended: {bits}");
    let got = f32::from_bits(bits as u32);
    if want.is_nan() { assert!(got.is_nan()); }
    else { assert_eq!(got.to_bits(), want.to_bits()); }
}
fn check(a: u32, b: u32) {
    let x = std::hint::black_box(f32::from_bits(a));
    let y = std::hint::black_box(f32::from_bits(b));
    let ai = a as i64; let bi = b as i64;
    assert_eq!(almd_bits(ai), ai);
    assert_eq!(almd_neg(ai) as u32, a ^ 0x80000000);
    assert_eq!(almd_abs(ai) as u32, a & 0x7fffffff);
    assert_eq!(almd_copysign(ai, bi) as u32, (a & 0x7fffffff) | (b & 0x80000000));
    same(almd_add(ai, bi), x + y);
    same(almd_sub(ai, bi), x - y);
    same(almd_mul(ai, bi), x * y);
    same(almd_div(ai, bi), x / y);
    same(almd_sqrt(ai), x.sqrt());
    same(almd_floor(ai), x.floor());
    same(almd_ceil(ai), x.ceil());
    same(almd_nearest(ai), x.round_ties_even());
    for (got, want) in [(almd_lt(ai, bi), x < y), (almd_le(ai, bi), x <= y),
        (almd_gt(ai, bi), x > y), (almd_ge(ai, bi), x >= y),
        (almd_eq(ai, bi), x == y), (almd_ne(ai, bi), x != y)] {
        assert_eq!(got, want as i64);
    }
    let promoted = almd_promote(ai);
    if x.is_nan() { assert!(promoted.is_nan()); }
    else { assert_eq!(promoted.to_bits(), (x as f64).to_bits()); }
    same(almd_demote(promoted), x);
    same(almd_demote_bits(promoted.to_bits() as i64), x);
}
fn main() {
    let mut patterns = vec![0, 0x80000000, 0x7f800000, 0xff800000, 0x7f800001,
        0xffc12345, 0.5f32.to_bits(), (-0.5f32).to_bits(),
        1.5f32.to_bits(), 2.5f32.to_bits(), (-2.5f32).to_bits()];
    for e in 0..256u32 {
        for m in [0, 1, 2, 0x3fffff, 0x7fffff] {
            patterns.push((e << 23) | m);
            patterns.push(0x80000000 | (e << 23) | m);
        }
    }
    let mut seed = 0x9e3779b9u32;
    for _ in 0..10000 {
        seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5;
        patterns.push(seed);
    }
    for (i, &a) in patterns.iter().enumerate() {
        check(a, patterns[(i * 17 + 3) % patterns.len()]);
        check(a, a);
    }
    // Halfway and neighboring f64 values round to binary32 ties-to-even.
    let midpoint = 1.0f64 + 2.0f64.powi(-24);
    for x in [midpoint, f64::from_bits(midpoint.to_bits()-1),
        f64::from_bits(midpoint.to_bits()+1), f64::MAX, -f64::MAX,
        f64::MIN_POSITIVE, -f64::MIN_POSITIVE, -0.0] {
        same(almd_demote(std::hint::black_box(x)), x as f32);
    }
    // (2^62 + 2^38 + 1) is above an f32 midpoint, but its f64
    // conversion is exactly the midpoint: double-rounding loses the +1.
    for x in [0i64, 1, -1, i64::MIN, i64::MAX, (1i64<<62)+(1i64<<38)+1,
        -((1i64<<62)+(1i64<<38)+1), 16777217, -16777217] {
        same(almd_from_int(std::hint::black_box(x)), x as f32);
        assert_eq!(almd_int_to_float(std::hint::black_box(x)).to_bits(), (x as f64).to_bits());
    }
    assert_eq!(almd_nearest(2.5f32.to_bits() as i64), 2.0f32.to_bits() as i64);
    assert_eq!(almd_nearest((-0.5f32).to_bits() as i64), 0x80000000);
    assert_eq!(almd_from_int((1i64<<62)+(1i64<<38)+1), 0x5e800001);
}
"#);
    let dir = std::env::temp_dir().join(format!("almide-native-f32-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the temp dir");
    let path = dir.join("family.rs");
    std::fs::write(&path, source).expect("write the probe source");
    for opt in ["0", "3"] {
        let binary = dir.join(format!("family-{opt}"));
        let compiled = Command::new("rustc").args(["--edition=2021", "-C", &format!("opt-level={opt}")])
            .arg(&path).arg("-o").arg(&binary).output().expect("run rustc");
        assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
        let ran = Command::new(binary).output().expect("run the probe binary");
        assert!(ran.status.success(), "{}", String::from_utf8_lossy(&ran.stderr));
        assert!(ran.stdout.is_empty());
    }
    std::fs::remove_dir_all(dir).expect("remove the temp dir");
}

#[test]
fn unsupported_min_max_and_wrong_float_width_are_refused() {
    for kind in [P::F32Bin(B::Min), P::F32Bin(B::Max)] {
        let p = MirProgram { functions: vec![primitive("bad", kind, 2)], ..Default::default() };
        let sigs = [("bad".into(), (vec![S::I64; 2], Some(S::I64)))].into();
        assert!(render_native_items(&p, &sigs).unwrap_err().to_string().contains("min/max NaN semantics"));
    }
    for kind in [P::F32Bits, P::F32Promote, P::IntToF32, P::F32Un(U::Abs)] {
        let p = MirProgram { functions: vec![primitive("bad", kind, 1)], ..Default::default() };
        let sigs = [("bad".into(), (vec![S::F64], Some(S::I64)))].into();
        assert!(render_native_items(&p, &sigs).unwrap_err().to_string().contains("Float32 lane requires i64 bits"));
    }
}
