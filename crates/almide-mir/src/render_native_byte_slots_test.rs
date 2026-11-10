use super::*;
use std::process::Command;

#[test]
fn byte_words_preserve_endianness_bits_and_reject_invalid_ranges() {
    let _guard = crate::render_native_rec::install(vec![crate::render_native_rec::RecDecl {
        name: crate::render_native_rec::BYTES_KEY.into(),
        alias: Some("Vec<u8>"),
        ..Default::default()
    }]);
    let mut st = AddrState::new(&MirFunction::default(), &Default::default());
    let (base, addr, index, value, dst) =
        (ValueId(0), ValueId(1), ValueId(2), ValueId(3), ValueId(4));
    st.addrs.insert(
        addr,
        Addr {
            base,
            off: i64::from(BLOCK_HEADER),
            idx: Some(Idx::Bytes(index)),
        },
    );
    let mut tys = BTreeMap::from([(base, NTy::Rec(0)), (index, NTy::I64), (value, NTy::I64)]);
    let dir = std::env::temp_dir().join(format!("almide-native-byte-words-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the temp dir");
    let mut source = format!(
        "{}\n{}\nfn main() {{ let {}: i64 = std::env::args().nth(1).unwrap().parse().unwrap();\n",
        crate::native_floor::floor_header(),
        crate::render_native::OOB_SHIM,
        var(index)
    );
    for width in [1, 2, 4, 8] {
        crate::render_native_bounds::reset();
        tys.insert(base, NTy::Rec(0));
        source.push_str(&format!(
            "{{ let mut {} = vec![0u8; 10]; let {}: i64 = -81985529216486896;\n",
            var(base),
            var(value)
        ));
        let store = Op::Prim {
            kind: PrimKind::Store { width },
            dst: None,
            args: vec![addr, value],
        };
        let load = Op::Prim {
            kind: PrimKind::Load { width },
            dst: Some(dst),
            args: vec![addr],
        };
        assert!(render(&store, &mut st, &mut tys, &mut source, 0)
            .unwrap()
            .unwrap());
        assert!(render(&load, &mut st, &mut tys, &mut source, 0)
            .unwrap()
            .unwrap());
        let expected = match width {
            1 => 0x10i64,
            2 => 0x3210,
            4 => 0x76543210,
            _ => -81985529216486896,
        };
        source.push_str(&format!("assert_eq!({}, {expected}i64); assert_eq!(&{}[{} as usize..{} as usize + {width}], &(-81985529216486896i64).to_le_bytes()[..{width}]); }}\n", var(dst), var(base), var(index), var(index)));
        // UTF-8 is read by byte offset, including inside a multibyte codepoint.
        tys.insert(base, NTy::StrRef);
        source.push_str(&format!("{{ let {}: &str = \"éabcdEFGH\";\n", var(base)));
        assert!(render(&load, &mut st, &mut tys, &mut source, 0)
            .unwrap()
            .unwrap());
        source.push_str(&format!("let expected = {}.as_bytes()[{} as usize..{} as usize + {width}].iter().enumerate().fold(0u64, |n, (i, b)| n | ((*b as u64) << (i * 8))); assert_eq!({}, expected as i64); }}\n", var(base), var(index), var(index), var(dst)));
    }
    source.push_str("}\n");
    let rs = dir.join("words.rs");
    let bin = dir.join("words");
    std::fs::write(&rs, source).expect("write the probe source");
    let compiled = Command::new("rustc")
        .arg(&rs)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run rustc");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    for index in [0i64, 2, -1, 3, 10, i64::MAX, i64::MIN] {
        let output = Command::new(&bin).arg(index.to_string()).output().expect("run the probe binary");
        assert_eq!(
            output.status.code(),
            Some(if matches!(index, 0 | 2) { 0 } else { 1 }),
            "index {index}: {:?}",
            output
        );
        assert_eq!(
            output.stderr.as_slice(),
            if matches!(index, 0 | 2) {
                b"".as_slice()
            } else {
                b"Error: index out of bounds\n"
            }
        );
    }
    tys.insert(base, NTy::RecRef(0));
    assert!(render(
        &Op::Prim {
            kind: PrimKind::Store { width: 1 },
            dst: None,
            args: vec![addr, value]
        },
        &mut st,
        &mut tys,
        &mut String::new(),
        0
    )
    .unwrap()
    .is_err());
    tys.insert(base, NTy::StrRef);
    assert!(render(
        &Op::Prim {
            kind: PrimKind::Store { width: 1 },
            dst: None,
            args: vec![addr, value]
        },
        &mut st,
        &mut tys,
        &mut String::new(),
        0
    )
    .unwrap()
    .is_err());
    std::fs::remove_dir_all(dir).expect("remove the temp dir");
}
