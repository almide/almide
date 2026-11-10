use super::*;

fn foreign(effect: bool) -> MirFunction {
    MirFunction {
        name: "foreign".into(), params: vec![], ret: None,
        ops: vec![Op::CallImport {
            target: ImportTarget::Native { kind: NativeExtern::Rust, is_effect: effect },
            dst: None, module: "host".into(), name: "invoke".into(),
            args: vec![], abi: vec![], result: None, result_abi: None,
        }],
        declared_caps: vec![], heap_slot_masks: Default::default(),
        owned_params: Default::default(), call_moves: Default::default(),
        aggregate_blocks: Default::default(),
    }
}

#[test]
fn foreign_effect_requires_both_capabilities_and_reaches_its_caller() {
    let mut callee = foreign(true);
    assert_eq!(crate::certificate::cap_witness_string(&callee), "|8 11");
    callee.declared_caps = vec![Capability::Foreign];
    assert_eq!(crate::certificate::cap_witness_string(&callee), "8|8 11");
    callee.declared_caps.push(Capability::ForeignEffect);
    assert_eq!(crate::certificate::cap_witness_string(&callee), "8 11|8 11");
    let mut caller = foreign(false);
    caller.name = "caller".into();
    caller.ops = vec![Op::CallFn { dst: None, name: "foreign".into(), args: vec![], result: None }];
    caller.declared_caps = vec![Capability::Foreign];
    let program = BTreeMap::from([(callee.name.clone(), callee), (caller.name.clone(), caller.clone())]);
    assert_eq!(crate::certificate::transitive_cap_witness_string(&caller, &program), "8|8 11");
}

#[test]
fn pure_foreign_decl_does_not_erase_the_foreign_boundary() {
    let mut callee = foreign(false);
    assert_eq!(crate::certificate::cap_witness_string(&callee), "|8");
    callee.declared_caps = vec![Capability::Foreign];
    assert_eq!(crate::certificate::cap_witness_string(&callee), "8|8");
}

#[test]
fn manifest_retains_the_boundary_without_granting_foreign_effects() {
    let mut pure = foreign(false);
    pure.declared_caps = vec![Capability::Foreign];
    crate::certificate::apply_manifest_caps(&mut pure, &["IO".into()]);
    assert_eq!(pure.declared_caps, vec![Capability::Foreign]);
    let mut effect = foreign(true);
    effect.declared_caps = vec![Capability::Foreign, Capability::ForeignEffect];
    crate::certificate::apply_manifest_caps(&mut effect, &[]);
    assert_eq!(crate::certificate::cap_witness_string(&effect), "8|8 11");
}
